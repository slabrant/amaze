use std::collections::VecDeque;

use crate::goals::{
    self, contact_per_cell, crow_flies_score, loop_score, route_reach_score, walking_score,
    EXTRA_BRIDGE_CELLS_PER_CELL_FOR_FULL_MARKS, STEP_BASELINE, ZONE_CROSSINGS_PER_CELL_FOR_FULL_MARKS,
};
use crate::maze::{open_walls_of_passage_under, Maze, Orient, EAST, NORTH, NO_ZONE, SOUTH, WEST};
use crate::seeded_rng::Rng;

const MOST_CELLS_A_ZONE_JUMPS: usize = 3;
const NOT_IN_A_PIECE: usize = usize::MAX;
const TIDYING_ROUNDS: usize = 4;

#[derive(Clone, Copy)]
enum Way {
    Wall(usize),
    Bridge,
}

#[derive(Clone, Copy)]
struct WayOut {
    to: usize,
    way: Way,
}

struct Jump {
    from: usize,
    to: usize,
    under: Vec<usize>,
    sides: Vec<[usize; 2]>,
}

struct Pieces {
    of: Vec<usize>,
    cells: Vec<Vec<usize>>,
}

impl Pieces {
    fn add(&mut self, cells: Vec<usize>) {
        for &c in &cells {
            self.of[c] = self.cells.len();
        }
        self.cells.push(cells);
    }

    fn holds(&self, c: usize) -> bool {
        self.of[c] != NOT_IN_A_PIECE
    }
}

fn leaning(pulls: &[f64]) -> Vec<f64> {
    pulls.iter().map(|p| p.powf(goals::LEANING_STRENGTH)).collect()
}

fn step_leaning(values: &[f64]) -> Vec<f64> {
    let best = values.iter().copied().fold(f64::MIN, f64::max);
    values.iter().map(|v| ((v - best) * goals::STEP_LEANING).exp()).collect()
}

fn bridge_value(cells_over: usize, over_other_zones: usize) -> f64 {
    goals::LEVELS_PER_CELL * cells_over as f64 / 2.0
        + goals::ZONE_CROSSINGS * over_other_zones as f64 / ZONE_CROSSINGS_PER_CELL_FOR_FULL_MARKS
        + goals::LONGER_BRIDGES * cells_over.saturating_sub(1) as f64 / EXTRA_BRIDGE_CELLS_PER_CELL_FOR_FULL_MARKS
}

impl Maze {
    pub fn generate(rows: usize, cols: usize, rng: &mut Rng) -> Maze {
        loop {
            let m = Maze::build(rows, cols, rng);
            if !m.breaks_a_rule() {
                return m;
            }
        }
    }

    fn breaks_a_rule(&self) -> bool {
        let reached = self.reachable_from_start();
        let ends = [self.start, self.goal];
        let crossings_between_zones = |a: usize, b: usize| self.zone_crossings.contains(&(a.min(b), a.max(b)));
        let cell_breaks_a_rule = |c: usize| {
            let ways_out = self.degree(c);
            let r1 = if ends.contains(&c) { ways_out != 1 } else { ways_out == 1 };
            let r3 = self.links(c).any(|x| self.zone[x] != self.zone[c] && !crossings_between_zones(c, x));
            let r5 = ways_out > 0 && !reached[c];
            r1 || r3 || r5
        };
        let bridge_breaks_a_rule = |b: &crate::maze::Bridge| {
            b.spans.iter().any(|&x| self.open_walls[x] != open_walls_of_passage_under(b.orientation) || self.degree(x) != 2)
        };
        self.start == self.goal || (0..self.zone.len()).any(cell_breaks_a_rule) || self.bridges.iter().any(bridge_breaks_a_rule)
    }

