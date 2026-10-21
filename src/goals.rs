use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::maze::{walk_length, Maze};

pub const LEVELS_PER_CELL: f64 = 0.26;
pub const LONG_SHORTEST_LOOPS: f64 = 0.22;
pub const MANY_FORKS: f64 = 0.19;
pub const ZONE_CONTACT: f64 = 0.15;
pub const LONGER_BRIDGES: f64 = 0.07;
pub const ZONE_COUNT: f64 = 0.05;
pub const WALKING_DISTANCE: f64 = 0.04;
pub const CROW_FLIES_CLOSENESS: f64 = 0.02;

pub const CELLS_PER_ZONE: f64 = 100.0;
pub const LOOP_IN_BOARD_SPANS_FOR_FULL_MARKS: f64 = 1.0;
pub const CONTACT_PER_CELL_FOR_FULL_MARKS: f64 = 1.0;
pub const BRIDGE_CELLS_FOR_FULL_MARKS: f64 = 6.0;
pub const WALK_IN_BOARD_SPANS_FOR_FULL_MARKS: f64 = 2.0;
pub const STEP_BASELINE: f64 = 0.05;
pub const LEANING_STRENGTH: f64 = 4.0;

#[derive(Clone, Copy, Default)]
pub struct Scores {
    pub levels_per_cell: f64,
    pub long_shortest_loops: f64,
    pub many_forks: f64,
    pub zone_contact: f64,
    pub longer_bridges: f64,
    pub zone_count: f64,
    pub walking_distance: f64,
    pub crow_flies_closeness: f64,
}

