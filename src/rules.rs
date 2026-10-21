use std::collections::{HashSet, VecDeque};

use crate::maze::{opposite, Maze, Orient, CROSSING, EAST, NORTH, SOUTH, WEST};

struct RederivedFromFinishedMaze<'a> {
    m: &'a Maze,
    landing_directions: Vec<Vec<usize>>,
    bridge_bits_overhead: Vec<u8>,
    landings: HashSet<usize>,
    floorless: Vec<bool>,
}

impl RederivedFromFinishedMaze<'_> {
    fn at(&self, c: usize) -> (usize, usize) {
        (c / self.m.cols, c % self.m.cols)
    }

    fn ways_out(&self, c: usize) -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter(|&d| self.m.is_open(c, d)).collect();
        out.extend(&self.landing_directions[c]);
        out
    }
}

pub fn broken_rules(m: &Maze) -> Vec<String> {
    let mut broken = Vec::new();
    openings_are_two_sided_and_on_the_board(m, &mut broken);
    let mut r = bridges_run_straight_over_the_cells_listed(m, &mut broken);
    under_a_single_bridge_solid_or_one_straight_passage(&mut r, &mut broken);
    no_two_passages_side_by_side_and_something_under_every_bridge(&r, &mut broken);
    no_opening_into_a_floorless_cell(&r, &mut broken);
    start_and_finish_are_the_only_dead_ends(&r, &mut broken);
    no_lone_pillar_of_wall(m, &mut broken);
    every_floor_reachable_and_zones_joined_only_at_their_crossings(&r, &mut broken);
    broken
}

fn openings_are_two_sided_and_on_the_board(m: &Maze, broken: &mut Vec<String>) {
    let at = |c: usize| (c / m.cols, c % m.cols);
    for c in 0..m.rows * m.cols {
        for d in 0..4 {
            if !m.is_open(c, d) {
                continue;
            }
            match m.step(c, d) {
                None => broken.push(format!("opening off the board at {:?}", at(c))),
                Some(x) if !m.is_open(x, opposite(d)) => broken.push(format!("one-sided opening at {:?}", at(c))),
                _ => {}
            }
        }
    }
}

fn bridges_run_straight_over_the_cells_listed<'a>(m: &'a Maze, broken: &mut Vec<String>) -> RederivedFromFinishedMaze<'a> {
    let n = m.rows * m.cols;
    let mut r = RederivedFromFinishedMaze {
        m,
        landing_directions: vec![Vec::new(); n],
        bridge_bits_overhead: vec![0; n],
        landings: HashSet::new(),
        floorless: vec![false; n],
    };
    for b in &m.bridges {
        let ((ra, ca), (rb, cb)) = (r.at(b.top_or_left), r.at(b.bottom_or_right));
        let (toward_b, toward_a, o) = if ra == rb && ca < cb {
            (EAST, WEST, Orient::H)
        } else if ca == cb && ra < rb {
            (SOUTH, NORTH, Orient::V)
        } else {
            broken.push(format!("bridge {:?}-{:?} is not straight", r.at(b.top_or_left), r.at(b.bottom_or_right)));
            continue;
        };
        if o != b.orientation {
            broken.push(format!("bridge {:?} has the wrong orientation", r.at(b.top_or_left)));
        }
        let mut between = Vec::new();
        let mut x = m.step(b.top_or_left, toward_b).unwrap();
        while x != b.bottom_or_right {
            between.push(x);
            x = m.step(x, toward_b).unwrap();
        }
        if between.is_empty() || between != b.spans {
            broken.push(format!("bridge {:?} spans the wrong cells", r.at(b.top_or_left)));
        }
        r.landing_directions[b.top_or_left].push(toward_b);
        r.landing_directions[b.bottom_or_right].push(toward_a);
        r.landings.insert(b.top_or_left);
        r.landings.insert(b.bottom_or_right);
        for &x in &between {
            if r.bridge_bits_overhead[x] & o.bit() != 0 {
                broken.push(format!("parallel bridges over {:?}", r.at(x)));
            }
            r.bridge_bits_overhead[x] |= o.bit();
        }
    }
    r
}

fn under_a_single_bridge_solid_or_one_straight_passage(r: &mut RederivedFromFinishedMaze, broken: &mut Vec<String>) {
    for x in 0..r.floorless.len() {
        let overhead = r.bridge_bits_overhead[x];
        if overhead == 0 {
            continue;
        }
        if r.landings.contains(&x) {
            broken.push(format!("bridge landing {:?} is under another bridge", r.at(x)));
        }
        if overhead == CROSSING || r.ways_out(x).is_empty() {
            if !r.ways_out(x).is_empty() {
                broken.push(format!("bridge crossing {:?} has floor openings", r.at(x)));
            }
            r.floorless[x] = true;
        } else {
            let straight_across: Vec<usize> =
                if overhead == Orient::H.bit() { vec![NORTH, SOUTH] } else { vec![WEST, EAST] };
            let mut got = r.ways_out(x);
            got.sort();
            if got != straight_across {
                broken.push(format!("cell under bridge {:?} has openings {:?}", r.at(x), got));
            }
        }
    }
}