    fn build(rows: usize, cols: usize, rng: &mut Rng) -> Maze {
        let target = goals::target_zone_count(rows, cols);
        let zone_counts: Vec<usize> = (target.saturating_sub(1).max(1)..=target + 1).collect();
        let pulls: Vec<f64> =
            zone_counts.iter().map(|&z| STEP_BASELINE + goals::ZONE_COUNT * goals::zone_count_score(rows, cols, z)).collect();
        let zones = zone_counts[rng.index_by_weight(&leaning(&pulls))];
        let route = Maze::empty(rows, cols, 1).route_through_a_plain_tree(rng);
        let mut m = Maze::empty(rows, cols, zones.min(route.len()));
        let mut pieces = Pieces { of: vec![NOT_IN_A_PIECE; m.zone.len()], cells: Vec::new() };
        m.lay_route_in_uneven_stretches(&route, rng);
        let mut jumps = m.grow_zones_toward_each_other(&route, &mut pieces, rng);
        loop {
            m.move_cells_with_one_neighbour_in_their_zone(&route, &pieces);
            let before = jumps.len();
            m.undo_jumps_that_would_leave_a_dead_end(&mut jumps, &route, &pieces);
            pieces = m.pieces_of(&jumps);
            if jumps.len() == before {
                break;
            }
        }
        let mut in_tree = vec![false; m.zone.len()];
        for &c in &route {
            in_tree[c] = true;
        }
        for piece in &pieces.cells {
            if piece.iter().any(|&c| in_tree[c]) {
                for &c in piece {
                    in_tree[c] = true;
                }
            }
        }
        let mut zone_order: Vec<u8> = (0..m.zone_count as u8).collect();
        rng.shuffle(&mut zone_order);
        for z in zone_order {
            let growing = (0..m.zone.len()).filter(|&c| in_tree[c] && m.zone[c] == z).collect();
            m.grow_tree(&mut in_tree, growing, true, &pieces, rng);
        }
        m.take_back_what_the_route_never_reached(route[0]);
        m.choose_start_and_finish_among_dead_ends(rng);
        for _ in 0..TIDYING_ROUNDS {
            m.join_or_fill_in_every_dead_end(rng);
            if !m.reattach_or_fill_in_what_is_cut_off(rng) {
                break;
            }
        }
        m
    }

    fn under_a_bridge(&self, c: usize) -> bool {
        self.bridges_overhead[c] != 0
    }

    fn passage_a_bridge_may_cross(&self, c: usize, bridge: Orient) -> bool {
        self.open_walls[c] == open_walls_of_passage_under(bridge) && !self.under_a_bridge(c) && self.bridged_to[c].is_empty()
    }

    fn ways_out(&self, c: usize, may_reach: impl Fn(usize) -> bool) -> Vec<WayOut> {
        let mut out = Vec::new();
        if self.under_a_bridge(c) {
            return out;
        }
        for d in 0..4 {
            let Some(next) = self.step(c, d) else { continue };
            if self.is_open(c, d) || self.under_a_bridge(next) {
                continue;
            }
            if may_reach(next) {
                out.push(WayOut { to: next, way: Way::Wall(d) });
                continue;
            }
            let mut x = next;
            while self.passage_a_bridge_may_cross(x, Orient::of_direction(d)) {
                let Some(beyond) = self.step(x, d) else { break };
                if may_reach(beyond) && !self.under_a_bridge(beyond) {
                    out.push(WayOut { to: beyond, way: Way::Bridge });
                    break;
                }
                x = beyond;
            }
        }
        out
    }

    fn cells_passed_over(&self, from: usize, w: WayOut) -> Vec<usize> {
        match w.way {
            Way::Wall(_) => Vec::new(),
            Way::Bridge => self.cells_between(from, w.to),
        }
    }

    fn take(&mut self, from: usize, w: WayOut) {
        match w.way {
            Way::Wall(d) => self.open_wall(from, d),
            Way::Bridge => self.lay_bridge(from, w.to),
        }
    }

    fn new_fork(&self, c: usize) -> f64 {
        if self.degree(c) == 2 { 1.0 } else { 0.0 }
    }

    fn bridge_value_of(&self, from: usize, w: WayOut) -> f64 {
        let over = self.cells_passed_over(from, w);
        let over_other_zones = over.iter().filter(|&&x| self.zone[x] != self.zone[from]).count();
        bridge_value(over.len(), over_other_zones)
    }

    fn value_of_growing(&self, from: usize, w: WayOut) -> f64 {
        goals::LEVELS_PER_CELL / 2.0 + goals::MANY_FORKS * self.new_fork(from) + self.bridge_value_of(from, w)
    }

