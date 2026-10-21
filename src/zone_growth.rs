use std::cmp::Reverse;
use std::collections::VecDeque;

use crate::maze::{Maze, Orient, DC, DR, EAST, NO_ZONE, SOUTH, STRAND_BLOCK_CELLS, WEST};
use crate::seeded_rng::Rng;

const CHANCE_TO_GROW_FROM_NEWEST_BLOCK: f64 = 0.2;
const PULL_TO_STEP_INTO_OPEN_GROUND: f64 = 1.0;
const PULL_TO_CROSS_OVER_A_STRAND: f64 = 20.0;
const PULL_TO_SLIP_UNDER_A_STRAND: f64 = 20.0;
const PULL_TO_CLOSE_A_RING_ROUND_A_STRAND: f64 = 40.0;
const MOST_BLOCKS_ONE_CROSSING_SPANS: usize = 3;
const FEWEST_BLOCKS_ROUND_A_RING: usize = 4;
const EDGE_WIGGLE_TRIES_PER_CELL: usize = 3;
const SMALLEST_SHARE_OF_SMALLEST_ZONE_KEPT_WHILE_WIGGLING: f64 = 0.8;
const ZONE_ORDER_SEARCH_STEPS: usize = 100_000;

fn block_spans_last_taking_any_left_over(cells: usize) -> Vec<(usize, usize)> {
    let b = STRAND_BLOCK_CELLS;
    let k = (cells / b).max(1);
    (0..k).map(|i| (b * i, if i + 1 == k { cells } else { b * i + b })).collect()
}

struct BlockGrid {
    rows: Vec<(usize, usize)>,
    cols: Vec<(usize, usize)>,
    owner: Vec<u8>,
    cells: Vec<Vec<usize>>,
    same_strand_blocks_across_bridges: Vec<Vec<usize>>,
}

impl BlockGrid {
    fn block_cols(&self) -> usize {
        self.cols.len()
    }

    fn step(&self, b: usize, d: usize) -> Option<usize> {
        let i = (b / self.block_cols()) as isize + DR[d];
        let j = (b % self.block_cols()) as isize + DC[d];
        if i < 0 || j < 0 || i >= self.rows.len() as isize || j >= self.block_cols() as isize {
            None
        } else {
            Some(i as usize * self.block_cols() + j as usize)
        }
    }

    fn own_blocks_beside(&self, b: usize, z: u8) -> usize {
        (0..4).filter_map(|d| self.step(b, d)).filter(|&x| self.owner[x] == z).count()
    }

    fn strand_neighbours(&self, b: usize, z: u8) -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter_map(|d| self.step(b, d)).filter(|&x| self.owner[x] == z).collect();
        out.extend(&self.same_strand_blocks_across_bridges[b]);
        out
    }

    fn closes_ring_round_another_strand(&self, z: u8, a: usize, b: usize, via: usize, b_across_a_bridge: bool) -> bool {
        let mut toward_b = vec![usize::MAX; self.owner.len()];
        toward_b[b] = b;
        let mut queue = VecDeque::from([b]);
        while let Some(x) = queue.pop_front() {
            if x == a {
                break;
            }
            for y in self.strand_neighbours(x, z) {
                if toward_b[y] == usize::MAX {
                    toward_b[y] = x;
                    queue.push_back(y);
                }
            }
        }
        if toward_b[a] == usize::MAX {
            return false;
        }
        let mut ring = vec![a];
        let mut crosses_a_bridge = b_across_a_bridge;
        while *ring.last().unwrap() != b {
            let x = *ring.last().unwrap();
            crosses_a_bridge |= self.same_strand_blocks_across_bridges[x].contains(&toward_b[x]);
            ring.push(toward_b[x]);
        }
        ring.push(via);
        ring.push(a);
        if ring.len() - 1 < FEWEST_BLOCKS_ROUND_A_RING {
            return false;
        }
        if crosses_a_bridge {
            return true;
        }
        let block_cols = self.block_cols();
        let upright_sides: Vec<(usize, f64, f64)> = ring
            .windows(2)
            .filter(|w| w[0] % block_cols == w[1] % block_cols)
            .map(|w| {
                let (ra, rb) = ((w[0] / block_cols) as f64, (w[1] / block_cols) as f64);
                (w[0] % block_cols, ra.min(rb), ra.max(rb))
            })
            .collect();
        (0..self.owner.len()).any(|x| {
            let w = self.owner[x];
            let just_below_middle = (x / block_cols) as f64 + 0.01;
            let sides_to_the_right = upright_sides
                .iter()
                .filter(|&&(k, lo, hi)| k > x % block_cols && lo < just_below_middle && just_below_middle < hi)
                .count();
            w != NO_ZONE && w != z && sides_to_the_right % 2 == 1
        })
    }
}

