use std::collections::VecDeque;

use crate::maze::{open_walls_of_passage_under, Maze, Orient};
use crate::seeded_rng::Rng;

const PULL_TO_TURN: f64 = 1.0;
const PULL_TO_CARRY_STRAIGHT_ON: f64 = 0.3;
const PULL_TOWARD_CELLS_WITH_FEWER_WAYS_ON: f64 = 1.0;
const MOST_CELLS_A_HOP_PASSES_OVER: usize = 3;
const WALKS_AT_AN_EAR_BEFORE_REDRAWING_LAYOUT: usize = 30;
const STEPS_AN_EAR_MAY_BACK_UP: usize = 500;
const SMALLEST_UNCARVED_PATCH_LEFT_BEHIND: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Carving {
    Uncarved,
    InEar,
    Carved,
}

#[derive(Clone, Copy)]
enum Join {
    Wall(usize),
    Hop(usize),
    ExistingBridge,
}

#[derive(Clone, Copy)]
struct WayOn {
    direction: usize,
    cell: usize,
    hop: bool,
}

struct StepTaken {
    from: usize,
    direction: usize,
    hop: bool,
    cells_added: usize,
}

impl Maze {
    pub(crate) fn carve_in_ears(&mut self, rng: &mut Rng) -> bool {
        let n = self.zone.len();
        let mut state: Vec<Carving> =
            (0..n).map(|c| if self.floorless(c) { Carving::Carved } else { Carving::Uncarved }).collect();
        let mut entered_by = vec![usize::MAX; n];
        let mut may_set_out_from: Vec<Vec<usize>> = vec![Vec::new(); self.zone_count];
        for (z, starts) in may_set_out_from.iter_mut().enumerate() {
            let cells: Vec<usize> = (0..n)
                .filter(|&c| self.zone[c] == z as u8 && state[c] == Carving::Uncarved && self.untouched_by_bridges(c))
                .collect();
            if cells.is_empty() {
                return false;
            }
            let c = rng.pick(&cells);
            state[c] = Carving::Carved;
            starts.push(c);
        }
        loop {
            for starts in may_set_out_from.iter_mut() {
                starts.retain(|&c| !self.ways_into_uncarved(c, &state, true).is_empty());
            }
            let live: Vec<usize> = (0..self.zone_count).filter(|&z| !may_set_out_from[z].is_empty()).collect();
            if live.is_empty() {
                break;
            }
            let z = rng.pick(&live);
            let mut walks = 0;
            let new_cells = loop {
                let saved = (self.clone(), state.clone(), entered_by.clone());
                let may_settle_for_longest_loop = walks >= WALKS_AT_AN_EAR_BEFORE_REDRAWING_LAYOUT / 2;
                let ear = self.walk_ear(z as u8, &may_set_out_from[z], may_settle_for_longest_loop, &mut state, &mut entered_by, rng);
                if let Some(cells) = ear {
                    if !self.walls_in_a_lone_uncarved_cell(z as u8, &state) {
                        break cells;
                    }
                }
                (*self, state, entered_by) = saved;
                walks += 1;
                if walks == WALKS_AT_AN_EAR_BEFORE_REDRAWING_LAYOUT {
                    return false;
                }
            };
            may_set_out_from[z].retain(|&c| state[c] == Carving::Carved);
            may_set_out_from[z].extend(new_cells);
        }
        state.iter().all(|&s| s == Carving::Carved)
    }

    fn walls_in_a_lone_uncarved_cell(&self, z: u8, state: &[Carving]) -> bool {
        (0..self.zone.len()).any(|c| {
            self.zone[c] == z
                && state[c] == Carving::Uncarved
                && !(0..4).filter_map(|d| self.can_open(c, d, true)).any(|x| state[x] == Carving::Uncarved)
                && !self.bridged_to[c].iter().any(|&x| state[x] == Carving::Uncarved)
        })
    }

    fn ways_into_uncarved(&self, c: usize, state: &[Carving], hops_allowed: bool) -> Vec<WayOn> {
        let walls: Vec<WayOn> = (0..4)
            .filter_map(|d| self.can_open(c, d, true).map(|m| WayOn { direction: d, cell: m, hop: false }))
            .filter(|w| state[w.cell] == Carving::Uncarved)
            .collect();
        if !walls.is_empty() || !hops_allowed {
            return walls;
        }
        self.hops_from(c, |y| state[y] == Carving::Uncarved && self.bridged_to[y].is_empty())
            .into_iter()
            .map(|(d, y)| WayOn { direction: d, cell: y, hop: true })
            .collect()
    }