    fn value_of_joining(&self, from: usize, w: WayOut, walking_from_here: &[usize], mean_loop: f64) -> f64 {
        let round = walking_from_here[w.to] + self.cells_walked_between(from, w.to);
        goals::LONG_SHORTEST_LOOPS * (loop_score(self, round) - mean_loop) * round as f64
            + goals::MANY_FORKS * self.new_fork(w.to)
            + self.bridge_value_of(from, w)
    }

    fn grow_tree(&mut self, in_tree: &mut [bool], mut growing: Vec<usize>, may_bridge: bool, pieces: &Pieces, rng: &mut Rng) {
        loop {
            let mut options: Vec<(usize, WayOut)> = Vec::new();
            growing.retain(|&c| {
                let ways = self.ways_out(c, |x| !in_tree[x] && self.zone[x] == self.zone[c]);
                let before = options.len();
                options.extend(ways.into_iter().filter(|w| may_bridge || matches!(w.way, Way::Wall(_))).map(|w| (c, w)));
                options.len() > before
            });
            if options.is_empty() {
                return;
            }
            let newest = *growing.last().unwrap();
            let can_extend = options.iter().any(|&(c, _)| c == newest);
            let can_branch = options.iter().any(|&(c, _)| c != newest);
            let extend = can_extend && !(can_branch && rng.from_zero_to_one() < goals::CHANCE_TO_BRANCH);
            options.retain(|&(c, _)| (c == newest) == extend);
            let values: Vec<f64> = options.iter().map(|&(c, w)| self.value_of_growing(c, w)).collect();
            let (c, w) = options[rng.index_by_weight(&step_leaning(&values))];
            self.take(c, w);
            let joined = if pieces.holds(w.to) { pieces.cells[pieces.of[w.to]].clone() } else { vec![w.to] };
            for x in joined {
                if !in_tree[x] {
                    in_tree[x] = true;
                    growing.push(x);
                }
            }
            if let Some(i) = growing.iter().position(|&x| x == w.to) {
                let landed = growing.remove(i);
                growing.push(landed);
            }
        }
    }

    fn tree_path(&self, from: usize, to: usize) -> Vec<usize> {
        let walking = self.walking_distances_from(to);
        let mut path = vec![from];
        while *path.last().unwrap() != to {
            let here = *path.last().unwrap();
            path.push(self.links(here).find(|&x| walking[x] < walking[here]).unwrap());
        }
        path
    }

    fn route_through_a_plain_tree(mut self, rng: &mut Rng) -> Vec<usize> {
        let mut in_tree = vec![false; self.zone.len()];
        let first = rng.below(self.zone.len());
        in_tree[first] = true;
        let no_pieces = Pieces { of: vec![NOT_IN_A_PIECE; self.zone.len()], cells: Vec::new() };
        self.grow_tree(&mut in_tree, vec![first], false, &no_pieces, rng);
        let ends: Vec<usize> = (0..self.zone.len()).filter(|&c| self.degree(c) <= 1).collect();
        let mut pairs = Vec::new();
        let mut pulls = Vec::new();
        for (i, &a) in ends.iter().enumerate() {
            let walking = self.walking_distances_from(a);
            for &b in &ends[i + 1..] {
                let reach = route_reach_score(&self, &self.tree_path(a, b));
                pairs.push((a, b));
                pulls.push(
                    STEP_BASELINE
                        + goals::WALKING_DISTANCE * walking_score(&self, walking[b])
                        + goals::ROUTE_REACH * reach
                        + goals::CROW_FLIES_CLOSENESS * crow_flies_score(&self, a, b),
                );
            }
        }
        let (start, goal) = if pairs.is_empty() { (first, first) } else { pairs[rng.index_by_weight(&leaning(&pulls))] };
        self.tree_path(start, goal)
    }

