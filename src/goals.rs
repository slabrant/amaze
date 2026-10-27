use crate::grid::{Cell, Grid};
use crate::maze::{Content, Maze};
use crate::solvers::{Solver, steps_to_finish};
use rand::prelude::*;

pub const NEAR: usize = 3;
pub const LONG_ROUTE: usize = 12;
pub const MIXING_REACH: usize = 2;
const SOLVER_RUNS: usize = 10;
const SOLVER_PATIENCE: usize = 50;

#[derive(Clone, Debug)]
pub struct Measures {
    pub long_way_round: f64,
    pub solution_coverage: f64,
    pub crossings: usize,
    pub crossings_in_a_row: usize,
    pub joins_apart: f64,
    pub filling: f64,
    pub finish_near_start: f64,
    pub zones_mixed: f64,
    pub solution_length: usize,
    pub away_from_border: f64,
    pub solver_effort: Vec<f64>,
}

pub fn measure(maze: &Maze) -> Measures {
    let solution = maze.solution();
    let cells: Vec<Cell> = maze.grid.cells().collect();
    let maze_cells: Vec<Cell> = cells
        .iter()
        .copied()
        .filter(|&cell| maze.content(cell) != Content::Empty)
        .collect();
    let near = cells_near(maze.grid, solution.iter().map(|&id| maze.piece(id).cell));
    let joins = zone_joins(maze);
    Measures {
        long_way_round: 1.0 - short_routes_between_neighbours(maze),
        solution_coverage: maze_cells
            .iter()
            .filter(|&&cell| near[maze.grid.index(cell)])
            .count() as f64
            / maze_cells.len() as f64,
        crossings: cells
            .iter()
            .filter(|&&cell| maze.content(cell) == Content::Crossing)
            .count(),
        crossings_in_a_row: crossings_in_a_row(maze),
        joins_apart: joins
            .iter()
            .enumerate()
            .flat_map(|(i, &a)| joins[i + 1..].iter().map(move |&b| a.squared_distance(b)))
            .min()
            .map_or(0.0, |squared| (squared as f64).sqrt()),
        filling: maze_cells.len() as f64 / cells.len() as f64,
        finish_near_start: {
            let (width, height) = (maze.grid.width as f64, maze.grid.height as f64);
            let diagonal = (width * width + height * height).sqrt();
            1.0 - (maze.start.squared_distance(maze.finish) as f64).sqrt() / diagonal
        },
        zones_mixed: {
            let counts = zone_counts(maze);
            (0..maze.zones())
                .map(|zone| {
                    let own: Vec<usize> = maze
                        .pieces()
                        .iter()
                        .filter(|piece| piece.zone == zone)
                        .map(|piece| maze.grid.index(piece.cell))
                        .collect();
                    let foreign: f64 = own
                        .iter()
                        .map(|&at| {
                            let all: usize = counts[at].iter().sum();
                            1.0 - counts[at][zone] as f64 / all as f64
                        })
                        .sum();
                    foreign / own.len().max(1) as f64
                })
                .fold(1.0, f64::min)
        },
        solution_length: solution.len(),
        away_from_border: 1.0
            - solution
                .iter()
                .filter(|&&id| {
                    let cell = maze.piece(id).cell;
                    cell.x <= 1
                        || cell.y <= 1
                        || cell.x + 2 >= maze.grid.width
                        || cell.y + 2 >= maze.grid.height
                })
                .count() as f64
                / solution.len().max(1) as f64,
        solver_effort: Solver::ALL
            .iter()
            .map(|&solver| {
                let mut rng = StdRng::seed_from_u64(0);
                let most = SOLVER_PATIENCE * maze.pieces().len();
                let total: usize = (0..SOLVER_RUNS)
                    .map(|_| steps_to_finish(maze, solver, &mut rng, most))
                    .sum();
                total as f64 / SOLVER_RUNS as f64 / maze.pieces().len() as f64
            })
            .collect(),
    }
}

pub fn cells_near(grid: Grid, route: impl IntoIterator<Item = Cell>) -> Vec<bool> {
    let mut near = vec![false; grid.width * grid.height];
    for on in route {
        for cell in grid.around(on, NEAR) {
            if cell.squared_distance(on) <= NEAR * NEAR {
                near[grid.index(cell)] = true;
            }
        }
    }
    near
}

pub fn zone_counts(maze: &Maze) -> Vec<Vec<usize>> {
    let grid = maze.grid;
    let mut counts = vec![vec![0; maze.zones()]; grid.width * grid.height];
    for piece in maze.pieces() {
        for cell in grid.around(piece.cell, MIXING_REACH) {
            counts[grid.index(cell)][piece.zone] += 1;
        }
    }
    counts
}

pub fn zone_joins(maze: &Maze) -> Vec<Cell> {
    maze.pieces()
        .iter()
        .filter(|piece| {
            piece
                .links
                .iter()
                .any(|&(_, other)| maze.piece(other).zone == piece.zone + 1)
        })
        .map(|piece| piece.cell)
        .collect()
}

fn short_routes_between_neighbours(maze: &Maze) -> f64 {
    let mut pairs = 0;
    let mut short = 0;
    for (id, piece) in maze.pieces().iter().enumerate() {
        let distances = maze.routes_from(&[id]).distances;
        let beside = std::iter::once(piece.cell)
            .chain(maze.grid.neighbours(piece.cell).map(|(_, cell)| cell));
        for cell in beside {
            for &other in maze.pieces_in(cell) {
                if other <= id || piece.links.iter().any(|&(_, linked)| linked == other) {
                    continue;
                }
                pairs += 1;
                if distances[other] < LONG_ROUTE {
                    short += 1;
                }
            }
        }
    }
    short as f64 / pairs.max(1) as f64
}

fn crossings_in_a_row(maze: &Maze) -> usize {
    maze.pieces()
        .iter()
        .enumerate()
        .filter(|(_, piece)| maze.content(piece.cell) == Content::Crossing)
        .map(|(id, piece)| {
            piece
                .links
                .iter()
                .filter(|&&(_, next)| {
                    next > id && maze.content(maze.piece(next).cell) == Content::Crossing
                })
                .count()
        })
        .sum()
}
