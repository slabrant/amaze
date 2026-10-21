use std::collections::VecDeque;

use crate::maze::{open_walls_of_passage_under, Maze, Orient};
use crate::seeded_rng::Rng;

const PULL_TO_TURN: f64 = 1.0;
const PULL_TO_CARRY_STRAIGHT_ON: f64 = 0.3;
const MOST_CELLS_A_HOP_PASSES_OVER: usize = 3;
const WALKS_AT_AN_EAR_BEFORE_REDRAWING_LAYOUT: usize = 30;
const WALKS_HOLDING_OUT_FOR_A_LOOP_WORTH_HAVING: usize = WALKS_AT_AN_EAR_BEFORE_REDRAWING_LAYOUT / 2;
const STEPS_AN_EAR_MAY_BACK_UP: usize = 500;
const SMALLEST_UNCARVED_PATCH_LEFT_BEHIND: usize = 2;
const NOT_A_CELL: usize = usize::MAX;
const TIE_BREAK_RANGE: usize = 1000;

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

#[derive(Clone)]
struct LoopJoin {
    round: Vec<usize>,
    join: Join,
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

struct EarWalk {
    zone: u8,
    start: usize,
    zones_first_ear: bool,
    may_settle_for_longest_loop: bool,
    shortest_way_back_to_start: Vec<Option<usize>>,
    cells: Vec<usize>,
    steps: Vec<StepTaken>,
    tried_from_each_place: Vec<Vec<usize>>,
    backed_up: usize,
}

impl EarWalk {
    fn newest(&self) -> usize {
        *self.cells.last().unwrap()
    }

    fn previous(&self) -> usize {
        if self.cells.len() > 1 { self.cells[self.cells.len() - 2] } else { NOT_A_CELL }
    }

    fn tried_here(&mut self) -> &mut Vec<usize> {
        let here = self.steps.len();
        &mut self.tried_from_each_place[here]
    }
}

enum Next {
    Close(LoopJoin),
    Detour(LoopJoin),
    Step(WayOn),
    BackUp,
}

impl Maze {
    pub(crate) fn carve_in_ears(&mut self, rng: &mut Rng) -> bool {
        let n = self.zone.len();
        let mut state: Vec<Carving> =
            (0..n).map(|cell| if self.floorless(cell) { Carving::Carved } else { Carving::Uncarved }).collect();
        let mut entered_by: Vec<Option<usize>> = vec![None; n];
        let mut may_set_out_from: Vec<Vec<usize>> = vec![Vec::new(); self.zone_count];
        for (zone, starts) in may_set_out_from.iter_mut().enumerate() {
            let cells: Vec<usize> = (0..n)
                .filter(|&cell| {
                    self.zone[cell] == zone as u8 && state[cell] == Carving::Uncarved && self.untouched_by_bridges(cell)
                })
                .collect();
            if cells.is_empty() {
                return false;
            }
            let first = rng.pick(&cells);
            state[first] = Carving::Carved;
            starts.push(first);
        }
        loop {
            for starts in may_set_out_from.iter_mut() {
                starts.retain(|&cell| !self.ways_into_uncarved(cell, &state, true).is_empty());
            }
            let live: Vec<usize> = (0..self.zone_count).filter(|&zone| !may_set_out_from[zone].is_empty()).collect();
            if live.is_empty() {
                break;
            }
            let zone = rng.pick(&live);
            let Some(new_cells) = self.carve_one_ear(zone as u8, &may_set_out_from[zone], &mut state, &mut entered_by, rng) else {
                return false;
            };
            may_set_out_from[zone].retain(|&cell| state[cell] == Carving::Carved);
            may_set_out_from[zone].extend(new_cells);
        }
        state.iter().all(|&s| s == Carving::Carved)
    }

    fn carve_one_ear(
        &mut self,
        zone: u8,
        starts: &[usize],
        state: &mut Vec<Carving>,
        entered_by: &mut Vec<Option<usize>>,
        rng: &mut Rng,
    ) -> Option<Vec<usize>> {
        for walks in 0..WALKS_AT_AN_EAR_BEFORE_REDRAWING_LAYOUT {
            let before_walking = (self.clone(), state.clone(), entered_by.clone());
            let may_settle = walks >= WALKS_HOLDING_OUT_FOR_A_LOOP_WORTH_HAVING;
            match self.walk_ear(zone, starts, may_settle, state, entered_by, rng) {
                Some(cells) if !self.walls_in_a_lone_uncarved_cell(zone, state) => return Some(cells),
                _ => (*self, *state, *entered_by) = before_walking,
            }
        }
        None
    }

