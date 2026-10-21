use std::collections::{HashMap, VecDeque};

use crate::maze::{Maze, Orient};
use crate::seeded_rng::Rng;

impl Maze {
    pub(crate) fn choose_top_bridge_at_each_crossing(&mut self, rng: &mut Rng) {
        let mut bridges_crossing_at: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, b) in self.bridges.iter().enumerate() {
            for &m in &b.spans {
                if self.is_crossing(m) {
                    bridges_crossing_at.entry(m).or_default().push(i);
                }
            }
        }
        let mut neighbouring_crossings: HashMap<usize, Vec<usize>> =
            bridges_crossing_at.keys().map(|&m| (m, Vec::new())).collect();
        for b in &self.bridges {
            let along: Vec<usize> = b.spans.iter().copied().filter(|m| bridges_crossing_at.contains_key(m)).collect();
            for pair in along.windows(2) {
                neighbouring_crossings.get_mut(&pair[0]).unwrap().push(pair[1]);
                neighbouring_crossings.get_mut(&pair[1]).unwrap().push(pair[0]);
            }
        }
        let mut horizontal_on_top: HashMap<usize, bool> = HashMap::new();
        let mut crossings: Vec<usize> = bridges_crossing_at.keys().copied().collect();
        crossings.sort();
        for first in crossings {
            if horizontal_on_top.contains_key(&first) {
                continue;
            }
            horizontal_on_top.insert(first, rng.chance(0.5));
            let mut queue = VecDeque::from([first]);
            while let Some(c) = queue.pop_front() {
                let flipped = !horizontal_on_top[&c];
                for &m in &neighbouring_crossings[&c] {
                    if !horizontal_on_top.contains_key(&m) {
                        horizontal_on_top.insert(m, flipped);
                        queue.push_back(m);
                    }
                }
            }
        }
        for (m, pair) in bridges_crossing_at {
            let wanted = if horizontal_on_top[&m] { Orient::H } else { Orient::V };
            let top = pair.into_iter().find(|&i| self.bridges[i].orientation == wanted).unwrap();
            self.top_bridge_at_crossing.insert(m, top);
        }
    }
}