impl Scores {
    pub fn weighted(&self) -> [(&'static str, f64, f64); 8] {
        [
            ("levels per cell", LEVELS_PER_CELL, self.levels_per_cell),
            ("long shortest loops", LONG_SHORTEST_LOOPS, self.long_shortest_loops),
            ("many forks", MANY_FORKS, self.many_forks),
            ("zone contact", ZONE_CONTACT, self.zone_contact),
            ("longer bridges", LONGER_BRIDGES, self.longer_bridges),
            ("zone count", ZONE_COUNT, self.zone_count),
            ("walking distance", WALKING_DISTANCE, self.walking_distance),
            ("crow-flies closeness", CROW_FLIES_CLOSENESS, self.crow_flies_closeness),
        ]
    }

    pub fn total(&self) -> f64 {
        self.weighted().iter().map(|(_, weight, score)| weight * score).sum()
    }

    pub fn add(&mut self, other: &Scores) {
        self.levels_per_cell += other.levels_per_cell;
        self.long_shortest_loops += other.long_shortest_loops;
        self.many_forks += other.many_forks;
        self.zone_contact += other.zone_contact;
        self.longer_bridges += other.longer_bridges;
        self.zone_count += other.zone_count;
        self.walking_distance += other.walking_distance;
        self.crow_flies_closeness += other.crow_flies_closeness;
    }
}

fn board_span(m: &Maze) -> f64 {
    (m.rows + m.cols) as f64
}

pub fn target_zone_count(rows: usize, cols: usize) -> usize {
    ((rows * cols) as f64 / CELLS_PER_ZONE).round().max(1.0) as usize
}

pub fn zone_count_score(rows: usize, cols: usize, zones: usize) -> f64 {
    let target = target_zone_count(rows, cols) as f64;
    (1.0 - (zones as f64 - target).abs() / target).max(0.0)
}

pub fn loop_score(m: &Maze, cells_round: usize) -> f64 {
    (cells_round as f64 / (LOOP_IN_BOARD_SPANS_FOR_FULL_MARKS * board_span(m))).min(1.0)
}

pub fn bridge_score(cells_passed_over: usize) -> f64 {
    (cells_passed_over as f64 / BRIDGE_CELLS_FOR_FULL_MARKS).min(1.0)
}

pub fn walking_score(m: &Maze, cells_walked: usize) -> f64 {
    (cells_walked as f64 / (WALK_IN_BOARD_SPANS_FOR_FULL_MARKS * board_span(m))).min(1.0)
}

pub fn crow_flies_score(m: &Maze, a: usize, b: usize) -> f64 {
    let (dr, dc) = ((a / m.cols).abs_diff(b / m.cols) as f64, (a % m.cols).abs_diff(b % m.cols) as f64);
    let diagonal = ((m.rows * m.rows + m.cols * m.cols) as f64).sqrt();
    1.0 - (dr * dr + dc * dc).sqrt() / diagonal
}

pub fn score(m: &Maze) -> Scores {
    let n = m.zone.len();
    let maze_cells = (0..n).filter(|&c| !m.is_solid(c)).count().max(1);
    let levels: usize = (0..n).map(|c| if m.is_solid(c) { 0 } else if m.bridges_overhead[c] != 0 { 2 } else { 1 }).sum();
    let forks = (0..n).filter(|&c| m.degree(c) >= 3).count();
    let route_walked = m.solve().map_or(0, |route| walk_length(&route, m.cols));
    let mean_bridge = if m.bridges.is_empty() {
        0.0
    } else {
        m.bridges.iter().map(|b| bridge_score(b.spans.len())).sum::<f64>() / m.bridges.len() as f64
    };
    Scores {
        levels_per_cell: levels as f64 / (2 * n) as f64,
        long_shortest_loops: mean_shortest_loop_score(m),
        many_forks: forks as f64 / maze_cells as f64,
        zone_contact: (weakest_zone_contact(m) / CONTACT_PER_CELL_FOR_FULL_MARKS).min(1.0),
        longer_bridges: mean_bridge,
        zone_count: zone_count_score(m.rows, m.cols, m.zone_count),
        walking_distance: walking_score(m, route_walked),
        crow_flies_closeness: crow_flies_score(m, m.start, m.goal),
    }
}

fn mean_shortest_loop_score(m: &Maze) -> f64 {
    let mut total = 0.0;
    let mut on_a_loop = 0;
    for a in 0..m.zone.len() {
        for b in m.links(a).filter(|&b| b > a).collect::<Vec<_>>() {
            let rest_of_the_way = walking_distance_without_link(m, a, b);
            if rest_of_the_way != usize::MAX {
                total += loop_score(m, rest_of_the_way + m.cells_walked_between(a, b));
                on_a_loop += 1;
            }
        }
    }
    if on_a_loop == 0 { 0.0 } else { total / on_a_loop as f64 }
}

fn walking_distance_without_link(m: &Maze, a: usize, b: usize) -> usize {
    let mut dist = vec![usize::MAX; m.zone.len()];
    dist[a] = 0;
    let mut heap = BinaryHeap::from([Reverse((0, a))]);
    while let Some(Reverse((d, x))) = heap.pop() {
        if x == b {
            return d;
        }
        if d > dist[x] {
            continue;
        }
        for y in m.links(x) {
            if (x, y) == (a, b) || (x, y) == (b, a) {
                continue;
            }
            let through_x = d + m.cells_walked_between(x, y);
            if through_x < dist[y] {
                dist[y] = through_x;
                heap.push(Reverse((through_x, y)));
            }
        }
    }
    usize::MAX
}

fn weakest_zone_contact(m: &Maze) -> f64 {
    let mut contact = vec![0usize; m.zone_count];
    let mut cells = vec![0usize; m.zone_count];
    for c in (0..m.zone.len()).filter(|&c| !m.is_solid(c)) {
        let z = m.zone[c] as usize;
        cells[z] += 1;
        for d in 0..4 {
            if m.step(c, d).is_some_and(|x| !m.is_solid(x) && m.zone[x] as usize != z) {
                contact[z] += 1;
            }
        }
    }
    for b in &m.bridges {
        let over = m.zone[b.top_or_left] as usize;
        for &x in &b.spans {
            let under = m.zone[x] as usize;
            if under != over {
                contact[over] += 1;
                contact[under] += 1;
            }
        }
    }
    (0..m.zone_count).filter(|&z| cells[z] > 0).map(|z| contact[z] as f64 / cells[z] as f64).fold(f64::MAX, f64::min)
}
