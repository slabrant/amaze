use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::maze::{Maze, EAST, SOUTH};

#[derive(Clone, Copy, Default, Debug)]
pub struct LoopCensus {
    pub loops: usize,
    pub wrapping: usize,
    pub around: usize,
    pub links: usize,
    pub wrapping_links: usize,
}

impl LoopCensus {
    pub fn add(&mut self, other: &LoopCensus) {
        self.loops += other.loops;
        self.wrapping += other.wrapping;
        self.around += other.around;
        self.links += other.links;
        self.wrapping_links += other.wrapping_links;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HowItWraps {
    Nothing,
    OnlyAcrossAnotherZone,
    AroundAnotherZonesPath,
}

struct LinkWithinZone {
    a: usize,
    b: usize,
    length: usize,
    passes_over: Vec<usize>,
}

impl LinkWithinZone {
    fn far_end_from(&self, x: usize) -> usize {
        if self.a == x { self.b } else { self.a }
    }
}

pub fn count_loops(m: &Maze) -> LoopCensus {
    let (rows, cols) = (m.rows, m.cols);
    let n = rows * cols;
    let at = |c: usize| (c / cols, c % cols);
    let mut links: Vec<LinkWithinZone> = Vec::new();
    for c in 0..n {
        for d in [SOUTH, EAST] {
            if m.is_open(c, d) {
                let x = m.step(c, d).unwrap();
                if m.zone[c] == m.zone[x] {
                    links.push(LinkWithinZone { a: c, b: x, length: 1, passes_over: Vec::new() });
                }
            }
        }
    }
    let mut zones_bridging_over: Vec<Vec<u8>> = vec![Vec::new(); n];
    let mut bridges_over = vec![0usize; n];
    for b in &m.bridges {
        for &x in &b.spans {
            zones_bridging_over[x].push(m.zone[b.top_or_left]);
            bridges_over[x] += 1;
        }
        if m.zone[b.top_or_left] == m.zone[b.bottom_or_right] {
            links.push(LinkWithinZone { a: b.top_or_left, b: b.bottom_or_right, length: b.spans.len() + 1, passes_over: b.spans.clone() });
        }
    }
    let mut links_at: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, l) in links.iter().enumerate() {
        links_at[l.a].push(i);
        links_at[l.b].push(i);
    }

    let shortest_links_from_a_to_b_avoiding = |a: usize, b: usize, avoided: usize| -> Option<Vec<usize>> {
        let mut dist = vec![usize::MAX; n];
        let mut arrived_by = vec![usize::MAX; n];
        dist[a] = 0;
        let mut heap = BinaryHeap::from([Reverse((0usize, a))]);
        while let Some(Reverse((d, x))) = heap.pop() {
            if x == b {
                break;
            }
            if d > dist[x] {
                continue;
            }
            for &i in &links_at[x] {
                if i == avoided {
                    continue;
                }
                let l = &links[i];
                let y = l.far_end_from(x);
                if d + l.length < dist[y] {
                    dist[y] = d + l.length;
                    arrived_by[y] = i;
                    heap.push(Reverse((d + l.length, y)));
                }
            }
        }
        if dist[b] == usize::MAX {
            return None;
        }
        let mut taken = Vec::new();
        let mut x = b;
        while x != a {
            taken.push(arrived_by[x]);
            x = links[arrived_by[x]].far_end_from(x);
        }
        Some(taken)
    };

    let how_it_wraps = |z: u8, from: usize, links_in_order_round: &[usize]| -> HowItWraps {
        let mut corners = vec![from];
        let mut x = from;
        let mut across = false;
        for &i in links_in_order_round {
            let l = &links[i];
            across |= l.passes_over.iter().any(|&c| m.zone[c] != z && (m.open_walls[c] != 0 || bridges_over[c] > 1));
            x = l.far_end_from(x);
            corners.push(x);
        }
        across |= corners.iter().any(|&c| zones_bridging_over[c].iter().any(|&w| w != z));
        let (mut r0, mut r1, mut c0, mut c1) = (rows, 0, cols, 0);
        for &v in &corners {
            let (r, c) = at(v);
            (r0, r1, c0, c1) = (r0.min(r), r1.max(r), c0.min(c), c1.max(c));
        }
        let upright_sides: Vec<(usize, f64, f64)> = corners
            .windows(2)
            .filter(|w| at(w[0]).1 == at(w[1]).1)
            .map(|w| {
                let (ra, rb) = (at(w[0]).0 as f64, at(w[1]).0 as f64);
                (at(w[0]).1, ra.min(rb), ra.max(rb))
            })
            .collect();
        let inside = |r: usize, c: usize| {
            let just_below_middle = r as f64 + 0.01;
            let sides_to_the_right = upright_sides
                .iter()
                .filter(|&&(k, lo, hi)| k > c && lo < just_below_middle && just_below_middle < hi)
                .count();
            sides_to_the_right % 2 == 1
        };
        let around = (r0..=r1).any(|r| (c0..=c1).any(|c| m.zone[r * cols + c] != z && inside(r, c)));
        if around {
            HowItWraps::AroundAnotherZonesPath
        } else if across {
            HowItWraps::OnlyAcrossAnotherZone
        } else {
            HowItWraps::Nothing
        }
    };

    let mut distinct_loops: HashMap<Vec<usize>, HowItWraps> = HashMap::new();
    let mut census = LoopCensus::default();
    for (i, l) in links.iter().enumerate() {
        let Some(mut round) = shortest_links_from_a_to_b_avoiding(l.a, l.b, i) else { continue };
        round.reverse();
        round.push(i);
        let mut same_loop_whichever_link_it_was_found_from = round.clone();
        same_loop_whichever_link_it_was_found_from.sort_unstable();
        let wraps = *distinct_loops
            .entry(same_loop_whichever_link_it_was_found_from)
            .or_insert_with(|| how_it_wraps(m.zone[l.a], l.a, &round));
        census.links += 1;
        census.wrapping_links += (wraps != HowItWraps::Nothing) as usize;
    }
    census.loops = distinct_loops.len();
    census.wrapping = distinct_loops.values().filter(|&&w| w != HowItWraps::Nothing).count();
    census.around = distinct_loops.values().filter(|&&w| w == HowItWraps::AroundAnotherZonesPath).count();
    census
}