    fn walk_ear(
        &mut self,
        z: u8,
        starts: &[usize],
        may_settle_for_longest_loop: bool,
        state: &mut [Carving],
        entered_by: &mut [usize],
        rng: &mut Rng,
    ) -> Option<Vec<usize>> {
        let n = self.zone.len();
        let u = rng.pick(starts);
        let zones_first_ear = self.degree(u) == 0;
        let mut back_to_u = vec![usize::MAX; n];
        back_to_u[u] = u;
        let mut queue = VecDeque::from([u]);
        while let Some(x) = queue.pop_front() {
            for y in self.links(x).collect::<Vec<_>>() {
                if back_to_u[y] == usize::MAX && state[y] == Carving::Carved {
                    back_to_u[y] = x;
                    queue.push_back(y);
                }
            }
        }
        let mut ear = vec![u];
        let mut steps: Vec<StepTaken> = Vec::new();
        let mut tried_from_each_place: Vec<Vec<usize>> = vec![Vec::new()];
        let mut backed_up = 0;
        loop {
            let cur = *ear.last().unwrap();
            let prev = if ear.len() > 1 { ear[ear.len() - 2] } else { usize::MAX };
            let mut joins: Vec<(Vec<usize>, Join)> = Vec::new();
            let may_stop_here = ear.len() > 1 && !self.leaves_too_small_a_patch_behind(cur, usize::MAX, state);
            if may_stop_here {
                for d in 0..4 {
                    if self.is_open(cur, d) {
                        continue;
                    }
                    let Some(v) = self.can_open(cur, d, true) else { continue };
                    if v != prev && state[v] != Carving::Uncarved {
                        if let Some(round) = self.loop_closed_by_joining(&ear, v, &back_to_u, state) {
                            joins.push((round, Join::Wall(d)));
                        }
                    }
                }
                let hops =
                    self.hops_from(cur, |y| y != prev && state[y] != Carving::Uncarved && self.bridged_to[y].is_empty());
                for (_, y) in hops {
                    if let Some(round) = self.loop_closed_by_joining(&ear, y, &back_to_u, state) {
                        joins.push((round, Join::Hop(y)));
                    }
                }
            }
            let here = steps.len();
            let lead_in_may_be_taken_back_so_no_hops = zones_first_ear;
            let mut ways: Vec<WayOn> = self
                .ways_into_uncarved(cur, state, !lead_in_may_be_taken_back_so_no_hops)
                .into_iter()
                .filter(|w| !tried_from_each_place[here].contains(&w.cell))
                .collect();
            ways.retain(|w| {
                if self.leaves_too_small_a_patch_behind(cur, w.cell, state) {
                    return false;
                }
                match self.bridged_to[w.cell].as_slice() {
                    [far_end] if !(w.hop && *far_end == cur) => {
                        let carried_on_to = if state[*far_end] == Carving::Uncarved { *far_end } else { usize::MAX };
                        !self.leaves_too_small_a_patch_behind(w.cell, carried_on_to, state)
                    }
                    _ => true,
                }
            });
            let winds_round_another_zone =
                joins.iter().filter(|j| self.loop_winds_round_or_across_another_zone(z, &j.0)).max_by_key(|j| j.0.len());
            if let Some((round, join)) = winds_round_another_zone {
                let (round, join) = (round.clone(), *join);
                return Some(self.close_ear(&ear, &round, join, zones_first_ear, state));
            }
            if ways.is_empty() {
                let detour_on_the_corridor_it_left = joins.iter().find(|j| {
                    let v = *j.0.last().unwrap();
                    j.0.len() == ear.len() + 1 && self.adjacent(u, v) && self.is_open(u, self.direction_toward(u, v))
                });
                if let Some((round, join)) = detour_on_the_corridor_it_left {
                    let (v, join) = (*round.last().unwrap(), *join);
                    let round = round.clone();
                    self.close_wall(u, self.direction_toward(u, v));
                    return Some(self.close_ear(&ear, &round, join, zones_first_ear, state));
                }
                let longest = joins
                    .iter()
                    .filter(|j| may_settle_for_longest_loop && !self.is_round_a_lone_pillar(&j.0))
                    .max_by_key(|j| (j.0.len(), rng.below(1000)));
                if let Some((round, join)) = longest {
                    let (round, join) = (round.clone(), *join);
                    return Some(self.close_ear(&ear, &round, join, zones_first_ear, state));
                }
            }
            if ways.is_empty() {
                let step = steps.pop()?;
                backed_up += 1;
                if backed_up > STEPS_AN_EAR_MAY_BACK_UP {
                    return None;
                }
                if step.hop {
                    self.take_back_last_hop();
                } else {
                    self.close_wall(step.from, step.direction);
                }
                for c in ear.drain(ear.len() - step.cells_added..) {
                    state[c] = Carving::Uncarved;
                }
                tried_from_each_place.pop();
                continue;
            }
            let pulls: Vec<f64> = ways
                .iter()
                .map(|w| {
                    let ways_on_from_there = self.ways_into_uncarved(w.cell, state, true).len();
                    let turn = if entered_by[cur] == w.direction { PULL_TO_CARRY_STRAIGHT_ON } else { PULL_TO_TURN };
                    turn / (1.0 + ways_on_from_there as f64).powf(PULL_TOWARD_CELLS_WITH_FEWER_WAYS_ON)
                })
                .collect();
            let w = ways[rng.index_by_weight(&pulls)];
            tried_from_each_place[here].push(w.cell);
            steps.push(StepTaken { from: cur, direction: w.direction, hop: w.hop, cells_added: 1 });
            tried_from_each_place.push(Vec::new());
            if w.hop {
                self.lay_hop(cur, w.cell);
            } else {
                self.open_wall(cur, w.direction);
            }
            entered_by[w.cell] = w.direction;
            state[w.cell] = Carving::InEar;
            ear.push(w.cell);
            let Some(&far_end) = self.bridged_to[w.cell].first() else { continue };
            let landed_on_an_existing_bridge = self.bridged_to[w.cell].len() == 1 && !(w.hop && far_end == cur);
            if !landed_on_an_existing_bridge {
                continue;
            }
            if state[far_end] == Carving::Uncarved {
                entered_by[far_end] = self.direction_toward(w.cell, far_end);
                state[far_end] = Carving::InEar;
                ear.push(far_end);
                steps.last_mut().unwrap().cells_added = 2;
            } else if let Some(round) = self.loop_closed_by_joining(&ear, far_end, &back_to_u, state) {
                return Some(self.close_ear(&ear, &round, Join::ExistingBridge, zones_first_ear, state));
            }
        }
    }