    fn lay_route_in_uneven_stretches(&mut self, route: &[usize], rng: &mut Rng) {
        let mut cuts: Vec<usize> = Vec::new();
        while cuts.len() + 1 < self.zone_count {
            let options: Vec<usize> = (1..route.len()).filter(|i| !cuts.contains(i)).collect();
            let pulls: Vec<f64> = options
                .iter()
                .map(|&i| {
                    let choke_points: Vec<(usize, usize)> = cuts.iter().chain([&i]).map(|&cut| (route[cut - 1], route[cut])).collect();
                    STEP_BASELINE + goals::CHOKE_POINT_SPREAD * goals::choke_point_spread_score(self, &choke_points)
                })
                .collect();
            cuts.push(options[rng.index_by_weight(&leaning(&pulls))]);
        }
        for (i, &c) in route.iter().enumerate() {
            self.zone[c] = cuts.iter().filter(|&&cut| cut <= i).count() as u8;
        }
        for w in route.windows(2) {
            self.open_wall(w[0], self.direction_toward(w[0], w[1]));
            if self.zone[w[0]] != self.zone[w[1]] {
                self.zone_crossings.push((w[0].min(w[1]), w[0].max(w[1])));
            }
        }
    }

    fn choose_start_and_finish_among_dead_ends(&mut self, rng: &mut Rng) {
        let last_zone = self.zone_count as u8 - 1;
        let dead_ends_in = |z: u8| (0..self.zone.len()).filter(|&c| self.zone[c] == z && self.degree(c) == 1).collect::<Vec<_>>();
        let (starts, finishes) = (dead_ends_in(0), dead_ends_in(last_zone));
        let mut pairs = Vec::new();
        let mut pulls = Vec::new();
        for &a in &starts {
            let walking = self.walking_distances_from(a);
            for &b in finishes.iter().filter(|&&b| b != a) {
                pairs.push((a, b));
                pulls.push(
                    STEP_BASELINE
                        + goals::WALKING_DISTANCE * walking_score(self, walking[b])
                        + goals::ROUTE_REACH * route_reach_score(self, &self.tree_path(a, b))
                        + goals::CROW_FLIES_CLOSENESS * crow_flies_score(self, a, b),
                );
            }
        }
        (self.start, self.goal) = pairs[rng.index_by_weight(&leaning(&pulls))];
    }

    fn zone_contact_counts(&self) -> (Vec<usize>, Vec<usize>) {
        let mut contact = vec![0usize; self.zone_count];
        let mut cells = vec![0usize; self.zone_count];
        for c in (0..self.zone.len()).filter(|&c| self.zone[c] != NO_ZONE) {
            let z = self.zone[c] as usize;
            cells[z] += 1;
            contact[z] += (0..4).filter_map(|d| self.step(c, d)).filter(|&x| self.zone[x] != NO_ZONE && self.zone[x] as usize != z).count();
        }
        for b in &self.bridges {
            let over = self.zone[b.top_or_left] as usize;
            for &x in b.spans.iter().filter(|&&x| self.zone[x] as usize != over) {
                contact[over] += 1;
                contact[self.zone[x] as usize] += 1;
            }
        }
        (contact, cells)
    }

    fn contact_after(&self, counts: &(Vec<usize>, Vec<usize>), claims: &[(usize, u8)], crossings: &[(u8, u8)]) -> f64 {
        let (mut contact, mut cells) = counts.clone();
        let zone_after = |x: usize| claims.iter().find(|&&(c, _)| c == x).map_or(self.zone[x], |&(_, z)| z);
        for &(c, z) in claims {
            cells[z as usize] += 1;
            for x in (0..4).filter_map(|d| self.step(c, d)) {
                let w = zone_after(x);
                if w != NO_ZONE && w != z {
                    contact[z as usize] += 1;
                    if self.zone[x] != NO_ZONE {
                        contact[w as usize] += 1;
                    }
                }
            }
        }
        for &(over, under) in crossings {
            contact[over as usize] += 1;
            contact[under as usize] += 1;
        }
        contact_per_cell(&contact, &cells)
    }

    fn jump_from(&self, x: usize, d: usize, cells_over: usize, route: &[usize], pieces: &Pieces) -> Option<Jump> {
        let z = self.zone[x];
        let across = if d == WEST || d == EAST { [NORTH, SOUTH] } else { [WEST, EAST] };
        let mut under = Vec::new();
        let mut sides = Vec::new();
        let mut c = x;
        for _ in 0..cells_over {
            c = self.step(c, d)?;
            let w = self.zone[c];
            if w == NO_ZONE || w == z || route.contains(&c) || pieces.holds(c) || self.under_a_bridge(c) {
                return None;
            }
            let has_another_of_its_zone_beside =
                |s: usize| (0..4).filter_map(|e| self.step(s, e)).any(|q| q != c && (self.zone[q] == w || self.zone[q] == NO_ZONE) && !self.under_a_bridge(q) && !pieces.holds(q));
            let side = |p: usize| {
                self.step(c, p).filter(|&s| {
                    (self.zone[s] == NO_ZONE || self.zone[s] == w)
                        && !route.contains(&s)
                        && !pieces.holds(s)
                        && !self.under_a_bridge(s)
                        && has_another_of_its_zone_beside(s)
                })
            };
            under.push(c);
            sides.push([side(across[0])?, side(across[1])?]);
        }
        let to = self.step(c, d).filter(|&y| self.zone[y] == NO_ZONE)?;
        Some(Jump { from: x, to, under, sides })
    }

