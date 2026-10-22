use crate::grid::Cell;
use crate::maze::{Content, Maze, PieceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    TooManyPaths(Cell),
    JunctionCrossed(Cell),
    NoRouteToStart(Cell),
    DeadEnd(Cell),
}

pub fn broken(maze: &Maze) -> Vec<Broken> {
    let mut found = Vec::new();
    for cell in maze.grid.cells() {
        let ids = maze.pieces_in(cell);
        if ids.len() > 2 {
            found.push(Broken::TooManyPaths(cell));
        }
        if ids.len() > 1 && ids.iter().any(|&id| maze.piece(id).links.len() > 2) {
            found.push(Broken::JunctionCrossed(cell));
        }
    }
    let reached = reached_from(maze, maze.start);
    for (id, piece) in maze.pieces().iter().enumerate() {
        if !reached[id] {
            found.push(Broken::NoRouteToStart(piece.cell));
        }
        if piece.links.len() < 2 {
            found.push(Broken::DeadEnd(piece.cell));
        }
    }
    if maze.content(maze.start) == Content::Empty {
        found.push(Broken::NoRouteToStart(maze.start));
    }
    found
}

fn reached_from(maze: &Maze, cell: Cell) -> Vec<bool> {
    let mut reached = vec![false; maze.pieces().len()];
    let mut queue: Vec<PieceId> = maze.pieces_in(cell).to_vec();
    while let Some(id) = queue.pop() {
        if !std::mem::replace(&mut reached[id], true) {
            queue.extend(maze.piece(id).links.iter().map(|&(_, other)| other));
        }
    }
    reached
}