    fn walls_in_a_lone_uncarved_cell(&self, zone: u8, state: &[Carving]) -> bool {
        (0..self.zone.len()).any(|cell| {
            self.zone[cell] == zone
                && state[cell] == Carving::Uncarved
                && self.uncarved_neighbours(cell, state).is_empty()
        })
    }

    fn uncarved_neighbours(&self, cell: usize, state: &[Carving]) -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter_map(|direction| self.can_open(cell, direction, true)).collect();
        out.extend(&self.bridged_to[cell]);
        out.retain(|&x| state[x] == Carving::Uncarved);
        out
    }

    fn ways_into_uncarved(&self, cell: usize, state: &[Carving], hops_allowed: bool) -> Vec<WayOn> {
        let walls: Vec<WayOn> = (0..4)
            .filter_map(|direction| self.can_open(cell, direction, true).map(|next| WayOn { direction, cell: next, hop: false }))
            .filter(|w| state[w.cell] == Carving::Uncarved)
            .collect();
        if !walls.is_empty() || !hops_allowed {
            return walls;
        }
        self.hops_from(cell, |landing| state[landing] == Carving::Uncarved && self.bridged_to[landing].is_empty())
            .into_iter()
            .map(|(direction, landing)| WayOn { direction, cell: landing, hop: true })
            .collect()
    }

    fn shortest_ways_back_to(&self, start: usize, state: &[Carving]) -> Vec<Option<usize>> {
        let mut way_back: Vec<Option<usize>> = vec![None; self.zone.len()];
        way_back[start] = Some(start);
        let mut queue = VecDeque::from([start]);
        while let Some(x) = queue.pop_front() {
            for y in self.links(x).collect::<Vec<_>>() {
                if way_back[y].is_none() && state[y] == Carving::Carved {
                    way_back[y] = Some(x);
                    queue.push_back(y);
                }
            }
        }
        way_back
    }

    fn walk_ear(
        &mut self,
        zone: u8,
        starts: &[usize],
        may_settle_for_longest_loop: bool,
        state: &mut [Carving],
        entered_by: &mut [Option<usize>],
        rng: &mut Rng,
    ) -> Option<Vec<usize>> {
        let start = rng.pick(starts);
        let mut walk = EarWalk {
            zone,
            start,
            zones_first_ear: self.degree(start) == 0,
            may_settle_for_longest_loop,
            shortest_way_back_to_start: self.shortest_ways_back_to(start, state),
            cells: vec![start],
            steps: Vec::new(),
            tried_from_each_place: vec![Vec::new()],
            backed_up: 0,
        };
        loop {
            match self.next_move(&mut walk, state, entered_by, rng) {
                Next::Close(j) => return Some(self.close_ear(&walk, &j, state)),
                Next::Detour(j) => {
                    self.send_the_corridor_it_left_round_the_detour(&walk, &j);
                    return Some(self.close_ear(&walk, &j, state));
                }
                Next::BackUp => {
                    if !self.back_up(&mut walk, state) {
                        return None;
                    }
                }
                Next::Step(way) => {
                    if let Some(j) = self.take_step(&mut walk, way, state, entered_by) {
                        return Some(self.close_ear(&walk, &j, state));
                    }
                }
            }
        }
    }

    fn send_the_corridor_it_left_round_the_detour(&mut self, walk: &EarWalk, detour: &LoopJoin) {
        let rejoined = *detour.round.last().unwrap();
        let wall_the_corridor_ran_through = self.direction_toward(walk.start, rejoined);
        self.close_wall(walk.start, wall_the_corridor_ran_through);
    }

