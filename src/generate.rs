use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Content, Maze, PieceId, Step, StepCell};
use rand::prelude::*;

const ATTEMPTS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failed;

pub fn generate(grid: Grid, seed: u64, steps: usize) -> Result<Maze, Failed> {
    if grid.width < 2 || grid.height < 2 {
        return Err(Failed);
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let mut maze = Maze::with_loop(grid, &first_loop(grid, &mut rng)).map_err(|_| Failed)?;
    for _ in 0..steps {
        let step = (0..ATTEMPTS)
            .find_map(|_| find_step(&maze, &mut rng))
            .ok_or(Failed)?;
        maze.make_step(&step).map_err(|_| Failed)?;
    }
    let path_cells: Vec<Cell> = grid
        .cells()
        .filter(|&cell| maze.content(cell) == Content::Path)
        .collect();
    let [start, finish] = path_cells.sample_array(&mut rng).ok_or(Failed)?;
    maze.start = start;
    maze.finish = finish;
    Ok(maze)
}

fn first_loop(grid: Grid, rng: &mut StdRng) -> Vec<Cell> {
    let width = rng.random_range(2..=grid.width);
    let height = rng.random_range(2..=grid.height);
    let left = rng.random_range(0..=grid.width - width);
    let top = rng.random_range(0..=grid.height - height);
    let right = left + width - 1;
    let bottom = top + height - 1;
    (left..right)
        .map(|x| Cell { x, y: top })
        .chain((top..bottom).map(|y| Cell { x: right, y }))
        .chain((left + 1..=right).rev().map(|x| Cell { x, y: bottom }))
        .chain((top + 1..=bottom).rev().map(|y| Cell { x: left, y }))
        .collect()
}

fn open_sides(maze: &Maze) -> Vec<(PieceId, Dir)> {
    maze.pieces()
        .iter()
        .enumerate()
        .filter(|(_, piece)| maze.pieces_in(piece.cell).len() == 1)
        .flat_map(|(id, piece)| {
            maze.grid
                .neighbours(piece.cell)
                .filter(|&(dir, _)| !piece.has_side(dir))
                .map(move |(dir, _)| (id, dir))
        })
        .collect()
}

fn can_enter(maze: &Maze, cell: Cell) -> bool {
    matches!(maze.content(cell), Content::Empty | Content::Path)
}

fn find_step(maze: &Maze, rng: &mut StdRng) -> Option<Step> {
    let grid = maze.grid;
    let &(from, out) = open_sides(maze).choose(rng)?;
    let from_cell = maze.piece(from).cell;
    let first = grid
        .neighbour(from_cell, out)
        .filter(|&cell| can_enter(maze, cell))?;
    let mut cells = vec![StepCell {
        cell: first,
        on_top: rng.random_bool(0.5),
    }];
    let limit = rng.random_range(1..=grid.width * grid.height);
    let mut longest = None;
    loop {
        let here = cells[cells.len() - 1].cell;
        for (_, next) in grid.neighbours(here) {
            for &to in maze.pieces_in(next) {
                let step = Step {
                    from,
                    cells: cells.clone(),
                    to,
                };
                if maze.check_step(&step).is_ok() {
                    longest = Some(step);
                }
            }
        }
        if cells.len() >= limit {
            break;
        }
        let onward: Vec<Cell> = grid
            .neighbours(here)
            .map(|(_, cell)| cell)
            .filter(|&cell| {
                cell != from_cell && !cells.iter().any(|c| c.cell == cell) && can_enter(maze, cell)
            })
            .collect();
        let Some(&next) = onward.choose(rng) else {
            break;
        };
        cells.push(StepCell {
            cell: next,
            on_top: rng.random_bool(0.5),
        });
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::broken;
    use crate::svg::render;

    const GRID: Grid = Grid {
        width: 20,
        height: 20,
    };

    #[test]
    fn generated_mazes_keep_the_rules() {
        for seed in 0..20 {
            let maze = generate(GRID, seed, 30).unwrap();
            assert_eq!(broken(&maze), vec![], "seed {seed}");
        }
    }

    #[test]
    fn generated_mazes_have_crossings_and_junctions() {
        let maze = generate(GRID, 1, 30).unwrap();
        let contents: Vec<Content> = GRID.cells().map(|cell| maze.content(cell)).collect();
        assert!(contents.contains(&Content::Crossing));
        assert!(contents.contains(&Content::Junction));
    }

    #[test]
    fn same_seed_same_maze() {
        let a = generate(GRID, 7, 30).unwrap();
        let b = generate(GRID, 7, 30).unwrap();
        assert_eq!(render(&a), render(&b));
    }

    #[test]
    fn draws_mazes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/svg");
        std::fs::create_dir_all(&dir).unwrap();
        for seed in 0..4 {
            let maze = generate(GRID, seed, 30).unwrap();
            std::fs::write(dir.join(format!("maze-{seed}.svg")), render(&maze)).unwrap();
        }
    }
}
