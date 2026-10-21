use std::collections::VecDeque;

use crate::goals::{self, bridge_score, crow_flies_score, loop_score, walking_score, STEP_BASELINE};
use crate::maze::{open_walls_of_passage_under, Maze, Orient, NO_ZONE};
use crate::seeded_rng::Rng;

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

fn leaning(pulls: &[f64]) -> Vec<f64> {
    pulls.iter().map(|p| p.powf(goals::LEANING_STRENGTH)).collect()
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
        m.grow_zones_toward_each_other(rng);
        m.move_cells_with_one_neighbour_in_their_zone(&route);
        let mut in_tree = vec![false; m.zone.len()];
        for &c in &route {
            in_tree[c] = true;
        }
        let mut zone_order: Vec<u8> = (0..m.zone_count as u8).collect();
        rng.shuffle(&mut zone_order);
        for z in zone_order {
            let growing = route.iter().copied().filter(|&c| m.zone[c] == z).collect();
            m.grow_tree(&mut in_tree, growing, true, rng);
        }
        m.choose_start_and_finish_among_dead_ends();
        m.join_or_fill_in_every_dead_end(rng);
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

    fn cells_passed_over(&self, from: usize, w: WayOut) -> usize {
        match w.way {
            Way::Wall(_) => 0,
            Way::Bridge => self.cells_walked_between(from, w.to) - 1,
        }
    }

    fn take(&mut self, from: usize, w: WayOut) {
        match w.way {
            Way::Wall(d) => self.open_wall(from, d),
            Way::Bridge => self.lay_bridge(from, w.to),
        }
    }

    fn makes_a_fork(&self, c: usize) -> f64 {
        if self.degree(c) == 2 { 1.0 } else { 0.0 }
    }

    fn pull_to_grow(&self, from: usize, w: WayOut) -> f64 {
        let over = self.cells_passed_over(from, w);
        STEP_BASELINE
            + goals::LEVELS_PER_CELL * (1 + over) as f64
            + goals::MANY_FORKS * self.makes_a_fork(from)
            + goals::LONGER_BRIDGES * bridge_score(over)
    }

    fn pull_to_join(&self, from: usize, w: WayOut, walking_from_here: &[usize]) -> f64 {
        let over = self.cells_passed_over(from, w);
        let round = walking_from_here[w.to] + self.cells_walked_between(from, w.to);
        STEP_BASELINE
            + goals::LONG_SHORTEST_LOOPS * loop_score(self, round)
            + goals::MANY_FORKS * self.makes_a_fork(w.to)
            + goals::LEVELS_PER_CELL * over as f64
            + goals::LONGER_BRIDGES * bridge_score(over)
    }

    fn grow_tree(&mut self, in_tree: &mut [bool], mut growing: Vec<usize>, may_bridge: bool, rng: &mut Rng) {
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
            if options.iter().any(|&(c, _)| c == newest) {
                options.retain(|&(c, _)| c == newest);
            }
            let pulls: Vec<f64> = options.iter().map(|&(c, w)| self.pull_to_grow(c, w)).collect();
            let (c, w) = options[rng.index_by_weight(&leaning(&pulls))];
            self.take(c, w);
            in_tree[w.to] = true;
            growing.push(w.to);
        }
    }

    fn route_through_a_plain_tree(mut self, rng: &mut Rng) -> Vec<usize> {
        let mut in_tree = vec![false; self.zone.len()];
        let first = rng.below(self.zone.len());
        in_tree[first] = true;
        self.grow_tree(&mut in_tree, vec![first], false, rng);
        let ends: Vec<usize> = (0..self.zone.len()).filter(|&c| self.degree(c) <= 1).collect();
        let mut best = (f64::MIN, first, first);
        for &a in &ends {
            let walking = self.walking_distances_from(a);
            for &b in ends.iter().filter(|&&b| b != a) {
                let pull = goals::WALKING_DISTANCE * walking_score(&self, walking[b])
                    + goals::CROW_FLIES_CLOSENESS * crow_flies_score(&self, a, b);
                if pull > best.0 {
                    best = (pull, a, b);
                }
            }
        }
        let (_, start, goal) = best;
        let walking = self.walking_distances_from(goal);
        let mut route = vec![start];
        while *route.last().unwrap() != goal {
            let here = *route.last().unwrap();
            route.push(self.links(here).find(|&x| walking[x] < walking[here]).unwrap());
        }
        route
    }

    fn lay_route_in_uneven_stretches(&mut self, route: &[usize], rng: &mut Rng) {
        let mut cuts: Vec<usize> = (1..route.len()).collect();
        rng.shuffle(&mut cuts);
        cuts.truncate(self.zone_count - 1);
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

    fn choose_start_and_finish_among_dead_ends(&mut self) {
        let last_zone = self.zone_count as u8 - 1;
        let dead_ends_in = |z: u8| (0..self.zone.len()).filter(|&c| self.zone[c] == z && self.degree(c) == 1).collect::<Vec<_>>();
        let (starts, finishes) = (dead_ends_in(0), dead_ends_in(last_zone));
        let mut best = (f64::MIN, starts[0], finishes[0]);
        for &a in &starts {
            let walking = self.walking_distances_from(a);
            for &b in finishes.iter().filter(|&&b| b != a) {
                let pull = goals::WALKING_DISTANCE * walking_score(self, walking[b])
                    + goals::CROW_FLIES_CLOSENESS * crow_flies_score(self, a, b);
                if pull > best.0 {
                    best = (pull, a, b);
                }
            }
        }
        (self.start, self.goal) = (best.1, best.2);
    }

    fn zone_contact_per_cell(&self) -> Vec<f64> {
        let mut contact = vec![0usize; self.zone_count];
        let mut cells = vec![0usize; self.zone_count];
        for c in (0..self.zone.len()).filter(|&c| self.zone[c] != NO_ZONE) {
            let z = self.zone[c] as usize;
            cells[z] += 1;
            contact[z] += (0..4).filter_map(|d| self.step(c, d)).filter(|&x| self.zone[x] != NO_ZONE && self.zone[x] as usize != z).count();
        }
        (0..self.zone_count).map(|z| contact[z] as f64 / cells[z].max(1) as f64).collect()
    }

    fn grow_zones_toward_each_other(&mut self, rng: &mut Rng) {
        let mut newest: Vec<Option<usize>> = vec![None; self.zone_count];
        let mut size = vec![0usize; self.zone_count];
        for c in (0..self.zone.len()).filter(|&c| self.zone[c] != NO_ZONE) {
            size[self.zone[c] as usize] += 1;
        }
        loop {
            let mut by_zone: Vec<Vec<(usize, usize)>> = vec![Vec::new(); self.zone_count];
            for y in (0..self.zone.len()).filter(|&y| self.zone[y] == NO_ZONE) {
                let beside: Vec<u8> = (0..4).filter_map(|d| self.step(y, d)).map(|x| self.zone[x]).filter(|&z| z != NO_ZONE).collect();
                for &z in &beside {
                    let touches_other_zones = beside.iter().filter(|&&other| other != z).count();
                    if !by_zone[z as usize].iter().any(|&(x, _)| x == y) {
                        by_zone[z as usize].push((y, touches_other_zones));
                    }
                }
            }
            let Some(smallest) = (0..self.zone_count).filter(|&z| !by_zone[z].is_empty()).min_by_key(|&z| size[z]) else {
                return;
            };
            let mut options = by_zone[smallest].clone();
            if let Some(tip) = newest[smallest] {
                let beside_tip: Vec<usize> = (0..4).filter_map(|d| self.step(tip, d)).collect();
                if options.iter().any(|(y, _)| beside_tip.contains(y)) {
                    options.retain(|(y, _)| beside_tip.contains(y));
                }
            }
            let pulls: Vec<f64> = options.iter().map(|&(_, touches)| STEP_BASELINE + goals::ZONE_CONTACT * touches as f64).collect();
            let (y, _) = options[rng.index_by_weight(&leaning(&pulls))];
            self.zone[y] = smallest as u8;
            size[smallest] += 1;
            newest[smallest] = Some(y);
        }
    }

    fn move_cells_with_one_neighbour_in_their_zone(&mut self, route: &[usize]) {
        let mut moved = true;
        while moved {
            moved = false;
            for c in (0..self.zone.len()).filter(|c| !route.contains(c)) {
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

    fn join_or_fill_in_every_dead_end(&mut self, rng: &mut Rng) {
        let is_dead_end = |m: &Maze, c: usize| m.degree(c) == 1 && c != m.start && c != m.goal;
        let mut dead_ends: Vec<usize> = (0..self.zone.len()).filter(|&c| is_dead_end(self, c)).collect();
        rng.shuffle(&mut dead_ends);
        let mut queue = VecDeque::from(dead_ends);
        while let Some(c) = queue.pop_front() {
            if !is_dead_end(self, c) {
                continue;
            }
            let joinable = |x: usize| self.zone[x] == self.zone[c] && !self.is_solid(x) && x != self.start && x != self.goal;
            let ways = self.ways_out(c, joinable);
            if ways.is_empty() {
                let only_link = self.links(c).next().unwrap();
                self.remove_link(c, only_link);
                if is_dead_end(self, only_link) {
                    queue.push_back(only_link);
                }
                continue;
            }
            let walking_from_here = self.walking_distances_from(c);
            let pulls: Vec<f64> = ways.iter().map(|&w| self.pull_to_join(c, w, &walking_from_here)).collect();
            let w = ways[rng.index_by_weight(&leaning(&pulls))];
            self.take(c, w);
        }
    }
}
