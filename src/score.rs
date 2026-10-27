use crate::goals::{LONG_ROUTE, cells_near, zone_counts};
use crate::grid::Cell;
use crate::maze::{Content, Maze, PieceId, Routes, Step, StepCell};

pub struct Scoring {
    solution: Solution,
    counts: Vec<Vec<usize>>,
    shares: Vec<f64>,
}

impl Scoring {
    pub fn of(maze: &Maze) -> Scoring {
        let mut sizes = vec![0; maze.zones()];
        for piece in maze.pieces() {
            sizes[piece.zone] += 1;
        }
        Scoring {
            solution: Solution::of(maze),
            counts: zone_counts(maze),
            shares: sizes
                .iter()
                .map(|&size| size as f64 / maze.pieces().len() as f64)
                .collect(),
        }
    }

    pub fn counts(&self) -> &[Vec<usize>] {
        &self.counts
    }

    pub fn step(&self, maze: &Maze, step: &Step) -> Vec<f64> {
        let fair_share = 1.0 / self.shares.len() as f64;
        let mut terms = step_terms(maze, &self.solution, &self.counts, step);
        terms.push(fair_share - self.shares[step.zone]);
        terms
    }
}

pub const STEP_TERMS: [&str; 7] = [
    "near solution",
    "short routes",
    "crossings",
    "crossings in a row",
    "empty cells",
    "mixing",
    "zone size",
];

pub const STEP_PRIORITIES: [f64; 7] = [2.0, 35.0, 1.0, 1.0, 1.0, 9.0, 51.0];

pub const ZONE_START_TERMS: [&str; 4] = [
    "join route",
    "circuit cells",
    "join distance",
    "finish near start",
];

pub const ZONE_START_PRIORITIES: [f64; 4] = [26.0, 38.0, 13.0, 95.0];

pub fn zone_start_terms(maze: &Maze, entry: PieceId, step: &Step, last: bool) -> Vec<f64> {
    let route = maze.routes_from(&[entry]).distances[step.from];
    let crow = maze
        .piece(entry)
        .cell
        .squared_distance(maze.piece(step.from).cell);
    let start_to_loop = step
        .cells
        .iter()
        .map(|c| c.cell.squared_distance(maze.start))
        .min()
        .unwrap_or(usize::MAX);
    let finish_away = if last {
        (start_to_loop as f64).sqrt()
    } else {
        0.0
    };
    vec![
        route as f64,
        step.cells.len() as f64,
        (crow as f64).sqrt(),
        -finish_away,
    ]
}

struct Solution {
    from_start: Routes,
    from_finish: Routes,
    length: usize,
    near_fraction: f64,
}

impl Solution {
    fn of(maze: &Maze) -> Solution {
        let from_start = maze.routes_from(maze.pieces_in(maze.start));
        let from_finish = maze.routes_from(maze.pieces_in(maze.finish));
        let route = maze.solution();
        let near_fraction = near_fraction(cells_near(
            maze.grid,
            route.iter().map(|&id| maze.piece(id).cell),
        ));
        Solution {
            from_start,
            from_finish,
            length: route.len(),
            near_fraction,
        }
    }

    fn near_fraction_after(&self, maze: &Maze, step: &Step) -> f64 {
        let through = |into: PieceId, out_of: PieceId| {
            self.from_start.distances[into]
                .saturating_add(step.cells.len() + 2)
                .saturating_add(self.from_finish.distances[out_of])
        };
        let forwards = through(step.from, step.to);
        let backwards = through(step.to, step.from);
        if forwards.min(backwards) >= self.length {
            return self.near_fraction;
        }
        let (into, out_of, cells): (PieceId, PieceId, Vec<Cell>) = if forwards <= backwards {
            (
                step.from,
                step.to,
                step.cells.iter().map(|c| c.cell).collect(),
            )
        } else {
            (
                step.to,
                step.from,
                step.cells.iter().rev().map(|c| c.cell).collect(),
            )
        };
        let mut finishing = self.from_finish.route_to(out_of);
        finishing.reverse();
        let route = self
            .from_start
            .route_to(into)
            .into_iter()
            .map(|id| maze.piece(id).cell)
            .chain(cells)
            .chain(finishing.into_iter().map(|id| maze.piece(id).cell));
        near_fraction(cells_near(maze.grid, route))
    }
}

fn near_fraction(near: Vec<bool>) -> f64 {
    near.iter().filter(|&&is_near| is_near).count() as f64 / near.len() as f64
}

fn step_terms(maze: &Maze, solution: &Solution, counts: &[Vec<usize>], step: &Step) -> Vec<f64> {
    let near_fraction = solution.near_fraction_after(maze, step);
    let is_crossing = |c: &StepCell| maze.content(c.cell) == Content::Path;
    let crossings = step.cells.iter().filter(|c| is_crossing(c)).count();
    let crossings_in_a_row = step
        .cells
        .windows(2)
        .filter(|pair| is_crossing(&pair[0]) && is_crossing(&pair[1]))
        .count();
    let empty_cells = step
        .cells
        .iter()
        .filter(|c| maze.content(c.cell) == Content::Empty)
        .count();
    vec![
        near_fraction,
        -short_route_shortfall(maze, step),
        crossings as f64,
        crossings_in_a_row as f64,
        empty_cells as f64,
        mixing(maze, counts, step),
    ]
}

pub fn foreign_share(counts: &[Vec<usize>], at: usize, zone: usize) -> f64 {
    let here = &counts[at];
    let all: usize = here.iter().sum();
    let own = here.get(zone).copied().unwrap_or(0);
    if all == 0 {
        0.0
    } else {
        1.0 - own as f64 / all as f64
    }
}

fn mixing(maze: &Maze, counts: &[Vec<usize>], step: &Step) -> f64 {
    step.cells
        .iter()
        .map(|c| foreign_share(counts, maze.grid.index(c.cell), step.zone))
        .sum()
}

fn short_route_shortfall(maze: &Maze, step: &Step) -> f64 {
    let from_start = maze.routes_shorter_than(&[step.from], LONG_ROUTE).distances;
    let from_end = maze.routes_shorter_than(&[step.to], LONG_ROUTE).distances;
    let length = step.cells.len();
    let mut shortfall = 0;
    for (i, c) in step.cells.iter().enumerate() {
        let beside =
            std::iter::once(c.cell).chain(maze.grid.neighbours(c.cell).map(|(_, cell)| cell));
        for cell in beside {
            for &other in maze.pieces_in(cell) {
                let joined =
                    (i == 0 && other == step.from) || (i + 1 == length && other == step.to);
                if joined {
                    continue;
                }
                let route = (i + 1)
                    .saturating_add(from_start[other])
                    .min((length - i).saturating_add(from_end[other]));
                shortfall += LONG_ROUTE.saturating_sub(route);
            }
            for (j, later) in step.cells.iter().enumerate().skip(i + 2) {
                if later.cell == cell {
                    shortfall += LONG_ROUTE.saturating_sub(j - i);
                }
            }
        }
    }
    shortfall as f64 / length as f64
}
