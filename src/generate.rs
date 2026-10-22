use std::collections::HashMap;

use crate::goals::{
    self, contact_per_cell, crow_flies_score, loop_score, route_reach_score, walking_score,
    EXTRA_BRIDGE_CELLS_PER_CELL_FOR_FULL_MARKS, STEP_BASELINE, ZONE_CROSSINGS_PER_CELL_FOR_FULL_MARKS,
};
use crate::maze::{open_walls_of_passage_under, Maze, Orient, EAST, NORTH, NO_ZONE, SOUTH, WEST};
use crate::seeded_rng::Rng;

#[derive(Clone, Copy, PartialEq)]
enum Ground {
    Free,
    Crossable,
}

struct Leg {
    cells: Vec<(usize, Ground)>,
}

struct Detour {
    a: usize,
    b: usize,
    out: usize,
    legs: [Leg; 2],
}

fn leaning(pulls: &[f64]) -> Vec<f64> {
    pulls.iter().map(|p| p.powf(goals::LEANING_STRENGTH)).collect()
}

fn step_leaning(values: &[f64]) -> Vec<f64> {
    let best = values.iter().copied().fold(f64::MIN, f64::max);
    values.iter().map(|v| ((v - best) * goals::STEP_LEANING).exp()).collect()
}

fn across(d: usize) -> [usize; 2] {
    if d == WEST || d == EAST { [NORTH, SOUTH] } else { [WEST, EAST] }
}

fn bridge_value(cells_over: usize, over_other_zones: usize, extra_cells: usize) -> f64 {
    goals::LEVELS_PER_CELL * cells_over as f64 / 2.0
        + goals::ZONE_CROSSINGS * over_other_zones as f64 / ZONE_CROSSINGS_PER_CELL_FOR_FULL_MARKS
        + goals::LONGER_BRIDGES * extra_cells as f64 / EXTRA_BRIDGE_CELLS_PER_CELL_FOR_FULL_MARKS
}

impl Maze {
    pub fn generate(rows: usize, cols: usize, rng: &mut Rng) -> Maze {
        let target = goals::target_zone_count(rows, cols);
        let zone_counts: Vec<usize> = (target.saturating_sub(1).max(1)..=target + 1).collect();
        let pulls: Vec<f64> =
            zone_counts.iter().map(|&z| STEP_BASELINE + goals::ZONE_COUNT * goals::zone_count_score(rows, cols, z)).collect();
        let zones = zone_counts[rng.index_by_weight(&leaning(&pulls))];
        let route = Maze::empty(rows, cols, 1).route_through_a_plain_tree(rng);
        let mut m = Maze::empty(rows, cols, zones.min(route.len()));
        m.lay_route_in_uneven_stretches(&route, rng);
        m.start = route[0];
        m.goal = *route.last().unwrap();
        m.grow_by_detours(rng);
        m.add_loops(rng);
        m
    }

    pub fn breaks_a_rule(&self) -> bool {
        let reached = self.reachable_from_start();
        let ends = [self.start, self.goal];
        let crossing = |a: usize, b: usize| self.zone_crossings.contains(&(a.min(b), a.max(b)));
        let cell_breaks_a_rule = |c: usize| {
            let ways_out = self.degree(c);
            let r1 = if ends.contains(&c) { ways_out != 1 } else { ways_out == 1 };
            let r3 = self.links(c).any(|x| self.zone[x] != self.zone[c] && !crossing(c, x));
            let r5 = ways_out > 0 && !reached[c];
            r1 || r3 || r5
        };
        let bridge_breaks_a_rule = |b: &crate::maze::Bridge| {
            b.spans.iter().any(|&x| self.open_walls[x] != open_walls_of_passage_under(b.orientation) || self.degree(x) != 2)
        };
        self.start == self.goal || (0..self.zone.len()).any(cell_breaks_a_rule) || self.bridges.iter().any(bridge_breaks_a_rule)
    }

    fn under_a_bridge(&self, c: usize) -> bool {
        self.bridges_overhead[c] != 0
    }

    fn passage_a_bridge_may_cross(&self, c: usize, bridge: Orient) -> bool {
        self.open_walls[c] == open_walls_of_passage_under(bridge) && !self.under_a_bridge(c) && self.bridged_to[c].is_empty()
    }

    fn may_gain_a_way_out(&self, c: usize) -> bool {
        !self.is_solid(c) && !self.under_a_bridge(c) && c != self.start && c != self.goal
    }