enum Growth {
    IntoOpenGround(usize),
    OverOtherStrands { from: usize, to: usize },
    UnderAnotherStrand { into: usize, its_block_before: usize, its_block_after: usize },
}

impl Maze {
    fn zone_keeps_its_promises(&self, z: u8) -> bool {
        let mut first = None;
        let mut count = 0;
        for c in 0..self.zone.len() {
            if self.zone[c] != z || self.floorless(c) {
                continue;
            }
            count += 1;
            first.get_or_insert(c);
            let ways = (0..4).filter(|&d| self.can_open(c, d, true).is_some()).count() + self.bridged_to[c].len();
            if ways < 2 {
                return false;
            }
        }
        let Some(first) = first else { return false };
        if self.has_square_forced_round_a_lone_pillar(z) {
            return false;
        }
        let mut seen = vec![false; self.zone.len()];
        seen[first] = true;
        let mut queue = VecDeque::from([first]);
        let mut reached = 1;
        while let Some(c) = queue.pop_front() {
            let walls = (0..4).filter_map(|d| self.can_open(c, d, true));
            for n in walls.chain(self.bridged_to[c].iter().copied()).collect::<Vec<_>>() {
                if !seen[n] {
                    seen[n] = true;
                    reached += 1;
                    queue.push_back(n);
                }
            }
        }
        reached == count
    }

    fn has_square_forced_round_a_lone_pillar(&self, z: u8) -> bool {
        (0..self.zone.len()).any(|c| self.square_forced_round_a_lone_pillar(c, z))
    }

    fn square_forced_round_a_lone_pillar(&self, top_left: usize, z: u8) -> bool {
        let (c, cols) = (top_left, self.cols);
        if c % cols + 1 >= cols || c / cols + 1 >= self.rows {
            return false;
        }
        let square = [c, c + 1, c + cols, c + cols + 1];
        if square.iter().any(|&x| self.zone[x] != z || self.floorless(x)) {
            return false;
        }
        let opens_only_into_the_square = |x: usize| {
            !(0..4).any(|d| self.can_open(x, d, true).is_some_and(|y| !square.contains(&y))) && self.bridged_to[x].is_empty()
        };
        let diagonal = |p: usize, q: usize| opens_only_into_the_square(square[p]) && opens_only_into_the_square(square[q]);
        diagonal(0, 3) || diagonal(1, 2)
    }

