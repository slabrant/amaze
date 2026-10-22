use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Content, Maze, PieceId, Step, StepCell};
use rand::prelude::*;

const ATTEMPTS: usize = 1000;
const CANDIDATES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failed;

pub fn generate(grid: Grid, seed: u64, zones: usize, steps: usize) -> Result<Maze, Failed> {
    if grid.width < 2 || grid.height < 2 || zones == 0 {
        return Err(Failed);
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let mut maze = Maze::with_loop(grid, &first_loop(grid, &mut rng)).map_err(|_| Failed)?;
    for zone in 1..zones {
        make_step(&mut maze, &mut rng, |_| zone)?;
    }
    for _ in 0..steps {
        make_step(&mut maze, &mut rng, |rng| rng.random_range(0..zones))?;
    }
    maze.start = *path_cells(&maze, 0).choose(&mut rng).ok_or(Failed)?;
    maze.finish = *path_cells(&maze, zones - 1)
        .choose(&mut rng)
        .ok_or(Failed)?;
    Ok(maze)
}

fn make_step(
    maze: &mut Maze,
    rng: &mut StdRng,
    pick_zone: impl Fn(&mut StdRng) -> usize,
) -> Result<(), Failed> {
    let step = (0..ATTEMPTS)
        .filter_map(|_| {
            let zone = pick_zone(rng);
            find_step(maze, rng, zone)
        })
        .take(CANDIDATES)
        .max_by_key(|step| empty_cells_filled(maze, step))
        .ok_or(Failed)?;
    maze.make_step(&step).map_err(|_| Failed)
}

fn empty_cells_filled(maze: &Maze, step: &Step) -> usize {
    step.cells
        .iter()
        .filter(|c| maze.content(c.cell) == Content::Empty)
        .count()
}

fn path_cells(maze: &Maze, zone: usize) -> Vec<Cell> {
    maze.grid
        .cells()
        .filter(|&cell| {
            maze.content(cell) == Content::Path && maze.piece(maze.pieces_in(cell)[0]).zone == zone
        })
        .collect()
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

fn open_sides(maze: &Maze, zone: usize) -> Vec<(PieceId, Dir)> {
    let starts_zone = zone == maze.zones();
    let from_zone = if starts_zone { zone - 1 } else { zone };
    maze.pieces()
        .iter()
        .enumerate()
        .filter(|(_, piece)| piece.zone == from_zone)
        .filter(|(_, piece)| match maze.content(piece.cell) {
            Content::Path => true,
            Content::Junction => !starts_zone,
            Content::Empty | Content::Crossing => false,
        })
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

fn find_step(maze: &Maze, rng: &mut StdRng, zone: usize) -> Option<Step> {
    let grid = maze.grid;
    let &(from, out) = open_sides(maze, zone).choose(rng)?;
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
                    zone,
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
            let maze = generate(GRID, seed, 3, 20).unwrap();
            assert_eq!(broken(&maze), vec![], "seed {seed}");
        }
    }

    #[test]
    fn generated_mazes_have_crossings_and_junctions() {
        let maze = generate(GRID, 1, 3, 20).unwrap();
        assert_eq!(maze.zones(), 3);
        let contents: Vec<Content> = GRID.cells().map(|cell| maze.content(cell)).collect();
        assert!(contents.contains(&Content::Crossing));
        assert!(contents.contains(&Content::Junction));
    }

    #[test]
    fn same_seed_same_maze() {
        let a = generate(GRID, 7, 3, 20).unwrap();
        let b = generate(GRID, 7, 3, 20).unwrap();
        assert_eq!(render(&a), render(&b));
    }

    #[test]
    fn draws_mazes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/svg");
        std::fs::create_dir_all(&dir).unwrap();
        for seed in 0..4 {
            let maze = generate(GRID, seed, 3, 20).unwrap();
            std::fs::write(dir.join(format!("maze-{seed}.svg")), render(&maze)).unwrap();
        }
    }
}