    fn route_through_a_plain_tree(mut self, rng: &mut Rng) -> Vec<usize> {
        let n = self.zone.len();
        let first = rng.below(n);
        let mut in_tree = vec![false; n];
        in_tree[first] = true;
        let mut growing = vec![first];
        while !growing.is_empty() {
            let i = if rng.from_zero_to_one() < goals::CHANCE_TO_BRANCH { rng.below(growing.len()) } else { growing.len() - 1 };
            let c = growing[i];
            let ways: Vec<usize> = (0..4).filter(|&d| self.step(c, d).is_some_and(|x| !in_tree[x])).collect();
            if ways.is_empty() {
                growing.remove(i);
                continue;
            }
            let d = ways[rng.below(ways.len())];
            let x = self.step(c, d).unwrap();
            self.open_wall(c, d);
            in_tree[x] = true;
            growing.push(x);
        }
        let ends: Vec<usize> = (0..n).filter(|&c| self.degree(c) <= 1).collect();
        let mut pairs = Vec::new();
        let mut pulls = Vec::new();
        for (i, &a) in ends.iter().enumerate() {
            let walking = self.walking_distances_from(a);
            for &b in &ends[i + 1..] {
                pairs.push((a, b));
                pulls.push(
                    STEP_BASELINE
                        + goals::WALKING_DISTANCE * walking_score(&self, walking[b])
                        + goals::ROUTE_REACH * route_reach_score(&self, &self.tree_path(a, b))
                        + goals::CROW_FLIES_CLOSENESS * crow_flies_score(&self, a, b),
                );
            }
        }
        let (start, goal) = pairs[rng.index_by_weight(&leaning(&pulls))];
        self.tree_path(start, goal)
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

    fn leg(&self, from: usize, out: usize, depth: usize) -> Option<Leg> {
        let mut cells = Vec::new();
        let mut c = from;
        for _ in 0..depth {
            c = self.step(c, out)?;
            if self.zone[c] == NO_ZONE {
                cells.push((c, Ground::Free));
            } else if self.passage_a_bridge_may_cross(c, Orient::of_direction(out)) {
                cells.push((c, Ground::Crossable));
            } else {
                return None;
            }
        }
        (cells.last()?.1 == Ground::Free).then_some(Leg { cells })
    }

    fn detours(&self) -> Vec<Detour> {
        let mut out = Vec::new();
        for a in (0..self.zone.len()).filter(|&a| !self.is_solid(a) && !self.under_a_bridge(a)) {
            for d in [EAST, SOUTH] {
                let Some(b) = self.step(a, d) else { continue };
                if !self.is_open(a, d) || self.under_a_bridge(b) || self.zone[a] != self.zone[b] {
                    continue;
                }
                for side in across(d) {
                    for depth in 1..=goals::DEEPEST_DETOUR {
                        let (Some(leg_a), Some(leg_b)) = (self.leg(a, side, depth), self.leg(b, side, depth)) else { continue };
                        out.push(Detour { a, b, out: side, legs: [leg_a, leg_b] });
                    }
                }
            }
        }
        out
    }

    fn value_of_detour(&self, t: &Detour, counts: &(Vec<usize>, Vec<usize>), weakest: f64) -> f64 {
        let z = self.zone[t.a];
        let cells: Vec<(usize, Ground)> = t.legs.iter().flat_map(|leg| leg.cells.iter().copied()).collect();
        let claims: Vec<(usize, u8)> = cells.iter().filter(|(_, g)| *g == Ground::Free).map(|&(c, _)| (c, z)).collect();
        let crossed: Vec<usize> = cells.iter().filter(|(_, g)| *g == Ground::Crossable).map(|&(c, _)| c).collect();
        let crossings: Vec<(u8, u8)> = crossed.iter().filter(|&&c| self.zone[c] != z).map(|&c| (z, self.zone[c])).collect();
        let extra_cells: usize = t.legs.iter().map(|leg| self.extra_bridge_cells(&leg.cells)).sum();
        let n = self.zone.len() as f64;
        goals::LEVELS_PER_CELL * claims.len() as f64 / 2.0
            + bridge_value(crossed.len(), crossings.len(), extra_cells)
            + goals::ZONE_CONTACT * n * (self.contact_after(counts, &claims, &crossings) - weakest)
    }

    fn extra_bridge_cells(&self, cells: &[(usize, Ground)]) -> usize {
        let mut extra = 0;
        let mut run = 0usize;
        for &(_, g) in cells {
            if g == Ground::Crossable {
                run += 1;
            } else {
                extra += run.saturating_sub(1);
                run = 0;
            }
        }
        extra
    }

    fn take_detour(&mut self, t: &Detour) {
        let z = self.zone[t.a];
        self.close_wall(t.a, self.direction_toward(t.a, t.b));
        let mut ends = [0; 2];
        for (i, (from, leg)) in [t.a, t.b].into_iter().zip(&t.legs).enumerate() {
            let mut previous = from;
            for &(c, ground) in &leg.cells {
                if ground == Ground::Crossable {
                    continue;
                }
                self.zone[c] = z;
                if self.cells_walked_between(previous, c) == 1 {
                    self.open_wall(previous, t.out);
                } else {
                    self.lay_bridge(previous, c);
                }
                previous = c;
            }
            ends[i] = previous;
        }
        self.open_wall(ends[0], self.direction_toward(ends[0], ends[1]));
    }

    fn grow_by_detours(&mut self, rng: &mut Rng) {
        loop {
            let detours = self.detours();
            if detours.is_empty() {
                return;
            }
            let counts = self.zone_contact_counts();
            let weakest = contact_per_cell(&counts.0, &counts.1);
            let values: Vec<f64> = detours.iter().map(|t| self.value_of_detour(t, &counts, weakest)).collect();
            let t = &detours[rng.index_by_weight(&step_leaning(&values))];
            self.take_detour(t);
        }
    }

    fn shortest_path(&self, from: usize, to: usize) -> Option<Vec<usize>> {
        let walking = self.walking_distances_from(to);
        if walking[from] == usize::MAX {
            return None;
        }
        let mut path = vec![from];
        while *path.last().unwrap() != to {
            let here = *path.last().unwrap();
            let next = self.links(here).find(|&x| walking[x] != usize::MAX && walking[x] + self.cells_walked_between(x, here) == walking[here])?;
            path.push(next);
        }
        Some(path)
    }

    fn value_of_a_loop(&self, a: usize, b: usize, round: &[usize], cells_round: usize, shortest_loop: &HashMap<(usize, usize), usize>) -> f64 {
        let fork = |c: usize| if self.degree(c) == 2 { 1.0 } else { 0.0 };
        let new_score = loop_score(self, cells_round);
        let mean = if shortest_loop.is_empty() {
            0.5
        } else {
            shortest_loop.values().map(|&l| loop_score(self, l)).sum::<f64>() / shortest_loop.len() as f64
        };
        let passages = round.windows(2).map(|w| (w[0].min(w[1]), w[0].max(w[1]))).chain([(a.min(b), a.max(b))]);
        let loop_change: f64 = passages
            .map(|p| match shortest_loop.get(&p) {
                Some(&old) if old <= cells_round => 0.0,
                Some(&old) => new_score - loop_score(self, old),
                None => new_score - mean,
            })
            .sum();
        goals::MANY_FORKS * (fork(a) + fork(b)) + goals::LONG_SHORTEST_LOOPS * loop_change
    }

    fn add_loops(&mut self, rng: &mut Rng) {
        let mut candidates: Vec<(usize, usize)> = (0..self.zone.len()).flat_map(|c| (0..4).map(move |d| (c, d))).collect();
        rng.shuffle(&mut candidates);
        let mut shortest_loop: HashMap<(usize, usize), usize> = HashMap::new();
        for (c, d) in candidates {
            let Some(next) = self.step(c, d) else { continue };
            let (a, b, extra_value, cells_between, link): (usize, usize, f64, usize, Box<dyn Fn(&mut Maze)>) = if self.zone[c] == NO_ZONE {
                let partners: Vec<usize> = (0..4)
                    .filter_map(|e| self.step(c, e))
                    .filter(|&x| self.may_gain_a_way_out(x) && self.zone[x] == self.zone[next] && x != next)
                    .collect();
                if !self.may_gain_a_way_out(next) || partners.is_empty() {
                    continue;
                }
                let walking = self.walking_distances_from(next);
                let other = *partners.iter().max_by_key(|&&x| walking[x]).unwrap();
                let z = self.zone[next];
                (next, other, goals::LEVELS_PER_CELL / 2.0, 2, Box::new(move |m: &mut Maze| {
                    m.zone[c] = z;
                    m.open_wall(c, m.direction_toward(c, next));
                    m.open_wall(c, m.direction_toward(c, other));
                }))
            } else {
                if !self.may_gain_a_way_out(c) || self.is_open(c, d) {
                    continue;
                }
                let mut over = Vec::new();
                let mut t = next;
                while self.passage_a_bridge_may_cross(t, Orient::of_direction(d)) {
                    over.push(t);
                    match self.step(t, d) {
                        Some(beyond) => t = beyond,
                        None => break,
                    }
                }
                if t == c || !self.may_gain_a_way_out(t) || self.zone[t] != self.zone[c] || self.bridged_to[c].contains(&t) {
                    continue;
                }
                let over_other_zones = over.iter().filter(|&&x| self.zone[x] != self.zone[c]).count();
                let value = bridge_value(over.len(), over_other_zones, over.len().saturating_sub(1));
                let cells_between = over.len() + 1;
                (c, t, value, cells_between, Box::new(move |m: &mut Maze| {
                    if over.is_empty() {
                        m.open_wall(c, d);
                    } else {
                        m.lay_bridge(c, t);
                    }
                }))
            };
            let Some(round) = self.shortest_path(a, b) else { continue };
            let cells_round = self.walking_distances_from(b)[a] + cells_between;
            let value = extra_value + self.value_of_a_loop(a, b, &round, cells_round, &shortest_loop);
            let chance = 1.0 / (1.0 + (-goals::STEP_LEANING * value).exp());
            if rng.from_zero_to_one() < chance {
                link(self);
                for w in round.windows(2) {
                    let p = (w[0].min(w[1]), w[0].max(w[1]));
                    let old = shortest_loop.get(&p).copied().unwrap_or(usize::MAX);
                    shortest_loop.insert(p, old.min(cells_round));
                }
                shortest_loop.insert((a.min(b), a.max(b)), cells_round);
            }
        }
    }
}