    pub(crate) fn grow_zones(&mut self, rng: &mut Rng) -> bool {
        let n = self.zone_count;
        let rows = block_spans_last_taking_any_left_over(self.rows);
        let cols = block_spans_last_taking_any_left_over(self.cols);
        let width = self.cols;
        let mut cells = Vec::new();
        for &(r0, r1) in &rows {
            for &(c0, c1) in &cols {
                cells.push((r0..r1).flat_map(|r| (c0..c1).map(move |c| r * width + c)).collect());
            }
        }
        let blocks = cells.len();
        let mut grid = BlockGrid {
            rows,
            cols,
            owner: vec![NO_ZONE; blocks],
            cells,
            same_strand_blocks_across_bridges: vec![Vec::new(); blocks],
        };

        let seeds = Self::seeds_as_far_apart_as_they_can_be(&grid, n, rng);
        let mut size = vec![1usize; n];
        let mut newest = seeds.clone();
        for (z, &b) in seeds.iter().enumerate() {
            self.claim_block(&mut grid, b, z as u8);
        }
        let mut still_growing = vec![true; n];
        loop {
            let live: Vec<usize> = (0..n).filter(|&z| still_growing[z]).collect();
            let Some(low) = live.iter().map(|&z| size[z]).min() else { break };
            let smallest: Vec<usize> = live.into_iter().filter(|&z| size[z] == low).collect();
            let z = rng.pick(&smallest);
            let mut options = if rng.chance(CHANCE_TO_GROW_FROM_NEWEST_BLOCK) {
                self.ways_to_grow(&grid, z as u8, &[newest[z]])
            } else {
                Vec::new()
            };
            if options.is_empty() {
                let own: Vec<usize> = (0..blocks).filter(|&b| grid.owner[b] == z as u8).collect();
                options = self.ways_to_grow(&grid, z as u8, &own);
            }
            let mut grown = false;
            while !options.is_empty() {
                let pulls: Vec<f64> = options.iter().map(|o| o.1).collect();
                let (growth, _) = options.swap_remove(rng.index_by_weight(&pulls));
                if let Some(block) = self.grow_if_zones_stay_sound(&mut grid, z as u8, growth, rng) {
                    newest[z] = block;
                    size[z] += 1;
                    grown = true;
                    break;
                }
            }
            if !grown {
                still_growing[z] = false;
            }
        }

        if !self.give_unreached_blocks_to_the_smallest_sound_neighbour(&mut grid, &mut size, rng) {
            return false;
        }
        self.wiggle_straight_zone_edges(rng);
        self.number_zones_so_each_touches_the_next(rng)
    }

    fn seeds_as_far_apart_as_they_can_be(grid: &BlockGrid, n: usize, rng: &mut Rng) -> Vec<usize> {
        let blocks = grid.owner.len();
        let block_cols = grid.block_cols();
        let gap = |a: usize, b: usize| (a / block_cols).abs_diff(b / block_cols) + (a % block_cols).abs_diff(b % block_cols);
        let mut seeds = vec![rng.below(blocks)];
        while seeds.len() < n {
            let farthest = (0..blocks)
                .filter(|b| !seeds.contains(b))
                .max_by_key(|&b| (seeds.iter().map(|&s| gap(s, b)).min().unwrap(), rng.below(1000)))
                .unwrap();
            seeds.push(farthest);
        }
        rng.shuffle(&mut seeds);
        seeds
    }

    fn give_unreached_blocks_to_the_smallest_sound_neighbour(
        &mut self,
        grid: &mut BlockGrid,
        size: &mut [usize],
        rng: &mut Rng,
    ) -> bool {
        loop {
            let mut unowned: Vec<usize> = (0..grid.owner.len()).filter(|&b| grid.owner[b] == NO_ZONE).collect();
            if unowned.is_empty() {
                return true;
            }
            rng.shuffle(&mut unowned);
            let mut progress = false;
            for b in unowned {
                let mut beside: Vec<u8> =
                    (0..4).filter_map(|d| grid.step(b, d)).map(|x| grid.owner[x]).filter(|&z| z != NO_ZONE).collect();
                beside.sort_by_key(|&z| (size[z as usize], z));
                beside.dedup();
                for z in beside {
                    let saved = self.clone();
                    self.claim_block(grid, b, z);
                    if self.zone_keeps_its_promises(z) {
                        size[z as usize] += 1;
                        progress = true;
                        break;
                    }
                    *self = saved;
                    grid.owner[b] = NO_ZONE;
                }
            }
            if !progress {
                return false;
            }
        }
    }

    fn claim_block(&mut self, grid: &mut BlockGrid, b: usize, z: u8) {
        grid.owner[b] = z;
        for &c in &grid.cells[b] {
            self.zone[c] = z;
        }
    }