    fn take_jump(&mut self, j: &Jump, pieces: &mut Pieces) {
        let z = self.zone[j.from];
        self.zone[j.to] = z;
        for (&c, &[s0, s1]) in j.under.iter().zip(&j.sides) {
            self.zone[s0] = self.zone[c];
            self.zone[s1] = self.zone[c];
            self.open_wall(c, self.direction_toward(c, s0));
            self.open_wall(c, self.direction_toward(c, s1));
            pieces.add(vec![s0, c, s1]);
        }
        self.lay_bridge(j.from, j.to);
        if !pieces.holds(j.from) {
            pieces.add(vec![j.from, j.to]);
        } else {
            let p = pieces.of[j.from];
            pieces.of[j.to] = p;
            pieces.cells[p].push(j.to);
        }
    }

    fn pieces_of(&self, jumps: &[Jump]) -> Pieces {
        let mut pieces = Pieces { of: vec![NOT_IN_A_PIECE; self.zone.len()], cells: Vec::new() };
        for j in jumps {
            for (&c, &[s0, s1]) in j.under.iter().zip(&j.sides) {
                pieces.add(vec![s0, c, s1]);
            }
            if !pieces.holds(j.from) {
                pieces.add(vec![j.from, j.to]);
            } else {
                let p = pieces.of[j.from];
                pieces.of[j.to] = p;
                pieces.cells[p].push(j.to);
            }
        }
        pieces
    }

    fn undo_jumps_that_would_leave_a_dead_end(&mut self, jumps: &mut Vec<Jump>, route: &[usize], pieces: &Pieces) {
        let has_another_of_its_zone_beside =
            |m: &Maze, s: usize| (0..4).filter_map(|d| m.step(s, d)).any(|q| m.zone[q] == m.zone[s] && !m.under_a_bridge(q));
        let mut i = 0;
        while i < jumps.len() {
            let ends: Vec<usize> = jumps[i].sides.iter().flatten().chain([&jumps[i].to]).copied().collect();
            for &end in &ends {
                if has_another_of_its_zone_beside(self, end) {
                    continue;
                }
                let free = |q: usize| !self.under_a_bridge(q) && !route.contains(&q) && !pieces.holds(q) && self.zone[q] != NO_ZONE;
                let other_partners = |q: usize| (0..4).filter_map(|d| self.step(q, d)).filter(|&x| x != end && self.zone[x] == self.zone[end] && !self.under_a_bridge(x)).count();
                let handed_over = (0..4).filter_map(|d| self.step(end, d)).filter(|&q| free(q) && other_partners(q) > 0).max_by_key(|&q| other_partners(q));
                if let Some(q) = handed_over {
                    self.zone[q] = self.zone[end];
                }
            }
            let usable = ends.iter().all(|&s| has_another_of_its_zone_beside(self, s));
            if usable {
                i += 1;
                continue;
            }
            let j = jumps.swap_remove(i);
            for (&c, &[s0, s1]) in j.under.iter().zip(&j.sides) {
                self.close_wall(c, self.direction_toward(c, s0));
                self.close_wall(c, self.direction_toward(c, s1));
            }
            self.remove_link(j.from, j.to);
        }
    }

    fn jump_ends_waiting_for_a_partner(&self, jumps: &[Jump]) -> Vec<(usize, Vec<usize>, f64)> {
        let mut waiting = Vec::new();
        for j in jumps {
            let value = bridge_value(j.under.len(), j.under.len());
            for &end in j.sides.iter().flatten().chain([&j.to]) {
                let beside: Vec<usize> = (0..4).filter_map(|d| self.step(end, d)).filter(|&q| !self.under_a_bridge(q)).collect();
                if beside.iter().any(|&q| self.zone[q] == self.zone[end]) {
                    continue;
                }
                let partners = beside.into_iter().filter(|&q| self.zone[q] == NO_ZONE).collect();
                waiting.push((end, partners, value));
            }
        }
        waiting
    }

