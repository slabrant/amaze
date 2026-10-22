use crate::grid::{Cell, Dir};
use crate::maze::{Maze, PieceId};
use std::fmt::Write;

const CELL: f64 = 20.0;
const PATH: f64 = 9.0;
const EDGE: f64 = 1.5;
const GAP: f64 = 2.5;

pub fn render(maze: &Maze) -> String {
    let width = maze.grid.width as f64 * CELL;
    let height = maze.grid.height as f64 * CELL;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="100%" height="100%" fill="white"/>"#
    );
    let lower: String = maze
        .grid
        .cells()
        .filter_map(|cell| maze.pieces_in(cell).first())
        .map(|&id| outline(maze, id))
        .collect();
    stroke(&mut svg, &lower, "black", PATH);
    stroke(&mut svg, &lower, "white", PATH - 2.0 * EDGE);
    for cell in maze.grid.cells() {
        if let [_, top] = maze.pieces_in(cell) {
            let upper = outline(maze, *top);
            stroke(&mut svg, &upper, "white", PATH + 2.0 * GAP);
            stroke(&mut svg, &upper, "black", PATH);
            stroke(&mut svg, &upper, "white", PATH - 2.0 * EDGE);
        }
    }
    svg.push_str("</svg>");
    svg
}

fn stroke(svg: &mut String, d: &str, colour: &str, width: f64) {
    write!(
        svg,
        r#"<path d="{d}" fill="none" stroke="{colour}" stroke-width="{width}" stroke-linejoin="round"/>"#
    )
    .unwrap();
}

fn outline(maze: &Maze, id: PieceId) -> String {
    let piece = maze.piece(id);
    let (cx, cy) = centre(piece.cell);
    let sides: Vec<(f64, f64)> = piece
        .links
        .iter()
        .map(|&(dir, _)| side(piece.cell, dir))
        .collect();
    sides
        .windows(2)
        .map(|pair| {
            let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
            format!("M{ax} {ay}L{cx} {cy}L{bx} {by}")
        })
        .collect()
}

fn centre(cell: Cell) -> (f64, f64) {
    ((cell.x as f64 + 0.5) * CELL, (cell.y as f64 + 0.5) * CELL)
}

fn side(cell: Cell, dir: Dir) -> (f64, f64) {
    let (x, y) = centre(cell);
    let half = CELL / 2.0;
    match dir {
        Dir::North => (x, y - half),
        Dir::East => (x + half, y),
        Dir::South => (x, y + half),
        Dir::West => (x - half, y),
    }
}