    fn next_move(&self, walk: &mut EarWalk, state: &[Carving], entered_by: &[Option<usize>], rng: &mut Rng) -> Next {
        let joins = self.loops_it_could_close(walk, state);
        let ways = self.ways_on_without_leaving_a_lone_cell(walk, state);
        let winds_round_another_zone = joins
            .iter()
            .filter(|j| self.loop_winds_round_or_across_another_zone(walk.zone, &j.round))
            .max_by_key(|j| j.round.len());
        if let Some(j) = winds_round_another_zone {
            return Next::Close(j.clone());
        }
        if !ways.is_empty() {
            return Next::Step(self.choose_way_on(walk, &ways, state, entered_by, rng));
        }
        let detour_on_the_corridor_it_left = joins.iter().find(|j| {
            let rejoined = *j.round.last().unwrap();
            j.round.len() == walk.cells.len() + 1
                && self.adjacent(walk.start, rejoined)
                && self.is_open(walk.start, self.direction_toward(walk.start, rejoined))
        });
        if let Some(j) = detour_on_the_corridor_it_left {
            return Next::Detour(j.clone());
        }
        let longest = joins
            .iter()
            .filter(|j| walk.may_settle_for_longest_loop && !self.is_round_a_lone_pillar(&j.round))
            .max_by_key(|j| (j.round.len(), rng.below(TIE_BREAK_RANGE)));
        match longest {
            Some(j) => Next::Close(j.clone()),
            None => Next::BackUp,
        }
    }

    fn loops_it_could_close(&self, walk: &EarWalk, state: &[Carving]) -> Vec<LoopJoin> {
        let (cur, prev) = (walk.newest(), walk.previous());
        let mut joins = Vec::new();
        let may_stop_here = walk.cells.len() > 1 && !self.leaves_too_small_a_patch_behind(cur, NOT_A_CELL, state);
        if !may_stop_here {
            return joins;
        }
        for direction in 0..4 {
            if self.is_open(cur, direction) {
                continue;
            }
            let Some(v) = self.can_open(cur, direction, true) else { continue };
            if v != prev && state[v] != Carving::Uncarved {
                if let Some(round) = self.loop_closed_by_joining(walk, v, state) {
                    joins.push(LoopJoin { round, join: Join::Wall(direction) });
                }
            }
        }
        let carved_landing = |y: usize| y != prev && state[y] != Carving::Uncarved && self.bridged_to[y].is_empty();
        for (_, landing) in self.hops_from(cur, carved_landing) {
            if let Some(round) = self.loop_closed_by_joining(walk, landing, state) {
                joins.push(LoopJoin { round, join: Join::Hop(landing) });
            }
        }
        joins
    }

    fn ways_on_without_leaving_a_lone_cell(&self, walk: &mut EarWalk, state: &[Carving]) -> Vec<WayOn> {
        let cur = walk.newest();
        let lead_in_may_be_taken_back_so_no_hops = walk.zones_first_ear;
        let tried = walk.tried_here().clone();
        let mut ways: Vec<WayOn> = self
            .ways_into_uncarved(cur, state, !lead_in_may_be_taken_back_so_no_hops)
            .into_iter()
            .filter(|w| !tried.contains(&w.cell))
            .collect();
        ways.retain(|w| {
            if self.leaves_too_small_a_patch_behind(cur, w.cell, state) {
                return false;
            }
            match self.bridged_to[w.cell].as_slice() {
                [far_end] if !(w.hop && *far_end == cur) => {
                    let carried_on_to = if state[*far_end] == Carving::Uncarved { *far_end } else { NOT_A_CELL };
                    !self.leaves_too_small_a_patch_behind(w.cell, carried_on_to, state)
                }
                _ => true,
            }
        });
        ways
    }

    fn choose_way_on(&self, walk: &EarWalk, ways: &[WayOn], state: &[Carving], entered_by: &[Option<usize>], rng: &mut Rng) -> WayOn {
        let pulls: Vec<f64> = ways.iter().map(|&way| self.pull_toward(walk.newest(), way, state, entered_by)).collect();
        ways[rng.index_by_weight(&pulls)]
    }

    fn pull_toward(&self, cur: usize, way: WayOn, state: &[Carving], entered_by: &[Option<usize>]) -> f64 {
        let carries_straight_on = entered_by[cur] == Some(way.direction);
        let pull_so_corridors_twist = if carries_straight_on { PULL_TO_CARRY_STRAIGHT_ON } else { PULL_TO_TURN };
        let ways_on_from_there = self.ways_into_uncarved(way.cell, state, true).len();
        let room_left_round_there = 1.0 + ways_on_from_there as f64;
        pull_so_corridors_twist / room_left_round_there
    }