    fn grow_zones_toward_each_other(&mut self, route: &[usize], pieces: &mut Pieces, rng: &mut Rng) -> Vec<Jump> {
        let n = self.zone.len() as f64;
        let mut taken = Vec::new();
        loop {
            let counts = self.zone_contact_counts();
            let weakest = contact_per_cell(&counts.0, &counts.1);
            let mut claims: Vec<(usize, u8)> = Vec::new();
            let mut jumps: Vec<Jump> = Vec::new();
            let mut values: Vec<f64> = Vec::new();
            let waiting = self.jump_ends_waiting_for_a_partner(&taken);
            for y in (0..self.zone.len()).filter(|&y| self.zone[y] == NO_ZONE) {
                let mut beside: Vec<u8> = (0..4).filter_map(|d| self.step(y, d)).map(|x| self.zone[x]).filter(|&z| z != NO_ZONE).collect();
                beside.sort();
                beside.dedup();
                for z in beside {
                    let mut value = goals::ZONE_CONTACT * n * (self.contact_after(&counts, &[(y, z)], &[]) - weakest);
                    for &(end, ref partners, jump_value) in waiting.iter().filter(|(_, partners, _)| partners.contains(&y)) {
                        if self.zone[end] == z {
                            value += jump_value;
                        } else if partners.len() == 1 {
                            value -= jump_value;
                        }
                    }
                    claims.push((y, z));
                    values.push(value);
                }
            }
            for x in (0..self.zone.len()).filter(|&x| self.zone[x] != NO_ZONE && !self.under_a_bridge(x)) {
                if pieces.holds(x) && self.bridged_to[x].len() > 0 && !route.contains(&x) {
                    continue;
                }
                for d in 0..4 {
                    for cells_over in 1..=MOST_CELLS_A_ZONE_JUMPS {
                        let Some(j) = self.jump_from(x, d, cells_over, route, pieces) else { continue };
                        let z = self.zone[x];
                        let mut new_cells = vec![(j.to, z)];
                        for (&c, sides) in j.under.iter().zip(&j.sides) {
                            new_cells.extend(sides.iter().filter(|&&s| self.zone[s] == NO_ZONE).map(|&s| (s, self.zone[c])));
                        }
                        let crossings: Vec<(u8, u8)> = j.under.iter().map(|&c| (z, self.zone[c])).collect();
                        let contact = self.contact_after(&counts, &new_cells, &crossings);
                        values.push(goals::ZONE_CONTACT * n * (contact - weakest) + bridge_value(j.under.len(), j.under.len()));
                        jumps.push(j);
                    }
                }
            }
            if values.is_empty() {
                return taken;
            }
            let i = rng.index_by_weight(&step_leaning(&values));
            if i < claims.len() {
                let (y, z) = claims[i];
                self.zone[y] = z;
            } else {
                let j = jumps.swap_remove(i - claims.len());
                self.take_jump(&j, pieces);
                taken.push(j);
            }
        }
    }

    fn move_cells_with_one_neighbour_in_their_zone(&mut self, route: &[usize], pieces: &Pieces) {
        let mut moved = true;
        while moved {
            moved = false;
            for c in (0..self.zone.len()).filter(|&c| !route.contains(&c) && !pieces.holds(c)) {
                let beside: Vec<u8> = (0..4).filter_map(|d| self.step(c, d)).map(|x| self.zone[x]).collect();
                let in_zone = |z: u8| beside.iter().filter(|&&y| y == z).count();
                if in_zone(self.zone[c]) < 2 {
                    if let Some(&better) = beside.iter().filter(|&&z| in_zone(z) >= 2).max() {
                        self.zone[c] = better;
                        moved = true;
                    }
                }
            }
        }
    }

