use crate::grid::{Cell, Grid};
use crate::maze::{Content, Maze};

pub const NEAR: usize = 3;
pub const LONG_ROUTE: usize = 12;
pub const MIXING_REACH: usize = 2;

#[derive(Clone, Debug)]
pub struct Measures {
    pub short_routes_between_neighbours: f64,
    pub maze_cells_near_solution: f64,
    pub crossings: usize,
    pub crossings_in_a_row: usize,
    pub nearest_zone_joins: Option<f64>,
    pub zones: usize,
    pub empty_cells: usize,
    pub start_to_finish: f64,
    pub least_mixed_zone: f64,
    pub solution_length: usize,
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
        short_routes_between_neighbours: short_routes_between_neighbours(maze),
        maze_cells_near_solution: maze_cells
            .iter()
            .filter(|&&cell| near[maze.grid.index(cell)])
            .count() as f64
            / maze_cells.len() as f64,
        crossings: cells
            .iter()
            .filter(|&&cell| maze.content(cell) == Content::Crossing)
            .count(),
        crossings_in_a_row: crossings_in_a_row(maze),
        nearest_zone_joins: joins
            .iter()
            .enumerate()
            .flat_map(|(i, &a)| joins[i + 1..].iter().map(move |&b| a.squared_distance(b)))
            .min()
            .map(|squared| (squared as f64).sqrt()),
        zones: maze.zones(),
        empty_cells: cells.len() - maze_cells.len(),
        start_to_finish: (maze.start.squared_distance(maze.finish) as f64).sqrt(),
        least_mixed_zone: {
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
    }
}

pub fn cells_near(grid: Grid, route: impl IntoIterator<Item = Cell>) -> Vec<bool> {
    let mut near = vec![false; grid.width * grid.height];
    for on in route {
        for y in on.y.saturating_sub(NEAR)..=(on.y + NEAR).min(grid.height - 1) {
            for x in on.x.saturating_sub(NEAR)..=(on.x + NEAR).min(grid.width - 1) {
                let cell = Cell { x, y };
                if cell.squared_distance(on) <= NEAR * NEAR {
                    near[grid.index(cell)] = true;
                }
            }
        }
    }
    near
}

pub fn zone_counts(maze: &Maze) -> Vec<Vec<usize>> {
    let grid = maze.grid;
    let mut counts = vec![vec![0; maze.zones()]; grid.width * grid.height];
    for piece in maze.pieces() {
        let at = piece.cell;
        for y in at.y.saturating_sub(MIXING_REACH)..=(at.y + MIXING_REACH).min(grid.height - 1) {
            for x in at.x.saturating_sub(MIXING_REACH)..=(at.x + MIXING_REACH).min(grid.width - 1) {
                counts[grid.index(Cell { x, y })][piece.zone] += 1;
            }
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
