use crate::goals::zone_counts;
use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Content, Maze, PieceId, Step, StepCell};
use crate::score::{STEP_TERMS, Scoring, ZONE_START_TERMS, foreign_share, zone_start_terms};
use rand::prelude::*;
use std::collections::HashSet;

const ATTEMPTS: usize = 1000;
const ZONE_START_ATTEMPTS: usize = 20_000;
const CANDIDATES: usize = 40;
const ALWAYS_SOME: f64 = 0.2;
const FIRST_LOOP_SHARE: f64 = 0.35;
const BUMP_ATTEMPTS: usize = 100_000;
const ZONE_START_CANDIDATES: usize = 200;
const NEAR_ENDS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failed;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    ZoneStart,
    Step,
}

pub struct Choice {
    pub kind: Kind,
    pub terms: &'static [&'static str],
    pub candidates: Vec<(Step, Vec<f64>)>,
    pub chosen: usize,
}

impl Choice {
    fn of(
        kind: Kind,
        terms: &'static [&'static str],
        candidates: Vec<(Step, Vec<f64>)>,
    ) -> Option<Choice> {
        let chosen = candidates
            .iter()
            .enumerate()
            .map(|(i, (_, terms))| (total(terms), i))
            .max_by(|a, b| a.0.total_cmp(&b.0))?
            .1;
        Some(Choice {
            kind,
            terms,
            candidates,
            chosen,
        })
    }

    pub fn step(&self) -> &Step {
        &self.candidates[self.chosen].0
    }
}

pub fn total(terms: &[f64]) -> f64 {
    terms.iter().sum()
}

pub fn generate(grid: Grid, seed: u64, zones: usize) -> Result<Maze, Failed> {
    generate_watched(grid, seed, zones, |_, _| {})
}

pub fn generate_watched(
    grid: Grid,
    seed: u64,
    zones: usize,
    mut watch: impl FnMut(&Maze, Option<&Choice>),
) -> Result<Maze, Failed> {
    if grid.width < 4 || grid.height < 4 || zones == 0 {
        return Err(Failed);
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let mut maze = Maze::with_loop(grid, &first_loop(grid, &mut rng)).map_err(|_| Failed)?;
    maze.start = *path_cells_in_zone(&maze, 0)
        .choose(&mut rng)
        .ok_or(Failed)?;
    watch(&maze, None);
    let mut entry = maze.pieces_in(maze.start)[0];
    for zone in 1..zones {
        let counts = zone_counts(&maze);
        let scored = candidates(&maze, &mut rng, &counts, zone, ZONE_START_ATTEMPTS)
            .map(|step| {
                let terms = zone_start_terms(&maze, entry, &step, zone + 1 == zones);
                (step, terms)
            })
            .collect();
        let choice = Choice::of(Kind::ZoneStart, &ZONE_START_TERMS, scored).ok_or(Failed)?;
        entry = choice.step().from;
        maze.make_step(choice.step()).map_err(|_| Failed)?;
        if zone + 1 == zones {
            place_finish(&mut maze, zones)?;
        }
        watch(&maze, Some(&choice));
    }
    if zones == 1 {
        place_finish(&mut maze, zones)?;
    }
    while let Some(choice) = best_step(&maze, &mut rng) {
        maze.make_step(choice.step()).map_err(|_| Failed)?;
        watch(&maze, Some(&choice));
    }
    Ok(maze)
}

fn candidates<'a>(
    maze: &'a Maze,
    rng: &'a mut StdRng,
    counts: &'a [Vec<usize>],
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
        .filter_map(move |_| find_step(maze, rng, counts, zone, &sides))
        .take(wanted)
}