    fn take_back_what_the_route_never_reached(&mut self, on_the_route: usize) {
        loop {
            let mut reached = vec![false; self.zone.len()];
            reached[on_the_route] = true;
            let mut queue = VecDeque::from([on_the_route]);
            while let Some(x) = queue.pop_front() {
                for y in self.links(x).collect::<Vec<_>>() {
                    if !reached[y] {
                        reached[y] = true;
                        queue.push_back(y);
                    }
                }
            }
            let stranded_bridge = self.bridges.iter().position(|b| {
                [b.top_or_left, b.bottom_or_right].iter().chain(&b.spans).any(|&c| !reached[c])
            });
            if let Some(i) = stranded_bridge {
                let b = &self.bridges[i];
                let (a, c) = (b.top_or_left, b.bottom_or_right);
                self.remove_link(a, c);
                continue;
            }
            let mut took_back = false;
            for c in (0..self.zone.len()).filter(|&c| !reached[c]) {
                for x in self.links(c).collect::<Vec<_>>() {
                    self.remove_link(c, x);
                    took_back = true;
                }
            }
            if !took_back {
                return;
            }
        }
    }

    fn reattach_or_fill_in_what_is_cut_off(&mut self, rng: &mut Rng) -> bool {
        let mut in_tree = self.reachable_from_start();
        let cut_off: Vec<usize> = (0..self.zone.len()).filter(|&c| !in_tree[c] && !self.is_solid(c)).collect();
        if cut_off.is_empty() {
            return false;
        }
        let mut pieces = Pieces { of: vec![NOT_IN_A_PIECE; self.zone.len()], cells: Vec::new() };
        for &c in &cut_off {
            if pieces.holds(c) {
                continue;
            }
            let mut piece = vec![c];
            let mut i = 0;
            while i < piece.len() {
                for y in self.links(piece[i]).collect::<Vec<_>>() {
                    if !piece.contains(&y) {
                        piece.push(y);
                    }
                }
                i += 1;
            }
            pieces.add(piece);
        }
        let ends = [self.start, self.goal];
        let growing = (0..self.zone.len()).filter(|&c| in_tree[c] && !ends.contains(&c)).collect();
        for piece in &pieces.cells {
            for &c in piece {
                in_tree[c] = false;
            }
        }
        self.grow_tree(&mut in_tree, growing, true, &pieces, rng);
        self.take_back_what_the_route_never_reached(self.start);
        true
    }

    fn join_or_fill_in_every_dead_end(&mut self, rng: &mut Rng) {
        let is_dead_end = |m: &Maze, c: usize| m.degree(c) == 1 && c != m.start && c != m.goal;
        let mut dead_ends: Vec<usize> = (0..self.zone.len()).filter(|&c| is_dead_end(self, c)).collect();
        rng.shuffle(&mut dead_ends);
        let mut queue = VecDeque::from(dead_ends);
        let (mut loops_closed, mut loop_scores) = (0usize, 0.0);
        while let Some(c) = queue.pop_front() {
            if !is_dead_end(self, c) {
                continue;
            }
            let joinable = |x: usize| self.zone[x] == self.zone[c] && !self.is_solid(x) && x != self.start && x != self.goal;
            let ways = self.ways_out(c, joinable);
            if ways.is_empty() {
                let only_link = self.links(c).next().unwrap();
                self.remove_link(c, only_link);
                let mut touched = vec![only_link];
                while let Some(i) = self.bridges.iter().position(|b| b.spans.iter().any(|&x| self.degree(x) != 2)) {
                    let b = &self.bridges[i];
                    let (a, z) = (b.top_or_left, b.bottom_or_right);
                    touched.extend(b.spans.iter().copied().chain([a, z]));
                    self.remove_link(a, z);
                }
                queue.extend(touched.into_iter().filter(|&x| is_dead_end(self, x)));
                continue;
            }
            let walking_from_here = self.walking_distances_from(c);
            let mean_loop = if loops_closed == 0 { 0.5 } else { loop_scores / loops_closed as f64 };
            let values: Vec<f64> = ways.iter().map(|&w| self.value_of_joining(c, w, &walking_from_here, mean_loop)).collect();
            let w = ways[rng.index_by_weight(&step_leaning(&values))];
            let round = walking_from_here[w.to] + self.cells_walked_between(c, w.to);
            loops_closed += 1;
            loop_scores += loop_score(self, round);
            self.take(c, w);
        }
    }
}
