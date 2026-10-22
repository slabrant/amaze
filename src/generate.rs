use crate::goals::{LONG_ROUTE, near_solution};
use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Content, Maze, PieceId, Routes, Step, StepCell};
use rand::prelude::*;

const ATTEMPTS: usize = 1000;
const ZONE_START_ATTEMPTS: usize = 20_000;
const CANDIDATES: usize = 40;
const ZONE_START_CANDIDATES: usize = 200;
const NEAR_ENDS: usize = 8;

const SHORT_ROUTE_WEIGHT: f64 = 5.0;
const NEAR_SOLUTION_WEIGHT: f64 = 100.0;
const CROSSING_WEIGHT: f64 = 1.0;
const CROSSING_IN_A_ROW_WEIGHT: f64 = 1.0;
const EMPTY_CELL_WEIGHT: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failed;

pub fn generate(grid: Grid, seed: u64, zones: usize) -> Result<Maze, Failed> {
    if grid.width < 4 || grid.height < 4 || zones == 0 {
        return Err(Failed);
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let mut maze = Maze::with_loop(grid, &first_loop(grid, &mut rng)).map_err(|_| Failed)?;
    maze.start = *path_cells_in_zone(&maze, 0)
        .choose(&mut rng)
        .ok_or(Failed)?;
    let mut entry = maze.pieces_in(maze.start)[0];
    for zone in 1..zones {
        let step = candidates(&maze, &mut rng, zone, ZONE_START_ATTEMPTS)
            .max_by_key(|step| zone_start_score(&maze, entry, step))
            .ok_or(Failed)?;
        entry = step.from;
        maze.make_step(&step).map_err(|_| Failed)?;
    }
    place_finish(&mut maze, zones)?;
    let mut turns: Vec<usize> = (0..zones).collect();
    loop {
        turns.shuffle(&mut rng);
        turns.sort_by_key(|&zone| {
            maze.pieces()
                .iter()
                .filter(|piece| piece.zone == zone)
                .count()
        });
        let mut stepped = false;
        for &zone in &turns {
            if let Some(step) = best_step(&maze, &mut rng, zone) {
                maze.make_step(&step).map_err(|_| Failed)?;
                stepped = true;
            }
        }
        if !stepped {
            break;
        }
    }
    Ok(maze)
}

fn candidates<'a>(
    maze: &'a Maze,
    rng: &'a mut StdRng,
    zone: usize,
    attempts: usize,
) -> impl Iterator<Item = Step> + 'a {
    let wanted = if zone == maze.zones() {
        ZONE_START_CANDIDATES
    } else {
        CANDIDATES
    };
    let sides = open_sides(maze, zone);
    (0..attempts)
        .filter_map(move |_| find_step(maze, rng, zone, &sides))
        .take(wanted)
}

fn zone_start_score(maze: &Maze, entry: PieceId, step: &Step) -> usize {
    let route = maze.routes_from(&[entry]).distances[step.from];
    let crow = maze
        .piece(entry)
        .cell
        .squared_distance(maze.piece(step.from).cell);
    2 * route + (crow as f64).sqrt() as usize + step.cells.len()
}

fn place_finish(maze: &mut Maze, zones: usize) -> Result<(), Failed> {
    let finishes: Vec<Cell> = path_cells_in_zone(maze, zones - 1)
        .into_iter()
        .filter(|&finish| finish != maze.start)
        .collect();
    let nearest = finishes
        .iter()
        .map(|finish| finish.squared_distance(maze.start))
        .min()
        .ok_or(Failed)?;
    let mut longest = 0;
    let mut best = maze.finish;
    for finish in finishes {
        if finish.squared_distance(maze.start) > nearest.max(NEAR_ENDS) {
            continue;
        }
        maze.finish = finish;
        let length = maze.solution().len();
        if length > longest {
            longest = length;
            best = finish;
        }
    }
    maze.finish = best;
    Ok(())
}