fn no_two_passages_side_by_side_and_something_under_every_bridge(r: &RederivedFromFinishedMaze, broken: &mut Vec<String>) {
    for b in &r.m.bridges {
        for pair in b.spans.windows(2) {
            if !r.floorless[pair[0]] && !r.floorless[pair[1]] {
                broken.push(format!("two passages side by side under a bridge at {:?}", r.at(pair[0])));
            }
        }
        if b.spans.iter().all(|&x| r.floorless[x] && r.bridge_bits_overhead[x] != CROSSING) {
            broken.push(format!("bridge {:?} over nothing but solid ground", r.at(b.top_or_left)));
        }
    }
}

fn no_opening_into_a_floorless_cell(r: &RederivedFromFinishedMaze, broken: &mut Vec<String>) {
    for c in 0..r.floorless.len() {
        if r.floorless[c] && r.m.open_walls[c] != 0 {
            broken.push(format!("opening into floorless cell {:?}", r.at(c)));
        }
    }
}

fn start_and_finish_are_the_only_dead_ends(r: &RederivedFromFinishedMaze, broken: &mut Vec<String>) {
    let m = r.m;
    for x in [m.start, m.goal] {
        if r.ways_out(x).len() != 1 {
            broken.push(format!("start/finish {:?} should have one way out, has {:?}", r.at(x), r.ways_out(x)));
        }
    }
    for x in 0..r.floorless.len() {
        if r.bridge_bits_overhead[x] != 0 || x == m.start || x == m.goal {
            continue;
        }
        match r.ways_out(x).len() {
            0 => broken.push(format!("sealed cell {:?}", r.at(x))),
            1 => broken.push(format!("dead end at {:?}", r.at(x))),
            _ => {}
        }
    }
}

fn no_lone_pillar_of_wall(m: &Maze, broken: &mut Vec<String>) {
    let cols = m.cols;
    for c in 0..m.rows * cols {
        let (r, k) = (c / cols, c % cols);
        let square_open_all_round = r + 1 < m.rows
            && k + 1 < cols
            && m.is_open(c, EAST)
            && m.is_open(c, SOUTH)
            && m.is_open(c + 1, SOUTH)
            && m.is_open(c + cols, EAST);
        if square_open_all_round {
            broken.push(format!("lone pillar of wall inside {:?}", (r, k)));
        }
    }
}

fn every_floor_reachable_and_zones_joined_only_at_their_crossings(r: &RederivedFromFinishedMaze, broken: &mut Vec<String>) {
    let m = r.m;
    let n = r.floorless.len();
    let mut neighbours: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut links = Vec::new();
    for c in 0..n {
        for d in [SOUTH, EAST] {
            if m.is_open(c, d) {
                links.push((c, m.step(c, d).unwrap()));
            }
        }
    }
    links.extend(m.bridges.iter().map(|b| (b.top_or_left, b.bottom_or_right)));
    for &(a, b) in &links {
        neighbours[a].push(b);
        neighbours[b].push(a);
    }
    let reachable_from_start_without = |cut: Option<(usize, usize)>| -> Vec<bool> {
        let mut seen = vec![false; n];
        seen[m.start] = true;
        let mut queue = VecDeque::from([m.start]);
        while let Some(x) = queue.pop_front() {
            for &y in &neighbours[x] {
                if cut == Some((x.min(y), x.max(y))) || seen[y] {
                    continue;
                }
                seen[y] = true;
                queue.push_back(y);
            }
        }
        seen
    };
    let total = (0..n).filter(|&c| !r.floorless[c]).count();
    let reached = reachable_from_start_without(None).iter().filter(|&&v| v).count();
    if reached != total {
        broken.push(format!("only {reached} of {total} cells reachable"));
    }
    let crossings: HashSet<(usize, usize)> = m.zone_crossings.iter().copied().collect();
    let mut other_links_between_zones: Vec<(usize, usize)> = links
        .iter()
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .filter(|&(a, b)| m.zone[a] != m.zone[b] && !crossings.contains(&(a, b)))
        .collect();
    other_links_between_zones.sort();
    if !other_links_between_zones.is_empty() {
        let shown: Vec<_> = other_links_between_zones.iter().take(3).map(|&(a, b)| (r.at(a), r.at(b))).collect();
        broken.push(format!("links between zones beyond the {} crossings: {:?}", m.zone_crossings.len(), shown));
    }
    if m.zone_crossings.len() + 1 != m.zone_count {
        broken.push(format!("{} zones but {} crossings", m.zone_count, m.zone_crossings.len()));
    }
    for &crossing in &m.zone_crossings {
        if reachable_from_start_without(Some(crossing))[m.goal] {
            broken.push(format!("zone crossing {:?} is not the only way through", (r.at(crossing.0), r.at(crossing.1))));
        }
    }
}
