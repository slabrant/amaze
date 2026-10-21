use std::collections::VecDeque;

use crate::seeded_rng::Rng;

pub const NORTH: usize = 0;
pub const SOUTH: usize = 1;
pub const WEST: usize = 2;
pub const EAST: usize = 3;
pub const DR: [isize; 4] = [-1, 1, 0, 0];
pub const DC: [isize; 4] = [0, 0, -1, 1];
pub const NO_ZONE: u8 = u8::MAX;

pub fn opposite(d: usize) -> usize {
    d ^ 1
}

pub fn open_walls_of_passage_under(bridge: Orient) -> u8 {
    match bridge {
        Orient::H => 1 << NORTH | 1 << SOUTH,
        Orient::V => 1 << WEST | 1 << EAST,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orient {
    H,
    V,
}

impl Orient {
    pub fn of_direction(d: usize) -> Orient {
        if d == WEST || d == EAST {
            Orient::H
        } else {
            Orient::V
        }
    }
    pub fn bit(self) -> u8 {
        match self {
            Orient::H => 1,
            Orient::V => 2,
        }
    }
    pub fn other(self) -> Orient {
        match self {
            Orient::H => Orient::V,
            Orient::V => Orient::H,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bridge {
    pub top_or_left: usize,
    pub bottom_or_right: usize,
    pub orientation: Orient,
    pub spans: Vec<usize>,
}

#[derive(Clone)]
pub struct Maze {
    pub rows: usize,
    pub cols: usize,
    pub zone_count: usize,
    pub zone: Vec<u8>,
    pub open_walls: Vec<u8>,
    pub bridges_overhead: Vec<u8>,
    pub bridged_to: Vec<Vec<usize>>,
    pub bridges: Vec<Bridge>,
    pub zone_crossings: Vec<(usize, usize)>,
    pub start: usize,
    pub goal: usize,
}

impl Maze {
    pub(crate) fn empty(rows: usize, cols: usize, zone_count: usize) -> Maze {
        let n = rows * cols;
        Maze {
            rows,
            cols,
            zone_count,
            zone: vec![NO_ZONE; n],
            open_walls: vec![0; n],
            bridges_overhead: vec![0; n],
            bridged_to: vec![Vec::new(); n],
            bridges: Vec::new(),
            zone_crossings: Vec::new(),
            start: 0,
            goal: 0,
        }
    }

    pub fn step(&self, c: usize, d: usize) -> Option<usize> {
        let r = (c / self.cols) as isize + DR[d];
        let k = (c % self.cols) as isize + DC[d];
        if r < 0 || k < 0 || r >= self.rows as isize || k >= self.cols as isize {
            None
        } else {
            Some(r as usize * self.cols + k as usize)
        }
    }

    pub fn overhead(&self, c: usize) -> Option<Orient> {
        [Orient::H, Orient::V].into_iter().find(|o| self.bridges_overhead[c] == o.bit())
    }

    pub fn is_open(&self, c: usize, d: usize) -> bool {
        self.open_walls[c] >> d & 1 == 1
    }

    pub fn open_wall(&mut self, c: usize, d: usize) {
        let n = self.step(c, d).unwrap();
        self.open_walls[c] |= 1 << d;
        self.open_walls[n] |= 1 << opposite(d);
    }

    pub fn degree(&self, c: usize) -> usize {
        self.open_walls[c].count_ones() as usize + self.bridged_to[c].len()
    }

    pub fn links(&self, c: usize) -> impl Iterator<Item = usize> + '_ {
        (0..4)
            .filter(move |&d| self.is_open(c, d))
            .map(move |d| self.step(c, d).unwrap())
            .chain(self.bridged_to[c].iter().copied())
    }

    pub fn openings_including_bridges(&self, c: usize) -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter(|&d| self.is_open(c, d)).collect();
        for &far_end in &self.bridged_to[c] {
            out.push(self.direction_toward(c, far_end));
        }
        out
    }

    pub fn direction_toward(&self, c: usize, p: usize) -> usize {
        let (r0, c0) = (c / self.cols, c % self.cols);
        let (r1, c1) = (p / self.cols, p % self.cols);
        if r0 == r1 {
            if c1 < c0 { WEST } else { EAST }
        } else if r1 < r0 {
            NORTH
        } else {
            SOUTH
        }
    }

    pub fn cells_between(&self, a: usize, b: usize) -> Vec<usize> {
        let d = self.direction_toward(a, b);
        let mut out = Vec::new();
        let mut x = self.step(a, d).unwrap();
        while x != b {
            out.push(x);
            x = self.step(x, d).unwrap();
        }
        out
    }

    pub fn lay_bridge(&mut self, a: usize, b: usize) {
        let (a, b) = (a.min(b), a.max(b));
        let orientation = Orient::of_direction(self.direction_toward(a, b));
        let spans = self.cells_between(a, b);
        for &m in &spans {
            self.bridges_overhead[m] |= orientation.bit();
        }
        self.bridged_to[a].push(b);
        self.bridged_to[b].push(a);
        self.bridges.push(Bridge { top_or_left: a, bottom_or_right: b, orientation, spans });
    }

    pub fn solve(&self) -> Option<Vec<usize>> {
        let mut came_from = vec![usize::MAX; self.zone.len()];
        came_from[self.start] = self.start;
        let mut queue = VecDeque::from([self.start]);
        while let Some(c) = queue.pop_front() {
            if c == self.goal {
                break;
            }
            for m in self.links(c) {
                if came_from[m] == usize::MAX {
                    came_from[m] = c;
                    queue.push_back(m);
                }
            }
        }
        if came_from[self.goal] == usize::MAX {
            return None;
        }
        let mut route = vec![self.goal];
        let mut c = self.goal;
        while c != self.start {
            c = came_from[c];
            route.push(c);
        }
        route.reverse();
        Some(route)
    }

    pub fn reachable_from_start(&self) -> usize {
        let mut seen = vec![false; self.zone.len()];
        seen[self.start] = true;
        let mut queue = VecDeque::from([self.start]);
        while let Some(c) = queue.pop_front() {
            for m in self.links(c) {
                if !seen[m] {
                    seen[m] = true;
                    queue.push_back(m);
                }
            }
        }
        seen.iter().filter(|&&s| s).count()
    }

    pub fn junctions(&self) -> usize {
        (0..self.zone.len()).filter(|&c| self.degree(c) >= 3).count()
    }
}

pub fn walk_length(route: &[usize], cols: usize) -> usize {
    route
        .windows(2)
        .map(|w| (w[0] / cols).abs_diff(w[1] / cols) + (w[0] % cols).abs_diff(w[1] % cols))
        .sum()
}

pub fn zones_by_area(rows: usize, cols: usize, rng: &mut Rng) -> usize {
    let area = rows * cols;
    let most = most_zones_that_fit(rows, cols);
    rng.from_to_inclusive((2 + area / 1000).min(most), (3 + area / 500).min(most))
}

pub fn most_zones_that_fit(rows: usize, cols: usize) -> usize {
    (rows * cols / 18).clamp(1, 64)
}