fn path_cells_in_zone(maze: &Maze, zone: usize) -> Vec<Cell> {
    maze.grid
        .cells()
        .filter(|&cell| {
            maze.content(cell) == Content::Path && maze.piece(maze.pieces_in(cell)[0]).zone == zone
        })
        .collect()
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
        let near_fraction = near_fraction(near_solution(
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
        near_fraction(near_solution(maze.grid, route))
    }
}

fn near_fraction(near: Vec<bool>) -> f64 {
    near.iter().filter(|&&is_near| is_near).count() as f64 / near.len() as f64
}

fn best_step(maze: &Maze, rng: &mut StdRng, zone: usize) -> Option<Step> {
    let solution = Solution::of(maze);
    candidates(maze, rng, zone, ATTEMPTS)
        .map(|step| (step_score(maze, &solution, &step), step))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, step)| step)
}

fn step_score(maze: &Maze, solution: &Solution, step: &Step) -> f64 {
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
    NEAR_SOLUTION_WEIGHT * near_fraction - SHORT_ROUTE_WEIGHT * short_route_shortfall(maze, step)
        + CROSSING_WEIGHT * crossings as f64
        + CROSSING_IN_A_ROW_WEIGHT * crossings_in_a_row as f64
        + EMPTY_CELL_WEIGHT * empty_cells as f64
}