    fn leaves_too_small_a_patch_behind(&self, cur: usize, going_to: usize, state: &[Carving]) -> bool {
        let uncarved_neighbours = |c: usize| -> Vec<usize> {
            let mut out: Vec<usize> = (0..4).filter_map(|d| self.can_open(c, d, true)).collect();
            out.extend(&self.bridged_to[c]);
            out.retain(|&x| x != going_to && x != cur && state[x] == Carving::Uncarved);
            out
        };
        uncarved_neighbours(cur).into_iter().any(|q| {
            let mut patch = vec![q];
            let mut i = 0;
            while i < patch.len() && patch.len() < SMALLEST_UNCARVED_PATCH_LEFT_BEHIND {
                for x in uncarved_neighbours(patch[i]) {
                    if !patch.contains(&x) {
                        patch.push(x);
                    }
                }
                i += 1;
            }
            patch.len() < SMALLEST_UNCARVED_PATCH_LEFT_BEHIND
        })
    }

    fn close_ear(&mut self, ear: &[usize], round: &[usize], join: Join, zones_first_ear: bool, state: &mut [Carving]) -> Vec<usize> {
        let cur = *ear.last().unwrap();
        match join {
            Join::Wall(d) => self.open_wall(cur, d),
            Join::Hop(y) => self.lay_hop(cur, y),
            Join::ExistingBridge => {}
        }
        let mut ear = ear;
        let came_back_to_itself_not_its_start = zones_first_ear && round[0] != ear[0];
        if came_back_to_itself_not_its_start {
            let loop_starts = ear.iter().position(|&c| c == round[0]).unwrap();
            for i in 0..loop_starts {
                self.close_wall(ear[i], self.direction_toward(ear[i], ear[i + 1]));
                state[ear[i]] = Carving::Uncarved;
            }
            ear = &ear[loop_starts - 1..];
        }
        for &c in &ear[1..] {
            state[c] = Carving::Carved;
        }
        ear[1..].to_vec()
    }

