use crate::goals::zone_counts;
use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Content, Maze, PieceId, Step, StepCell};
use crate::score::{
    STEP_PRIORITIES, STEP_TERMS, Scoring, ZONE_START_PRIORITIES, ZONE_START_TERMS, foreign_share,
    zone_start_terms,
};
use rand::prelude::*;
use std::collections::HashSet;

const ATTEMPTS: usize = 1000;
const ZONE_START_ATTEMPTS: usize = 20_000;
const CANDIDATES: usize = 40;
const ALWAYS_SOME: f64 = 0.2;
const FIRST_LOOP_SHARE: f64 = 0.35;
const BUMP_ATTEMPTS: usize = 100_000;
const BUMP_CHOICES: usize = 30;
const CROWD_REACH: usize = 2;
const ZONE_START_CANDIDATES: usize = 200;
const ZONE_START_REACH: usize = 16;
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
        priorities: &[f64],
        raw: Vec<(Step, Vec<f64>)>,
    ) -> Option<Choice> {
        let count = raw.len() as f64;
        let scale: Vec<(f64, f64)> = (0..terms.len())
            .map(|t| {
                let mean = raw.iter().map(|(_, terms)| terms[t]).sum::<f64>() / count;
                let squares: f64 = raw.iter().map(|(_, terms)| (terms[t] - mean).powi(2)).sum();
                (mean, (squares / count).sqrt())
            })
            .collect();
        let candidates: Vec<(Step, Vec<f64>)> = raw
            .into_iter()
            .map(|(step, terms)| {
                let normal = terms
                    .iter()
                    .zip(&scale)
                    .zip(priorities)
                    .map(|((&value, &(mean, spread)), &priority)| {
                        if spread > 0.0 {
                            priority * (value - mean) / spread
                        } else {
                            0.0
                        }
                    })
                    .collect();
                (step, normal)
            })
            .collect();
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

pub enum Event<'a> {
    Loop(&'a [Cell]),
    Chosen(&'a Maze, &'a Choice),
}

pub fn total(terms: &[f64]) -> f64 {
    terms.iter().sum()
}

pub fn generate(grid: Grid, seed: u64, zones: usize) -> Result<Maze, Failed> {
    generate_watched(grid, seed, zones, |_| {})
}

pub fn generate_watched(
    grid: Grid,
    seed: u64,
    zones: usize,
    mut watch: impl FnMut(Event),
) -> Result<Maze, Failed> {
    if grid.width < 4 || grid.height < 4 || zones == 0 {
        return Err(Failed);
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let cells = first_loop(grid, &mut rng, &mut |cells| watch(Event::Loop(cells)));
    let mut maze = Maze::with_loop(grid, &cells).map_err(|_| Failed)?;
    maze.start = *path_cells_in_zone(&maze, 0)
        .choose(&mut rng)
        .ok_or(Failed)?;
    let mut entry = maze.pieces_in(maze.start)[0];
    for zone in 1..zones {
        let counts = zone_counts(&maze);
        let scored = candidates(&maze, &mut rng, &counts, zone, ZONE_START_ATTEMPTS)
            .map(|step| {
                let terms = zone_start_terms(&maze, entry, &step, zone + 1 == zones);
                (step, terms)
            })
            .collect();
        let choice = Choice::of(
            Kind::ZoneStart,
            &ZONE_START_TERMS,
            &ZONE_START_PRIORITIES,
            scored,
        )
        .ok_or(Failed)?;
        entry = choice.step().from;
        maze.make_step(choice.step()).map_err(|_| Failed)?;
        if zone + 1 == zones {
            place_finish(&mut maze, zones)?;
        }
        watch(Event::Chosen(&maze, &choice));
    }
    if zones == 1 {
        place_finish(&mut maze, zones)?;
    }
    while let Some(choice) = best_step(&maze, &mut rng) {
        maze.make_step(choice.step()).map_err(|_| Failed)?;
        watch(Event::Chosen(&maze, &choice));
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
    let mut found = HashSet::new();
    (0..attempts)
        .filter_map(move |_| {
            if zone == maze.zones() {
                find_zone_start(maze, rng, counts, zone, &sides)
            } else {
                find_step(maze, rng, counts, zone, &sides)
            }
        })
        .filter(move |step| found.insert(path_of(maze, step)))
        .take(wanted)
}

fn path_of(maze: &Maze, step: &Step) -> Vec<Cell> {
    let cells: Vec<Cell> = std::iter::once(maze.piece(step.from).cell)
        .chain(step.cells.iter().map(|c| c.cell))
        .chain(std::iter::once(maze.piece(step.to).cell))
        .collect();
    let key = |cell: &Cell| (cell.x, cell.y);
    let backwards: Vec<Cell> = cells.iter().rev().copied().collect();
    if cells.iter().map(key).lt(backwards.iter().map(key)) {
        cells
    } else {
        backwards
    }
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
    Choice::of(Kind::Step, &STEP_TERMS, &STEP_PRIORITIES, scored)
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

fn first_loop(grid: Grid, rng: &mut StdRng, grew: &mut impl FnMut(&[Cell])) -> Vec<Cell> {
    let width = rng.random_range(2..=((grid.width - 2) / 4).max(2));
    let height = rng.random_range(2..=((grid.height - 2) / 4).max(2));
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
    grew(&cells);
    let mut taken: HashSet<Cell> = cells.iter().copied().collect();
    let inside = |cell: Cell| {
        (1..grid.width - 1).contains(&cell.x) && (1..grid.height - 1).contains(&cell.y)
    };
    let wanted = (FIRST_LOOP_SHARE * (grid.width * grid.height) as f64) as usize;
    let crowding = |taken: &HashSet<Cell>, new: [Cell; 2]| {
        new.iter()
            .flat_map(|&cell| grid.around(cell, CROWD_REACH))
            .filter(|cell| taken.contains(cell))
            .count()
    };
    let mut attempts = 0;
    while cells.len() < wanted && attempts < BUMP_ATTEMPTS {
        let mut best: Option<(usize, usize, [Cell; 2])> = None;
        let mut found = 0;
        while found < BUMP_CHOICES && attempts < BUMP_ATTEMPTS {
            attempts += 1;
            let i = rng.random_range(0..cells.len());
            let (a, b) = (cells[i], cells[(i + 1) % cells.len()]);
            let Some(along) = Dir::between(a, b) else {
                continue;
            };
            let side = *[along.turned_left(), along.turned_right()]
                .choose(rng)
                .unwrap_or(&along);
            if let (Some(a_out), Some(b_out)) = (grid.neighbour(a, side), grid.neighbour(b, side))
                && inside(a_out)
                && inside(b_out)
                && !taken.contains(&a_out)
                && !taken.contains(&b_out)
            {
                found += 1;
                let crowd = crowding(&taken, [a_out, b_out]);
                if best.is_none_or(|(least, _, _)| crowd < least) {
                    best = Some((crowd, i, [a_out, b_out]));
                }
            }
        }
        let Some((_, i, new)) = best else {
            break;
        };
        cells.splice(i + 1..i + 1, new);
        taken.extend(new);
        grew(&cells);
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
    let limit = rng.random_range(1..=grid.width * grid.height);
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

fn find_zone_start(
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
    let back = *sides
        .iter()
        .filter(|&&(id, dir)| id == from && dir != out)
        .map(|&(_, dir)| dir)
        .collect::<Vec<_>>()
        .choose(rng)?;
    let target = grid.neighbour(from_cell, back)?;
    let first = grid
        .neighbour(from_cell, out)
        .filter(|&cell| can_enter(maze, cell))?;
    let mut visited = vec![false; grid.width * grid.height];
    visited[grid.index(from_cell)] = true;
    visited[grid.index(first)] = true;
    let mut cells = vec![first];
    let mut heading = out;
    let outward = rng.random_range(1..=grid.width * grid.height / ZONE_START_REACH);
    while cells.len() < outward {
        let here = cells[cells.len() - 1];
        let onward: Vec<(Dir, Cell)> = ways_on(maze, here, &heading)
            .iter()
            .filter_map(|&dir| Some((dir, grid.neighbour(here, dir)?)))
            .filter(|&(dir, cell)| {
                !visited[grid.index(cell)] && cell != target && enters_straight(maze, cell, dir)
            })
            .collect();
        let Ok(&(dir, next)) = onward.choose_weighted(rng, |&(_, cell)| {
            ALWAYS_SOME + foreign_share(counts, grid.index(cell), zone)
        }) else {
            break;
        };
        heading = dir;
        visited[grid.index(next)] = true;
        cells.push(next);
    }
    cells.extend(way_back(
        maze,
        &visited,
        cells[cells.len() - 1],
        heading,
        target,
        back,
    )?);
    let step = Step {
        from,
        cells: cells
            .into_iter()
            .map(|cell| StepCell {
                cell,
                on_top: rng.random_bool(0.5),
            })
            .collect(),
        to: from,
        zone,
    };
    maze.check_step(&step).is_ok().then_some(step)
}

fn enters_straight(maze: &Maze, cell: Cell, dir: Dir) -> bool {
    can_enter(maze, cell)
        && match maze.pieces_in(cell) {
            [crossed] => !maze.piece(*crossed).has_side(dir),
            _ => true,
        }
}

fn way_back(
    maze: &Maze,
    visited: &[bool],
    tip: Cell,
    heading: Dir,
    target: Cell,
    back: Dir,
) -> Option<Vec<Cell>> {
    let grid = maze.grid;
    let state = |cell: Cell, dir: Dir| grid.index(cell) * 4 + dir as usize;
    let arrived = |cell: Cell, dir: Dir| {
        cell == target && ways_on(maze, cell, &dir).contains(&back.opposite())
    };
    if arrived(tip, heading) {
        return Some(Vec::new());
    }
    let mut came_from: Vec<Option<(Cell, Dir)>> = vec![None; grid.width * grid.height * 4];
    let mut queue = std::collections::VecDeque::from([(tip, heading)]);
    let mut seen = vec![false; grid.width * grid.height * 4];
    seen[state(tip, heading)] = true;
    while let Some((here, facing)) = queue.pop_front() {
        for &dir in ways_on(maze, here, &facing) {
            let Some(next) = grid.neighbour(here, dir) else {
                continue;
            };
            if visited[grid.index(next)]
                || seen[state(next, dir)]
                || !enters_straight(maze, next, dir)
            {
                continue;
            }
            seen[state(next, dir)] = true;
            came_from[state(next, dir)] = Some((here, facing));
            if arrived(next, dir) {
                let mut route = vec![next];
                let mut at = (here, facing);
                while at != (tip, heading) {
                    route.push(at.0);
                    at = came_from[state(at.0, at.1)]?;
                }
                route.reverse();
                return Some(route);
            }
            queue.push_back((next, dir));
        }
    }
    None
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
        let seeds = 40;
        let grid = crate::book::STANDARD;
        let started = std::time::Instant::now();
        let made: Vec<Maze> = (0..seeds)
            .filter_map(|seed| generate(grid, seed, crate::book::zones_for(grid)).ok())
            .collect();
        for maze in &made {
            assert_eq!(broken(maze), vec![]);
        }
        let all: Vec<crate::goals::Measures> = made.iter().map(crate::goals::measure).collect();
        println!(
            "failed {} of {seeds}, solution cells {:.0}",
            seeds as usize - made.len(),
            made.iter().map(|maze| maze.solution().len()).sum::<usize>() as f64 / made.len() as f64
        );
        let average = |f: &dyn Fn(&crate::goals::Measures) -> f64| {
            all.iter().map(f).sum::<f64>() / all.len() as f64
        };
        println!(
            "long_way_round {:.2} solution_coverage {:.2} crossings {:.0} crossings_in_a_row {:.0} joins_apart {:.1} filling {:.3} finish_near_start {:.2} solution_length {:.2} zones_mixed {:.2} away_from_border {:.2} solver_effort {:.2} {:.2} {:.2} {:.2} each {:?}",
            average(&|m| m.long_way_round),
            average(&|m| m.solution_coverage),
            average(&|m| m.crossings as f64),
            average(&|m| m.crossings_in_a_row as f64),
            average(&|m| m.joins_apart),
            average(&|m| m.filling),
            average(&|m| m.finish_near_start),
            average(&|m| m.solution_length),
            average(&|m| m.zones_mixed),
            average(&|m| m.away_from_border),
            average(&|m| m.solver_effort[0]),
            average(&|m| m.solver_effort[1]),
            average(&|m| m.solver_effort[2]),
            average(&|m| m.solver_effort[3]),
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