fn short_route_shortfall(maze: &Maze, step: &Step) -> f64 {
    let from_start = maze.routes_from(&[step.from]).distances;
    let from_end = maze.routes_from(&[step.to]).distances;
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

fn first_loop(grid: Grid, rng: &mut StdRng) -> Vec<Cell> {
    let width = rng.random_range((grid.width - 2).div_ceil(2).max(2)..=grid.width - 2);
    let height = rng.random_range((grid.height - 2).div_ceil(2).max(2)..=grid.height - 2);
    let left = rng.random_range(1..=grid.width - 1 - width);
    let top = rng.random_range(1..=grid.height - 1 - height);
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
    if cell == maze.start || cell == maze.finish {
        return false;
    }
    match maze.pieces_in(cell) {
        [] => true,
        [crossed] => maze.piece(*crossed).is_straight(),
        _ => false,
    }
}

fn ways_on(maze: &Maze, here: Cell, heading: Dir) -> Vec<Dir> {
    if maze.content(here) == Content::Path {
        vec![heading]
    } else {
        Dir::ALL.to_vec()
    }
}

fn find_step(maze: &Maze, rng: &mut StdRng, zone: usize, sides: &[(PieceId, Dir)]) -> Option<Step> {
    let grid = maze.grid;
    let &(from, out) = sides.choose(rng)?;
    let from_cell = maze.piece(from).cell;
    let first = grid
        .neighbour(from_cell, out)
        .filter(|&cell| can_enter(maze, cell))?;
    let mut visited = vec![false; grid.width * grid.height];
    visited[grid.index(from_cell)] = true;
    visited[grid.index(first)] = true;
    let mut cells = vec![StepCell {
        cell: first,
        on_top: rng.random_bool(0.5),
    }];
    let mut heading = out;
    let limit = if zone == maze.zones() {
        grid.width * grid.height
    } else {
        rng.random_range(1..=grid.width * grid.height)
    };
    let mut ends: Vec<(usize, PieceId)> = Vec::new();
    loop {
        let here = cells[cells.len() - 1].cell;
        let ways = ways_on(maze, here, heading);
        for &dir in &ways {
            if let Some(next) = grid.neighbour(here, dir)
                && let [to] = *maze.pieces_in(next)
                && (to == from || maze.piece(to).zone == zone)
            {
                ends.push((cells.len(), to));
            }
        }
        if cells.len() >= limit {
            break;
        }
        let onward: Vec<(Dir, Cell)> = ways
            .into_iter()
            .filter_map(|dir| grid.neighbour(here, dir).map(|cell| (dir, cell)))
            .filter(|&(_, cell)| !visited[grid.index(cell)] && can_enter(maze, cell))
            .collect();
        let Some(&(dir, next)) = onward.choose(rng) else {
            break;
        };
        heading = dir;
        visited[grid.index(next)] = true;
        cells.push(StepCell {
            cell: next,
            on_top: rng.random_bool(0.5),
        });
    }
    ends.into_iter().rev().find_map(|(length, to)| {
        let step = Step {
            from,
            cells: cells[..length].to_vec(),
            to,
            zone,
        };
        maze.check_step(&step).is_ok().then_some(step)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::broken;
    use crate::svg::{render, render_solution};

    const GRID: Grid = Grid {
        width: 20,
        height: 20,
    };

    #[test]
    fn generated_mazes_keep_the_rules() {
        for seed in 0..20 {
            let maze = generate(GRID, seed, 3).unwrap();
            assert_eq!(broken(&maze), vec![], "seed {seed}");
        }
    }

    #[test]
    fn generated_mazes_nearly_fill_the_grid() {
        for seed in 0..5 {
            let maze = generate(GRID, seed, 3).unwrap();
            let empty = GRID
                .cells()
                .filter(|&cell| maze.content(cell) == Content::Empty)
                .count();
            println!("seed {seed} empty {empty}");
            assert!(
                empty * 50 <= GRID.width * GRID.height,
                "seed {seed}: {empty} empty"
            );
        }
    }

    #[test]
    fn generated_mazes_have_crossings_and_junctions() {
        let maze = generate(GRID, 1, 3).unwrap();
        assert_eq!(maze.zones(), 3);
        let contents: Vec<Content> = GRID.cells().map(|cell| maze.content(cell)).collect();
        assert!(contents.contains(&Content::Crossing));
        assert!(contents.contains(&Content::Junction));
    }

    #[test]
    fn solution_goes_from_start_to_finish_through_every_zone() {
        for seed in 0..5 {
            let maze = generate(GRID, seed, 3).unwrap();
            let solution = maze.solution();
            let first = maze.piece(solution[0]);
            let last = maze.piece(solution[solution.len() - 1]);
            assert_eq!((first.cell, last.cell), (maze.start, maze.finish));
            let zones: Vec<usize> = solution.iter().map(|&id| maze.piece(id).zone).collect();
            assert!(zones.windows(2).all(|pair| pair[1] <= pair[0] + 1));
            assert_eq!(zones.iter().max(), Some(&2));
        }
    }

    #[test]
    fn prints_goal_measures() {
        for seed in 0..5 {
            let maze = generate(GRID, seed, 3).unwrap();
            println!("seed {seed} {:?}", crate::goals::measure(&maze));
        }
    }

    #[test]
    #[ignore]
    fn reports_goals_over_many_seeds() {
        let seeds = 50;
        let started = std::time::Instant::now();
        let all: Vec<crate::goals::Measures> = (0..seeds)
            .map(|seed| {
                let maze = generate(GRID, seed, 3).unwrap();
                assert_eq!(broken(&maze), vec![], "seed {seed}");
                crate::goals::measure(&maze)
            })
            .collect();
        let average = |f: &dyn Fn(&crate::goals::Measures) -> f64| {
            all.iter().map(f).sum::<f64>() / all.len() as f64
        };
        println!(
            "short {:.2} near {:.2} crossings {:.0} in_a_row {:.0} joins {:.1} empty {:.1} ends {:.1} solution {:.0} each {:?}",
            average(&|m| m.short_routes_between_neighbours),
            average(&|m| m.maze_cells_near_solution),
            average(&|m| m.crossings as f64),
            average(&|m| m.crossings_in_a_row as f64),
            average(&|m| m.nearest_zone_joins.unwrap_or(0.0)),
            average(&|m| m.empty_cells as f64),
            average(&|m| m.start_to_finish),
            average(&|m| m.solution_length as f64),
            started.elapsed() / seeds as u32,
        );
    }

    #[test]
    fn same_seed_same_maze() {
        let a = generate(GRID, 7, 3).unwrap();
        let b = generate(GRID, 7, 3).unwrap();
        assert_eq!(render(&a), render(&b));
    }

    #[test]
    fn draws_mazes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/svg");
        std::fs::create_dir_all(&dir).unwrap();
        for seed in 0..4 {
            let maze = generate(GRID, seed, 3).unwrap();
            std::fs::write(dir.join(format!("maze-{seed}.svg")), render(&maze)).unwrap();
            std::fs::write(
                dir.join(format!("maze-{seed}-solution.svg")),
                render_solution(&maze),
            )
            .unwrap();
        }
    }
}