    fn back_up(&mut self, walk: &mut EarWalk, state: &mut [Carving]) -> bool {
        let Some(step) = walk.steps.pop() else { return false };
        walk.backed_up += 1;
        if walk.backed_up > STEPS_AN_EAR_MAY_BACK_UP {
            return false;
        }
        if step.hop {
            self.take_back_last_hop();
        } else {
            self.close_wall(step.from, step.direction);
        }
        let keep = walk.cells.len() - step.cells_added;
        for cell in walk.cells.drain(keep..) {
            state[cell] = Carving::Uncarved;
        }
        walk.tried_from_each_place.pop();
        true
    }

    fn take_step(&mut self, walk: &mut EarWalk, way: WayOn, state: &mut [Carving], entered_by: &mut [Option<usize>]) -> Option<LoopJoin> {
        let cur = walk.newest();
        walk.tried_here().push(way.cell);
        walk.steps.push(StepTaken { from: cur, direction: way.direction, hop: way.hop, cells_added: 1 });
        walk.tried_from_each_place.push(Vec::new());
        if way.hop {
            self.lay_hop(cur, way.cell);
        } else {
            self.open_wall(cur, way.direction);
        }
        entered_by[way.cell] = Some(way.direction);
        state[way.cell] = Carving::InEar;
        walk.cells.push(way.cell);
        let far_end = *self.bridged_to[way.cell].first()?;
        let landed_on_an_existing_bridge = self.bridged_to[way.cell].len() == 1 && !(way.hop && far_end == cur);
        if !landed_on_an_existing_bridge {
            return None;
        }
        if state[far_end] == Carving::Uncarved {
            entered_by[far_end] = Some(self.direction_toward(way.cell, far_end));
            state[far_end] = Carving::InEar;
            walk.cells.push(far_end);
            walk.steps.last_mut().unwrap().cells_added = 2;
            return None;
        }
        let round = self.loop_closed_by_joining(walk, far_end, state)?;
        Some(LoopJoin { round, join: Join::ExistingBridge })
    }

    fn leaves_too_small_a_patch_behind(&self, cur: usize, going_to: usize, state: &[Carving]) -> bool {
        let neighbours_left_behind = |cell: usize| -> Vec<usize> {
            let mut out = self.uncarved_neighbours(cell, state);
            out.retain(|&x| x != going_to && x != cur);
            out
        };
        neighbours_left_behind(cur).into_iter().any(|q| {
            let mut patch = vec![q];
            let mut i = 0;
            while i < patch.len() && patch.len() < SMALLEST_UNCARVED_PATCH_LEFT_BEHIND {
                for x in neighbours_left_behind(patch[i]) {
                    if !patch.contains(&x) {
                        patch.push(x);
                    }
                }
                i += 1;
            }
            patch.len() < SMALLEST_UNCARVED_PATCH_LEFT_BEHIND
        })
    }

    fn close_ear(&mut self, walk: &EarWalk, j: &LoopJoin, state: &mut [Carving]) -> Vec<usize> {
        let cur = walk.newest();
        match j.join {
            Join::Wall(direction) => self.open_wall(cur, direction),
            Join::Hop(landing) => self.lay_hop(cur, landing),
            Join::ExistingBridge => {}
        }
        let mut ear: &[usize] = &walk.cells;
        let came_back_to_itself_not_its_start = walk.zones_first_ear && j.round[0] != ear[0];
        if came_back_to_itself_not_its_start {
            let loop_starts = ear.iter().position(|&cell| cell == j.round[0]).unwrap();
            for i in 0..loop_starts {
                self.close_wall(ear[i], self.direction_toward(ear[i], ear[i + 1]));
                state[ear[i]] = Carving::Uncarved;
            }
            ear = &ear[loop_starts - 1..];
        }
        for &cell in &ear[1..] {
            state[cell] = Carving::Carved;
        }
        ear[1..].to_vec()
    }

    fn loop_closed_by_joining(&self, walk: &EarWalk, v: usize, state: &[Carving]) -> Option<Vec<usize>> {
        if state[v] == Carving::InEar {
            let j = walk.cells.iter().position(|&cell| cell == v)?;
            return Some(walk.cells[j..].to_vec());
        }
        walk.shortest_way_back_to_start[v]?;
        let mut round = walk.cells.clone();
        let mut x = v;
        while x != walk.start {
            round.push(x);
            x = walk.shortest_way_back_to_start[x].unwrap();
        }
        Some(round)
    }

