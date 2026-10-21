use std::collections::VecDeque;

use crate::maze::{open_walls_of_passage_under, Maze, Orient};
use crate::seeded_rng::Rng;

const CHANCE_TO_GROW_FROM_NEWEST: f64 = 0.5;

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

impl Maze {
    pub fn generate(rows: usize, cols: usize, zone_count: usize, rng: &mut Rng) -> Maze {
        let route = Maze::empty(rows, cols, 1).longest_path_of_a_tree(rng);
        let mut m = Maze::empty(rows, cols, zone_count);
        m.lay_route_in_zones(&route);
        m.grow_zones_out_from_the_route(&route, rng);
        m.join_every_dead_end(rng);
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

    fn take(&mut self, from: usize, w: WayOut) {
        match w.way {
            Way::Wall(d) => self.open_wall(from, d),
            Way::Bridge => self.lay_bridge(from, w.to),
        }
    }

    fn grow_tree(&mut self, in_tree: &mut [bool], mut growing: Vec<usize>, may_bridge: bool, rng: &mut Rng) {
        while !growing.is_empty() {
            let i = if rng.chance(CHANCE_TO_GROW_FROM_NEWEST) { growing.len() - 1 } else { rng.below(growing.len()) };
            let c = growing[i];
            let mut ways = self.ways_out(c, |x| !in_tree[x] && self.zone[x] == self.zone[c]);
            ways.retain(|w| may_bridge || matches!(w.way, Way::Wall(_)));
            if ways.is_empty() {
                growing.remove(i);
                continue;
            }
            let w = rng.pick(&ways);
            self.take(c, w);
            in_tree[w.to] = true;
            growing.push(w.to);
        }
    }

    fn farthest_along_links(&self, source: usize) -> (usize, Vec<usize>) {
        let mut came_from = vec![usize::MAX; self.zone.len()];
        came_from[source] = source;
        let mut queue = VecDeque::from([source]);
        let mut last = source;
        while let Some(x) = queue.pop_front() {
            last = x;
            for y in self.links(x).collect::<Vec<_>>() {
                if came_from[y] == usize::MAX {
                    came_from[y] = x;
                    queue.push_back(y);
                }
            }
        }
        (last, came_from)
    }

    fn longest_path_of_a_tree(mut self, rng: &mut Rng) -> Vec<usize> {
        let mut in_tree = vec![false; self.zone.len()];
        let first = rng.below(self.zone.len());
        in_tree[first] = true;
        self.grow_tree(&mut in_tree, vec![first], false, rng);
        let (start, _) = self.farthest_along_links(first);
        let (goal, came_from) = self.farthest_along_links(start);
        let mut route = vec![goal];
        while *route.last().unwrap() != start {
            route.push(came_from[*route.last().unwrap()]);
        }
        route
    }

    fn lay_route_in_zones(&mut self, route: &[usize]) {
        let mut queue = VecDeque::new();
        for (i, &c) in route.iter().enumerate() {
            self.zone[c] = (i * self.zone_count / route.len()) as u8;
            queue.push_back(c);
        }
        while let Some(x) = queue.pop_front() {
            for d in 0..4 {
                if let Some(y) = self.step(x, d).filter(|&y| self.zone[y] == u8::MAX) {
                    self.zone[y] = self.zone[x];
                    queue.push_back(y);
                }
            }
        }
        for w in route.windows(2) {
            self.open_wall(w[0], self.direction_toward(w[0], w[1]));
            if self.zone[w[0]] != self.zone[w[1]] {
                self.zone_crossings.push((w[0].min(w[1]), w[0].max(w[1])));
            }
        }
        self.start = route[0];
        self.goal = *route.last().unwrap();
    }

    fn grow_zones_out_from_the_route(&mut self, route: &[usize], rng: &mut Rng) {
        let mut in_tree = vec![false; self.zone.len()];
        for &c in route {
            in_tree[c] = true;
        }
        let growing = route[1..route.len() - 1].to_vec();
        self.grow_tree(&mut in_tree, growing, true, rng);
    }

    fn join_every_dead_end(&mut self, rng: &mut Rng) {
        let mut dead_ends: Vec<usize> =
            (0..self.zone.len()).filter(|&c| self.degree(c) == 1 && c != self.start && c != self.goal).collect();
        rng.shuffle(&mut dead_ends);
        for c in dead_ends {
            if self.degree(c) > 1 {
                continue;
            }
            let same_zone = |x: usize| self.zone[x] == self.zone[c] && x != self.start && x != self.goal;
            let ways = self.ways_out(c, same_zone);
            if !ways.is_empty() {
                let w = rng.pick(&ways);
                self.take(c, w);
            }
        }
    }
}