    fn ways_to_grow(&self, grid: &BlockGrid, z: u8, from: &[usize]) -> Vec<(Growth, f64)> {
        let mut out = Vec::new();
        for &s in from {
            for d in 0..4 {
                let Some(t) = grid.step(s, d) else { continue };
                if grid.owner[t] == NO_ZONE {
                    if grid.own_blocks_beside(t, z) == 2 {
                        let own: Vec<usize> =
                            (0..4).filter_map(|e| grid.step(t, e)).filter(|&x| grid.owner[x] == z).collect();
                        if grid.closes_ring_round_another_strand(z, own[0], own[1], t, false) {
                            out.push((Growth::IntoOpenGround(t), PULL_TO_CLOSE_A_RING_ROUND_A_STRAND));
                        }
                        continue;
                    }
                    if grid.own_blocks_beside(t, z) != 1 {
                        continue;
                    }
                    out.push((Growth::IntoOpenGround(t), PULL_TO_STEP_INTO_OPEN_GROUND));
                    let first_side = if d == WEST || d == EAST { 0 } else { WEST };
                    if let (Some(before), Some(after)) = (grid.step(t, first_side), grid.step(t, first_side + 1)) {
                        let w = grid.owner[before];
                        if w != NO_ZONE && w != z && grid.owner[after] == w {
                            let growth = Growth::UnderAnotherStrand { into: t, its_block_before: before, its_block_after: after };
                            out.push((growth, PULL_TO_SLIP_UNDER_A_STRAND));
                        }
                    }
                    continue;
                }
                let mut x = t;
                for blocks_spanned in 1..=MOST_BLOCKS_ONE_CROSSING_SPANS {
                    if grid.owner[x] == z || grid.owner[x] == NO_ZONE {
                        break;
                    }
                    let Some(beyond) = grid.step(x, d) else { break };
                    if grid.owner[beyond] == NO_ZONE {
                        let shorter_reads_better = (blocks_spanned * blocks_spanned) as f64;
                        match grid.own_blocks_beside(beyond, z) {
                            0 => out.push((
                                Growth::OverOtherStrands { from: s, to: beyond },
                                PULL_TO_CROSS_OVER_A_STRAND / shorter_reads_better,
                            )),
                            1 => {
                                let own = (0..4).filter_map(|e| grid.step(beyond, e)).find(|&x| grid.owner[x] == z).unwrap();
                                if grid.closes_ring_round_another_strand(z, own, s, beyond, true) {
                                    out.push((
                                        Growth::OverOtherStrands { from: s, to: beyond },
                                        PULL_TO_CLOSE_A_RING_ROUND_A_STRAND / shorter_reads_better,
                                    ));
                                }
                            }
                            _ => {}
                        }
                        break;
                    }
                    x = beyond;
                }
            }
        }
        out
    }

    fn landing_pairs_on_facing_block_edges(&self, grid: &BlockGrid, from: usize, to: usize) -> Vec<(usize, usize)> {
        let block_cols = grid.block_cols();
        let (fb, tb) = ((from / block_cols, from % block_cols), (to / block_cols, to % block_cols));
        let w = self.cols;
        if fb.0 == tb.0 {
            let (c_from, c_to) = if tb.1 > fb.1 {
                (grid.cols[fb.1].1 - 1, grid.cols[tb.1].0)
            } else {
                (grid.cols[fb.1].0, grid.cols[tb.1].1 - 1)
            };
            let (r0, r1) = grid.rows[fb.0];
            (r0..r1).map(|r| (r * w + c_from, r * w + c_to)).collect()
        } else {
            let (r_from, r_to) = if tb.0 > fb.0 {
                (grid.rows[fb.0].1 - 1, grid.rows[tb.0].0)
            } else {
                (grid.rows[fb.0].0, grid.rows[tb.0].1 - 1)
            };
            let (c0, c1) = grid.cols[fb.1];
            (c0..c1).map(|c| (r_from * w + c, r_to * w + c)).collect()
        }
    }

