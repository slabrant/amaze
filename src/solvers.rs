use crate::grid::Dir;
use crate::maze::{Maze, PieceId};
use rand::prelude::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Solver {
    Wanderer,
    Homing,
    Contrary,
    Straight,
}

impl Solver {
    pub const ALL: [Solver; 4] = [
        Solver::Wanderer,
        Solver::Homing,
        Solver::Contrary,
        Solver::Straight,
    ];
}

pub fn steps_to_finish(maze: &Maze, solver: Solver, rng: &mut StdRng, most: usize) -> usize {
    let Some(&first) = maze.pieces_in(maze.start).first() else {
        return most;
    };
    let mut here = first;
    let mut came_from: Option<PieceId> = None;
    let mut taken: HashMap<(PieceId, PieceId), usize> = HashMap::new();
    for steps in 0..most {
        if maze.piece(here).cell == maze.finish {
            return steps;
        }
        let onward: Vec<(Dir, PieceId)> = maze
            .piece(here)
            .links
            .iter()
            .copied()
            .filter(|&(_, next)| Some(next) != came_from)
            .collect();
        let next = choose(maze, solver, rng, here, came_from, &onward, &taken);
        *taken.entry((here, next.1)).or_default() += 1;
        came_from = Some(here);
        here = next.1;
    }
    most
}

fn choose(
    maze: &Maze,
    solver: Solver,
    rng: &mut StdRng,
    here: PieceId,
    came_from: Option<PieceId>,
    onward: &[(Dir, PieceId)],
    taken: &HashMap<(PieceId, PieceId), usize>,
) -> (Dir, PieceId) {
    let least_taken = onward
        .iter()
        .map(|&(_, next)| taken.get(&(here, next)).copied().unwrap_or(0))
        .min()
        .unwrap_or(0);
    let fresh: Vec<(Dir, PieceId)> = onward
        .iter()
        .copied()
        .filter(|&(_, next)| taken.get(&(here, next)).copied().unwrap_or(0) == least_taken)
        .collect();
    let to_finish =
        |&(_, next): &(Dir, PieceId)| maze.piece(next).cell.squared_distance(maze.finish);
    let heading = came_from.and_then(|before| {
        maze.piece(here)
            .links
            .iter()
            .find(|&&(_, linked)| linked == before)
            .map(|&(dir, _)| dir.opposite())
    });
    let preferred: Vec<(Dir, PieceId)> = match solver {
        Solver::Wanderer => onward.to_vec(),
        Solver::Homing => keep_best(&fresh, |option| to_finish(option) as i64),
        Solver::Contrary => keep_best(&fresh, |option| -(to_finish(option) as i64)),
        Solver::Straight => keep_best(&fresh, |&(dir, _)| i64::from(Some(dir) != heading)),
    };
    *preferred.choose(rng).unwrap_or(&onward[0])
}

fn keep_best(
    options: &[(Dir, PieceId)],
    cost: impl Fn(&(Dir, PieceId)) -> i64,
) -> Vec<(Dir, PieceId)> {
    let best = options.iter().map(&cost).min().unwrap_or(0);
    options
        .iter()
        .copied()
        .filter(|option| cost(option) == best)
        .collect()
}