    fn loop_closed_by_joining(&self, ear: &[usize], v: usize, back_to_start: &[usize], state: &[Carving]) -> Option<Vec<usize>> {
        if state[v] == Carving::InEar {
            let j = ear.iter().position(|&c| c == v)?;
            return Some(ear[j..].to_vec());
        }
        if back_to_start[v] == usize::MAX {
            return None;
        }
        let mut round = ear.to_vec();
        let mut x = v;
        while x != ear[0] {
            round.push(x);
            x = back_to_start[x];
        }
        Some(round)
    }

    fn is_round_a_lone_pillar(&self, round: &[usize]) -> bool {
        round.len() == 4 && (0..4).all(|i| self.adjacent(round[i], round[(i + 1) % 4]))
    }

    fn loop_winds_round_or_across_another_zone(&self, z: u8, round: &[usize]) -> bool {
        let k = round.len();
        for i in 0..k {
            let (a, b) = (round[i], round[(i + 1) % k]);
            let another_zones_passage_or_bridge =
                |m: usize| self.zone[m] != z && (!self.solid_under_bridge[m] || self.is_crossing(m));
            if !self.adjacent(a, b) && self.cells_between(a.min(b), a.max(b)).iter().any(|&m| another_zones_passage_or_bridge(m)) {
                return true;
            }
        }
        for &c in round {
            let under_another_zones_bridge = self.bridges_overhead[c] != 0
                && self.bridges.iter().any(|b| self.zone[b.top_or_left] != z && b.spans.contains(&c));
            if under_another_zones_bridge {
                return true;
            }
        }
        let cols = self.cols;
        let (mut r0, mut r1) = (self.rows, 0);
        for &c in round {
            r0 = r0.min(c / cols);
            r1 = r1.max(c / cols);
        }
        let mut upright_sides_crossing_row: Vec<Vec<usize>> = vec![Vec::new(); r1 + 1];
        for i in 0..k {
            let (a, b) = (round[i], round[(i + 1) % k]);
            if a % cols == b % cols {
                let (top, bottom) = ((a / cols).min(b / cols), (a / cols).max(b / cols));
                for row in upright_sides_crossing_row.iter_mut().take(bottom).skip(top) {
                    row.push(a % cols);
                }
            }
        }
        for (r, xs) in upright_sides_crossing_row.iter_mut().enumerate().skip(r0) {
            xs.sort_unstable();
            for inside in xs.chunks(2) {
                if let [lo, hi] = *inside {
                    if (lo..hi).any(|c| self.zone[r * cols + c] != z) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn lay_hop(&mut self, a: usize, b: usize) {
        let (lo, hi) = (a.min(b), a.max(b));
        let solid: Vec<bool> = self.cells_between(lo, hi).iter().map(|&m| self.solid_under_bridge[m]).collect();
        self.lay_bridge(lo, hi, &solid);
        self.stats.corridor_hops += 1;
    }

    fn take_back_last_hop(&mut self) {
        let b = self.bridges.pop().unwrap();
        self.bridged_to[b.top_or_left].pop();
        self.bridged_to[b.bottom_or_right].pop();
        for &m in &b.spans {
            self.bridges_overhead[m] &= !b.orientation.bit();
        }
        self.stats.corridor_hops -= 1;
    }

    fn hops_from(&self, cur: usize, lands: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if self.bridges_overhead[cur] != 0 || self.solid_under_bridge[cur] {
            return out;
        }
        for d in 0..4 {
            let o = Orient::of_direction(d);
            let mut x = cur;
            let mut just_passed_a_passage = false;
            for k in 1..=MOST_CELLS_A_HOP_PASSES_OVER + 1 {
                let Some(next) = self.step(x, d) else { break };
                x = next;
                let own_zones_landing = self.zone[x] == self.zone[cur]
                    && self.bridges_overhead[x] == 0
                    && !self.solid_under_bridge[x]
                    && lands(x);
                if k >= 2 && own_zones_landing {
                    out.push((d, x));
                    break;
                }
                if k > MOST_CELLS_A_HOP_PASSES_OVER {
                    break;
                }
                let finished_passage_across = self.untouched_by_bridges(x) && self.open_walls[x] == open_walls_of_passage_under(o);
                let solid_under_crossing_bridge = self.solid_under_bridge[x] && self.bridges_overhead[x] == o.other().bit();
                if finished_passage_across && !just_passed_a_passage {
                    just_passed_a_passage = true;
                } else if solid_under_crossing_bridge {
                    just_passed_a_passage = false;
                } else {
                    break;
                }
            }
        }
        out
    }
}