    fn is_round_a_lone_pillar(&self, round: &[usize]) -> bool {
        round.len() == 4 && (0..4).all(|i| self.adjacent(round[i], round[(i + 1) % 4]))
    }

    fn loop_winds_round_or_across_another_zone(&self, zone: u8, round: &[usize]) -> bool {
        let k = round.len();
        let another_zones_passage_or_bridge =
            |cell: usize| self.zone[cell] != zone && (!self.solid_under_bridge[cell] || self.is_crossing(cell));
        for i in 0..k {
            let (a, b) = (round[i], round[(i + 1) % k]);
            let bridges_over_another_zone =
                !self.adjacent(a, b) && self.cells_between(a.min(b), a.max(b)).iter().any(|&cell| another_zones_passage_or_bridge(cell));
            if bridges_over_another_zone {
                return true;
            }
        }
        for &cell in round {
            let under_another_zones_bridge = self.bridges_overhead[cell] != 0
                && self.bridges.iter().any(|b| self.zone[b.top_or_left] != zone && b.spans.contains(&cell));
            if under_another_zones_bridge {
                return true;
            }
        }
        self.another_zone_inside(zone, round)
    }

    fn another_zone_inside(&self, zone: u8, round: &[usize]) -> bool {
        let (k, cols) = (round.len(), self.cols);
        let (mut top_row, mut bottom_row) = (self.rows, 0);
        for &cell in round {
            top_row = top_row.min(cell / cols);
            bottom_row = bottom_row.max(cell / cols);
        }
        let mut upright_sides_crossing_row: Vec<Vec<usize>> = vec![Vec::new(); bottom_row + 1];
        for i in 0..k {
            let (a, b) = (round[i], round[(i + 1) % k]);
            if a % cols == b % cols {
                let (top, bottom) = ((a / cols).min(b / cols), (a / cols).max(b / cols));
                for row in upright_sides_crossing_row.iter_mut().take(bottom).skip(top) {
                    row.push(a % cols);
                }
            }
        }
        for (row, xs) in upright_sides_crossing_row.iter_mut().enumerate().skip(top_row) {
            xs.sort_unstable();
            for inside in xs.chunks(2) {
                if let [from_col, to_col] = *inside {
                    if (from_col..to_col).any(|col| self.zone[row * cols + col] != zone) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn lay_hop(&mut self, a: usize, b: usize) {
        let (lo, hi) = (a.min(b), a.max(b));
        let solid: Vec<bool> = self.cells_between(lo, hi).iter().map(|&cell| self.solid_under_bridge[cell]).collect();
        self.lay_bridge(lo, hi, &solid);
        self.stats.corridor_hops += 1;
    }

    fn take_back_last_hop(&mut self) {
        let b = self.bridges.pop().unwrap();
        self.bridged_to[b.top_or_left].pop();
        self.bridged_to[b.bottom_or_right].pop();
        for &cell in &b.spans {
            self.bridges_overhead[cell] &= !b.orientation.bit();
        }
        self.stats.corridor_hops -= 1;
    }

    fn hops_from(&self, cur: usize, lands: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if self.bridges_overhead[cur] != 0 || self.solid_under_bridge[cur] {
            return out;
        }
        for direction in 0..4 {
            let along = Orient::of_direction(direction);
            let mut x = cur;
            let mut just_passed_a_passage = false;
            for cells_out in 1..=MOST_CELLS_A_HOP_PASSES_OVER + 1 {
                let Some(next) = self.step(x, direction) else { break };
                x = next;
                let past_at_least_one_cell = cells_out >= 2;
                if past_at_least_one_cell
                    && self.zone[x] == self.zone[cur]
                    && self.bridges_overhead[x] == 0
                    && !self.solid_under_bridge[x]
                    && lands(x)
                {
                    out.push((direction, x));
                    break;
                }
                if cells_out > MOST_CELLS_A_HOP_PASSES_OVER {
                    break;
                }
                let finished_passage_across =
                    self.untouched_by_bridges(x) && self.open_walls[x] == open_walls_of_passage_under(along);
                let solid_under_crossing_bridge =
                    self.solid_under_bridge[x] && self.bridges_overhead[x] == along.other().bit();
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
