use crate::grid::{Cell, Dir};
use crate::maze::{Maze, PieceId};

const PATH: f64 = 0.45;
const EDGE: f64 = 0.075;
const GAP: f64 = 0.125;
const MARK: f64 = 0.15;
const ROUTE: f64 = 0.15;

pub type Point = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colour {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

impl Colour {
    pub const BLACK: Colour = Colour::grey(0.0);
    pub const WHITE: Colour = Colour::grey(1.0);
    pub const SOLUTION: Colour = Colour::grey(0.45);

    pub const fn grey(level: f64) -> Colour {
        Colour {
            red: level,
            green: level,
            blue: level,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Lines {
        lines: Vec<Vec<Point>>,
        width: f64,
        colour: Colour,
    },
    Disc {
        centre: Point,
        radius: f64,
        colour: Colour,
    },
    Square {
        centre: Point,
        half: f64,
        colour: Colour,
    },
}

impl Mark {
    pub fn moved(&self, scale: f64, place: impl Fn(Point) -> Point) -> Mark {
        match self {
            Mark::Lines {
                lines,
                width,
                colour,
            } => Mark::Lines {
                lines: lines
                    .iter()
                    .map(|line| line.iter().map(|&point| place(point)).collect())
                    .collect(),
                width: width * scale,
                colour: *colour,
            },
            Mark::Disc {
                centre,
                radius,
                colour,
            } => Mark::Disc {
                centre: place(*centre),
                radius: radius * scale,
                colour: *colour,
            },
            Mark::Square {
                centre,
                half,
                colour,
            } => Mark::Square {
                centre: place(*centre),
                half: half * scale,
                colour: *colour,
            },
        }
    }
}

pub fn draw(maze: &Maze) -> Vec<Mark> {
    let lower: Vec<Vec<Point>> = maze
        .grid
        .cells()
        .filter_map(|cell| maze.pieces_in(cell).first())
        .flat_map(|&id| outline(maze, id))
        .collect();
    let mut marks = vec![
        lines(lower.clone(), PATH, Colour::BLACK),
        lines(lower, PATH - 2.0 * EDGE, Colour::WHITE),
    ];
    for cell in maze.grid.cells() {
        if let [_, top] = maze.pieces_in(cell) {
            let upper = outline(maze, *top);
            marks.push(lines(upper.clone(), PATH + 2.0 * GAP, Colour::WHITE));
            marks.push(lines(upper.clone(), PATH, Colour::BLACK));
            marks.push(lines(upper, PATH - 2.0 * EDGE, Colour::WHITE));
        }
    }
    marks.push(Mark::Disc {
        centre: centre(maze.start),
        radius: MARK,
        colour: Colour::BLACK,
    });
    marks.push(Mark::Square {
        centre: centre(maze.finish),
        half: MARK,
        colour: Colour::BLACK,
    });
    marks
}

pub fn draw_solution(maze: &Maze, colour: Colour) -> Vec<Mark> {
    let mut marks = draw(maze);
    let route = maze
        .solution()
        .iter()
        .map(|&id| centre(maze.piece(id).cell))
        .collect();
    marks.push(lines(vec![route], ROUTE, colour));
    marks
}

pub fn draw_zones(maze: &Maze, colours: &[Colour]) -> Vec<Mark> {
    let mut marks = draw(maze);
    for (id, piece) in maze.pieces().iter().enumerate() {
        marks.push(lines(
            outline(maze, id),
            PATH - 3.0 * EDGE,
            colours[piece.zone % colours.len()],
        ));
    }
    marks
}

fn lines(lines: Vec<Vec<Point>>, width: f64, colour: Colour) -> Mark {
    Mark::Lines {
        lines,
        width,
        colour,
    }
}

fn outline(maze: &Maze, id: PieceId) -> Vec<Vec<Point>> {
    let piece = maze.piece(id);
    let middle = centre(piece.cell);
    let sides: Vec<Point> = piece
        .links
        .iter()
        .map(|&(dir, _)| side(piece.cell, dir))
        .collect();
    sides
        .windows(2)
        .map(|pair| vec![pair[0], middle, pair[1]])
        .collect()
}

fn centre(cell: Cell) -> Point {
    (cell.x as f64 + 0.5, cell.y as f64 + 0.5)
}

fn side(cell: Cell, dir: Dir) -> Point {
    let (x, y) = centre(cell);
    match dir {
        Dir::North => (x, y - 0.5),
        Dir::East => (x + 0.5, y),
        Dir::South => (x, y + 0.5),
        Dir::West => (x - 0.5, y),
    }
}
