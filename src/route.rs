use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use crate::maze::Maze;
use crate::seeded_rng::Rng;

const FIRST_CROSSINGS_TRIED_FOR_ROUTE: usize = 12;

#[derive(Clone, Copy)]
struct Crossing {
    from: usize,
    into_next_zone: usize,
    direction: usize,
}

#[derive(Clone, Copy)]
struct End {
    cell: usize,
    wall_put_back: usize,
}

struct RoutePlan {
    length: usize,
    crossings: Vec<Crossing>,
    start: End,
    goal_cell: usize,
}

impl Maze {
    fn path_distances_from(&self, source: usize) -> Vec<usize> {
        let mut dist = vec![usize::MAX; self.zone.len()];
        dist[source] = 0;
        let mut heap = BinaryHeap::from([Reverse((0, source))]);
        while let Some(Reverse((d, c))) = heap.pop() {
            if d > dist[c] {
                continue;
            }
            for m in self.links(c) {
                let (r0, c0) = (c / self.cols, c % self.cols);
                let (r1, c1) = (m / self.cols, m % self.cols);
                let bridge_counts_the_cells_it_spans = d + r0.abs_diff(r1) + c0.abs_diff(c1);
                if bridge_counts_the_cells_it_spans < dist[m] {
                    dist[m] = bridge_counts_the_cells_it_spans;
                    heap.push(Reverse((bridge_counts_the_cells_it_spans, m)));
                }
            }
        }
        dist
    }

    fn wall_to_put_back_making_an_end(&self, c: usize, z: u8, dist: &[usize], taken: &[usize]) -> Option<usize> {
        if self.zone[c] != z || !self.untouched_by_bridges(c) || self.degree(c) != 2 || taken.contains(&c) {
            return None;
        }
        let ways: Vec<usize> = (0..4).filter(|&d| self.is_open(c, d)).collect();
        if ways.len() != 2 {
            return None;
        }
        let mut best = None;
        for (i, &d) in ways.iter().enumerate() {
            let x = self.step(c, d).unwrap();
            let kept = self.step(c, ways[1 - i]).unwrap();
            if self.degree(x) < 3 || taken.contains(&x) || dist[kept] < dist[x] {
                continue;
            }
            if self.still_reachable_the_other_way(c, kept, x, z) {
                best = Some(d);
            }
        }
        best
    }

    fn still_reachable_the_other_way(&self, c: usize, kept: usize, x: usize, z: u8) -> bool {
        let mut seen = vec![c, kept];
        let mut queue = VecDeque::from([kept]);
        while let Some(y) = queue.pop_front() {
            if y == x {
                return true;
            }
            for m in self.links(y) {
                if self.zone[m] == z && !seen.contains(&m) {
                    seen.push(m);
                    queue.push_back(m);
                }
            }
        }
        false
    }

    fn farthest_possible_end(&self, source: usize, z: usize, taken: &[usize]) -> Option<(usize, End)> {
        let dist = self.path_distances_from(source);
        let n = self.zone.len();
        let mut by_distance: Vec<(usize, usize)> =
            (0..n).filter(|&c| dist[c] != usize::MAX && self.zone[c] == z as u8).map(|c| (dist[c], c)).collect();
        by_distance.sort_unstable_by(|a, b| b.cmp(a));
        by_distance.into_iter().find_map(|(dc, c)| {
            self.wall_to_put_back_making_an_end(c, z as u8, &dist, taken).map(|wall| (dc, End { cell: c, wall_put_back: wall }))
        })
    }

    pub(crate) fn place_crossings_start_and_finish(&mut self, rng: &mut Rng) -> bool {
        let last = self.zone_count - 1;
        let n = self.zone.len();
        let mut options: Vec<Vec<Crossing>> = vec![Vec::new(); self.zone_count];
        for c in 0..n {
            for d in 0..4 {
                if let Some(m) = self.can_open(c, d, false) {
                    if self.zone[m] == self.zone[c] + 1 && self.untouched_by_bridges(c) && self.untouched_by_bridges(m) {
                        options[self.zone[c] as usize].push(Crossing { from: c, into_next_zone: m, direction: d });
                    }
                }
            }
        }
        if options[..last].iter().any(|o| o.is_empty()) {
            return false;
        }
        let mut best: Option<RoutePlan> = None;
        for _ in 0..FIRST_CROSSINGS_TRIED_FOR_ROUTE {
            let (start_source, mut crossings) = if self.zone_count == 1 {
                (rng.below(n), Vec::new())
            } else {
                let first = rng.pick(&options[0]);
                (first.from, vec![first])
            };
            if self.floorless(start_source) {
                continue;
            }
            let Some((to_start, start)) = self.farthest_possible_end(start_source, 0, &[]) else { continue };
            let mut length = to_start;
            let mut entry = if self.zone_count == 1 { start.cell } else { crossings[0].into_next_zone };
            for z in 1..last {
                let dist = self.path_distances_from(entry);
                let farthest_way_on = options[z].iter().copied().filter(|o| o.from != entry).max_by_key(|o| (dist[o.from], o.from));
                let Some(next) = farthest_way_on else { break };
                length += dist[next.from];
                crossings.push(next);
                entry = next.into_next_zone;
            }
            if crossings.len() != last {
                continue;
            }
            let taken: Vec<usize> = crossings.iter().flat_map(|x| [x.from, x.into_next_zone]).chain([start.cell]).collect();
            let Some((to_goal, goal)) = self.farthest_possible_end(entry, last, &taken) else { continue };
            length += to_goal;
            if best.as_ref().is_none_or(|b| length > b.length) {
                best = Some(RoutePlan { length, crossings, start, goal_cell: goal.cell });
            }
        }
        let Some(plan) = best else { return false };
        for x in &plan.crossings {
            self.open_wall(x.from, x.direction);
            self.zone_crossings.push((x.from.min(x.into_next_zone), x.from.max(x.into_next_zone)));
        }
        self.close_wall(plan.start.cell, plan.start.wall_put_back);
        let entry_to_last_zone = if self.zone_count == 1 {
            plan.start.cell
        } else {
            self.zone_crossings.last().map(|&(a, b)| if self.zone[a] as usize == last { a } else { b }).unwrap()
        };
        let taken: Vec<usize> = self.zone_crossings.iter().flat_map(|&(a, b)| [a, b]).chain([plan.start.cell]).collect();
        let goal_wall_chosen_after_start_wall_is_up =
            self.wall_to_put_back_making_an_end(plan.goal_cell, last as u8, &self.path_distances_from(entry_to_last_zone), &taken);
        let Some(goal_wall) = goal_wall_chosen_after_start_wall_is_up else { return false };
        self.close_wall(plan.goal_cell, goal_wall);
        self.start = plan.start.cell;
        self.goal = plan.goal_cell;
        true
    }
}
