use crate::maze::{opposite, Maze};

const MOST_STRAIGHT_CELLS_BETWEEN_TWO_TURNS: usize = 1;

#[derive(Clone, Copy, Default, Debug)]
pub struct ShapeCensus {
    pub path_cells: usize,
    pub junctions: usize,
    pub c_bends: usize,
    pub s_bends: usize,
}

impl ShapeCensus {
    pub fn add(&mut self, other: &ShapeCensus) {
        self.path_cells += other.path_cells;
        self.junctions += other.junctions;
        self.c_bends += other.c_bends;
        self.s_bends += other.s_bends;
    }

    pub fn per_hundred_cells(&self, count: usize) -> f64 {
        100.0 * count as f64 / self.path_cells.max(1) as f64
    }
}

fn corridor_exits(m: &Maze, cell: usize) -> Option<[usize; 2]> {
    if !m.bridged_to[cell].is_empty() || m.bridges_overhead[cell] != 0 {
        return None;
    }
    let exits: Vec<usize> = (0..4).filter(|&d| m.is_open(cell, d)).collect();
    match exits.as_slice() {
        [a, b] => Some([*a, *b]),
        _ => None,
    }
}

fn is_turn(exits: [usize; 2]) -> bool {
    exits[0] != opposite(exits[1])
}

pub fn count_shapes(m: &Maze) -> ShapeCensus {
    let mut census = ShapeCensus::default();
    for cell in 0..m.zone.len() {
        census.path_cells += 1;
        if m.degree(cell) >= 3 {
            census.junctions += 1;
        }
        let Some(exits) = corridor_exits(m, cell) else { continue };
        if !is_turn(exits) {
            continue;
        }
        for (i, &heading) in exits.iter().enumerate() {
            let heading_before_first_turn = opposite(exits[1 - i]);
            let mut x = cell;
            let mut heading = heading;
            for _ in 0..=MOST_STRAIGHT_CELLS_BETWEEN_TWO_TURNS {
                let Some(next) = m.step(x, heading) else { break };
                let Some(next_exits) = corridor_exits(m, next) else { break };
                let way_out = if next_exits[0] == opposite(heading) { next_exits[1] } else { next_exits[0] };
                if is_turn(next_exits) {
                    if next > cell {
                        if way_out == opposite(heading_before_first_turn) {
                            census.c_bends += 1;
                        } else if way_out == heading_before_first_turn {
                            census.s_bends += 1;
                        }
                    }
                    break;
                }
                x = next;
                heading = way_out;
            }
        }
    }
    census
}