fn best_step(maze: &Maze, rng: &mut StdRng) -> Option<Choice> {
    let scoring = Scoring::of(maze);
    let steps: Vec<Step> = (0..maze.zones())
        .flat_map(|zone| {
            candidates(maze, rng, scoring.counts(), zone, ATTEMPTS).collect::<Vec<_>>()
        })
        .collect();
    let scored = steps
        .into_iter()
        .map(|step| {
            let terms = scoring.step(maze, &step);
            (step, terms)
        })
        .collect();
    Choice::of(Kind::Step, &STEP_TERMS, scored)
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

fn first_loop(grid: Grid, rng: &mut StdRng) -> Vec<Cell> {
    let width = grid.width - 2;
    let height = grid.height - 2;
    let left = rng.random_range(1..=grid.width - 1 - width);
    let top = rng.random_range(1..=grid.height - 1 - height);
    let right = left + width - 1;
    let bottom = top + height - 1;
    let mut cells: Vec<Cell> = (left..right)
        .map(|x| Cell { x, y: top })
        .chain((top..bottom).map(|y| Cell { x: right, y }))
        .chain((left + 1..=right).rev().map(|x| Cell { x, y: bottom }))
        .chain((top + 1..=bottom).rev().map(|y| Cell { x: left, y }))
        .collect();
    let mut taken: HashSet<Cell> = cells.iter().copied().collect();
    let inside = |cell: Cell| {
        (1..grid.width - 1).contains(&cell.x) && (1..grid.height - 1).contains(&cell.y)
    };
    let wanted = (FIRST_LOOP_SHARE * (grid.width * grid.height) as f64) as usize;
    for _ in 0..BUMP_ATTEMPTS {
        if cells.len() >= wanted {
            break;
        }
        let i = rng.random_range(0..cells.len());
        let (a, b) = (cells[i], cells[(i + 1) % cells.len()]);
        let Some(along) = Dir::between(a, b) else {
            continue;
        };
        let side = *[along.turned_left(), along.turned_right()]
            .choose(rng)
            .unwrap_or(&along);
        let bump = (grid.neighbour(a, side), grid.neighbour(b, side));
        if let (Some(a_out), Some(b_out)) = bump
            && inside(a_out)
            && inside(b_out)
            && !taken.contains(&a_out)
            && !taken.contains(&b_out)
        {
            cells.splice(i + 1..i + 1, [a_out, b_out]);
            taken.extend([a_out, b_out]);
        }
    }
    cells
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

fn ways_on<'a>(maze: &Maze, here: Cell, heading: &'a Dir) -> &'a [Dir] {
    if maze.content(here) == Content::Path {
        std::slice::from_ref(heading)
    } else {
        &Dir::ALL
    }
}

fn find_step(
    maze: &Maze,
    rng: &mut StdRng,
    counts: &[Vec<usize>],
    zone: usize,
    sides: &[(PieceId, Dir)],
) -> Option<Step> {
    let grid = maze.grid;
    let &(from, out) = sides
        .choose_weighted(rng, |&(id, _)| {
            ALWAYS_SOME + foreign_share(counts, grid.index(maze.piece(id).cell), zone)
        })
        .ok()?;
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
        let ways = ways_on(maze, here, &heading);
        for &dir in ways {
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
        let mut onward = [(heading, here); 4];
        let mut found = 0;
        for &dir in ways {
            if let Some(cell) = grid.neighbour(here, dir)
                && !visited[grid.index(cell)]
                && can_enter(maze, cell)
            {
                onward[found] = (dir, cell);
                found += 1;
            }
        }
        let Ok(&(dir, next)) = onward[..found].choose_weighted(rng, |&(_, cell)| {
            ALWAYS_SOME + foreign_share(counts, grid.index(cell), zone)
        }) else {
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
    use crate::svg::{render, render_solution, render_zones};

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
            "short {:.2} near {:.2} crossings {:.0} in_a_row {:.0} joins {:.1} empty {:.1} ends {:.1} solution {:.0} least_mixed {:.2} border {:.2} solvers {:.2} {:.2} {:.2} {:.2} each {:?}",
            average(&|m| m.short_routes_between_neighbours),
            average(&|m| m.maze_cells_near_solution),
            average(&|m| m.crossings as f64),
            average(&|m| m.crossings_in_a_row as f64),
            average(&|m| m.nearest_zone_joins.unwrap_or(0.0)),
            average(&|m| m.empty_cells as f64),
            average(&|m| m.start_to_finish),
            average(&|m| m.solution_length as f64),
            average(&|m| m.least_mixed_zone),
            average(&|m| m.solution_on_border),
            average(&|m| m.solver_steps[0]),
            average(&|m| m.solver_steps[1]),
            average(&|m| m.solver_steps[2]),
            average(&|m| m.solver_steps[3]),
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
            std::fs::write(
                dir.join(format!("maze-{seed}-zones.svg")),
                render_zones(&maze),
            )
            .unwrap();
        }
    }
}
