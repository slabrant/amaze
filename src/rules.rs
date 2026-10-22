use crate::grid::Cell;
use crate::maze::{Content, Maze, PieceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Broken {
    TooManyPaths(Cell),
    JunctionCrossed(Cell),
    NoRouteToStart(Cell),
    ZonesJoinedOutOfOrder(Cell),
    ZonesNotJoinedAtOneJunction(usize),
    ZonesNotJoined(usize),
    StartNotInFirstZone,
    FinishNotInLastZone,
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
    found.extend(broken_zones(maze));
    found
}

fn broken_zones(maze: &Maze) -> Vec<Broken> {
    let mut found = Vec::new();
    let mut join_junctions: Vec<Vec<PieceId>> = vec![Vec::new(); maze.zones()];
    for (id, piece) in maze.pieces().iter().enumerate() {
        for &(_, other) in &piece.links {
            let other_zone = maze.piece(other).zone;
            if other_zone == piece.zone + 1 {
                join_junctions[piece.zone].push(id);
            } else if other_zone > piece.zone + 1 {
                found.push(Broken::ZonesJoinedOutOfOrder(piece.cell));
            }
        }
    }
    for (zone, junctions) in join_junctions.iter_mut().enumerate().take(maze.zones() - 1) {
        junctions.dedup();
        match junctions.as_slice() {
            [] => found.push(Broken::ZonesNotJoined(zone)),
            [junction] if maze.content(maze.piece(*junction).cell) == Content::Junction => {}
            _ => found.push(Broken::ZonesNotJoinedAtOneJunction(zone)),
        }
    }
    let zones_in = |cell: Cell| maze.pieces_in(cell).iter().map(|&id| maze.piece(id).zone);
    if zones_in(maze.start).any(|zone| zone != 0) {
        found.push(Broken::StartNotInFirstZone);
    }
    if zones_in(maze.finish).any(|zone| zone != maze.zones() - 1) {
        found.push(Broken::FinishNotInLastZone);
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