    fn lay_bridge_keeping_zones_sound(&mut self, grid: &BlockGrid, from: usize, to: usize, rng: &mut Rng) -> bool {
        let mut lines = self.landing_pairs_on_facing_block_edges(grid, from, to);
        rng.shuffle(&mut lines);
        for (a, b) in lines {
            if !self.untouched_by_bridges(a) || !self.untouched_by_bridges(b) {
                continue;
            }
            let o = Orient::of_direction(self.direction_toward(a, b));
            let spans = self.cells_between(a.min(b), a.max(b));
            let mut must_stay_solid = 0u32;
            let mut line_is_free = true;
            for (i, &m) in spans.iter().enumerate() {
                let landing_or_parallel_bridge = !self.bridged_to[m].is_empty() || self.bridges_overhead[m] & o.bit() != 0;
                let passage_running_along_the_line = self.bridges_overhead[m] != 0 && !self.solid_under_bridge[m];
                if landing_or_parallel_bridge || passage_running_along_the_line {
                    line_is_free = false;
                } else if self.bridges_overhead[m] != 0 {
                    must_stay_solid |= 1 << i;
                }
            }
            if !line_is_free {
                continue;
            }
            let passages_under = |p: &u32| {
                let no_two_side_by_side = p & (p >> 1) == 0;
                let something_goes_under = (p | must_stay_solid) != 0;
                no_two_side_by_side && p & must_stay_solid == 0 && something_goes_under
            };
            let mut patterns: Vec<u32> = (0..1u32 << spans.len()).filter(passages_under).collect();
            rng.shuffle(&mut patterns);
            patterns.sort_by_key(|p| Reverse(p.count_ones()));
            let mut zones: Vec<u8> = spans.iter().map(|&m| self.zone[m]).collect();
            zones.push(self.zone[a]);
            zones.sort();
            zones.dedup();
            for p in patterns {
                let solid: Vec<bool> = (0..spans.len()).map(|i| p >> i & 1 == 0).collect();
                let saved = self.clone();
                self.lay_bridge(a, b, &solid);
                if zones.iter().all(|&z| self.zone_keeps_its_promises(z)) {
                    self.stats.strand_crossings += 1;
                    return true;
                }
                *self = saved;
            }
        }
        false
    }

    fn grow_if_zones_stay_sound(&mut self, grid: &mut BlockGrid, z: u8, growth: Growth, rng: &mut Rng) -> Option<usize> {
        let saved = self.clone();
        let block = match growth {
            Growth::IntoOpenGround(t) | Growth::OverOtherStrands { to: t, .. } | Growth::UnderAnotherStrand { into: t, .. } => t,
        };
        let closes_a_ring = match growth {
            Growth::IntoOpenGround(t) => grid.own_blocks_beside(t, z) == 2,
            Growth::OverOtherStrands { to, .. } => grid.own_blocks_beside(to, z) == 1,
            Growth::UnderAnotherStrand { .. } => false,
        };
        self.claim_block(grid, block, z);
        let sound = match growth {
            Growth::IntoOpenGround(_) => self.zone_keeps_its_promises(z),
            Growth::OverOtherStrands { from, to } => self.lay_bridge_keeping_zones_sound(grid, from, to, rng),
            Growth::UnderAnotherStrand { its_block_before, its_block_after, .. } => {
                self.zone_keeps_its_promises(z)
                    && self.lay_bridge_keeping_zones_sound(grid, its_block_before, its_block_after, rng)
            }
        };
        if sound {
            let bridged = match growth {
                Growth::OverOtherStrands { from, to } => Some((from, to)),
                Growth::UnderAnotherStrand { its_block_before, its_block_after, .. } => {
                    Some((its_block_before, its_block_after))
                }
                Growth::IntoOpenGround(_) => None,
            };
            if let Some((p, q)) = bridged {
                grid.same_strand_blocks_across_bridges[p].push(q);
                grid.same_strand_blocks_across_bridges[q].push(p);
            }
            self.stats.rings += closes_a_ring as usize;
            return Some(block);
        }
        *self = saved;
        grid.owner[block] = NO_ZONE;
        None
    }

    fn wiggle_straight_zone_edges(&mut self, rng: &mut Rng) {
        let n = self.zone.len();
        let mut size = vec![0usize; self.zone_count];
        for &z in &self.zone {
            size[z as usize] += 1;
        }
        let smallest_allowed =
            *size.iter().min().unwrap() as f64 * SMALLEST_SHARE_OF_SMALLEST_ZONE_KEPT_WHILE_WIGGLING;
        for _ in 0..EDGE_WIGGLE_TRIES_PER_CELL * n {
            let x = rng.below(n);
            let z = self.zone[x];
            let toward_other_zone: Vec<usize> =
                (0..4).filter(|&d| self.step(x, d).is_some_and(|m| self.zone[m] != z)).collect();
            if toward_other_zone.is_empty() || !self.untouched_by_bridges(x) {
                continue;
            }
            let d = rng.pick(&toward_other_zone);
            let w = self.zone[self.step(x, d).unwrap()];
            let mut moving = vec![x];
            let two_side_by_side_so_neither_is_left_with_one_neighbour = rng.chance(0.5);
            if two_side_by_side_so_neither_is_left_with_one_neighbour {
                let along_edge = if d == WEST || d == EAST { rng.below(2) } else { WEST + rng.below(2) };
                let Some(y) = self.step(x, along_edge) else { continue };
                let y_borders_w_the_same_way = self.step(y, d).map(|m| self.zone[m]) == Some(w);
                if self.zone[y] != z || !self.untouched_by_bridges(y) || !y_borders_w_the_same_way {
                    continue;
                }
                moving.push(y);
            }
            if ((size[z as usize] - moving.len()) as f64) < smallest_allowed {
                continue;
            }
            for &c in &moving {
                self.zone[c] = w;
            }
            if self.zone_keeps_its_promises(z) && self.zone_keeps_its_promises(w) {
                size[z as usize] -= moving.len();
                size[w as usize] += moving.len();
            } else {
                for &c in &moving {
                    self.zone[c] = z;
                }
            }
        }
    }

    fn number_zones_so_each_touches_the_next(&mut self, rng: &mut Rng) -> bool {
        let n = self.zone_count;
        let mut touch = vec![vec![false; n]; n];
        for c in 0..self.zone.len() {
            for d in [SOUTH, EAST] {
                if let Some(m) = self.can_open(c, d, false) {
                    let (a, b) = (self.zone[c] as usize, self.zone[m] as usize);
                    if a != b && self.untouched_by_bridges(c) && self.untouched_by_bridges(m) {
                        touch[a][b] = true;
                        touch[b][a] = true;
                    }
                }
            }
        }
        let mut starts: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut starts);
        let mut order = Vec::new();
        let mut steps_left = ZONE_ORDER_SEARCH_STEPS;
        let found = starts.into_iter().any(|z| {
            order = vec![z];
            extend_chain_through_every_zone(&touch, &mut order, &mut steps_left, rng)
        });
        if !found {
            return false;
        }
        let mut rank = vec![0u8; n];
        for (i, &z) in order.iter().enumerate() {
            rank[z] = i as u8;
        }
        for z in &mut self.zone {
            *z = rank[*z as usize];
        }
        true
    }
}

fn extend_chain_through_every_zone(touch: &[Vec<bool>], chain: &mut Vec<usize>, steps_left: &mut usize, rng: &mut Rng) -> bool {
    if chain.len() == touch.len() {
        return true;
    }
    if *steps_left == 0 {
        return false;
    }
    *steps_left -= 1;
    let last = *chain.last().unwrap();
    let mut next: Vec<usize> = (0..touch.len()).filter(|&z| touch[last][z] && !chain.contains(&z)).collect();
    rng.shuffle(&mut next);
    for z in next {
        chain.push(z);
        if extend_chain_through_every_zone(touch, chain, steps_left, rng) {
            return true;
        }
        chain.pop();
    }
    false
}
