use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Content, Maze, PieceId};

const PATH: f64 = 0.7;
const EDGE: f64 = 0.04;
const UNDER_DASH: [f64; 2] = [0.1, 0.07];
const HATCH_SPACING: f64 = 0.18;
const HATCH_WIDTH: f64 = 0.02;
const MARK: f64 = PATH / 2.0;
const ROUTE: f64 = 0.3;
const ROUTE_GAP: f64 = PATH / 2.0 + 0.08;
const ROUTE_HUES: (f64, f64) = (0.0, 0.78);

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
    pub const HATCH: Colour = Colour::grey(0.6);
    pub const UNDER: Colour = Colour::grey(0.5);
    pub const START: Colour = Colour {
        red: 0.86,
        green: 0.12,
        blue: 0.12,
    };
    pub const FINISH: Colour = Colour {
        red: 0.58,
        green: 0.1,
        blue: 0.86,
    };

    pub const fn grey(level: f64) -> Colour {
        Colour {
            red: level,
            green: level,
            blue: level,
        }
    }

    pub fn hue(turn: f64) -> Colour {
        let sector = (turn.rem_euclid(1.0)) * 6.0;
        let rising = sector.fract();
        let falling = 1.0 - rising;
        let (red, green, blue) = match sector as usize {
            0 => (1.0, rising, 0.0),
            1 => (falling, 1.0, 0.0),
            2 => (0.0, 1.0, rising),
            3 => (0.0, falling, 1.0),
            4 => (rising, 0.0, 1.0),
            _ => (1.0, 0.0, falling),
        };
        let deepen = 0.85;
        Colour {
            red: red * deepen,
            green: green * deepen,
            blue: blue * deepen,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Lines {
        lines: Vec<Vec<Point>>,
        width: f64,
        colour: Colour,
        dash: Option<[f64; 2]>,
        round: bool,
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
                dash,
                round,
            } => Mark::Lines {
                lines: lines
                    .iter()
                    .map(|line| line.iter().map(|&point| place(point)).collect())
                    .collect(),
                width: width * scale,
                colour: *colour,
                dash: dash.map(|[on, off]| [on * scale, off * scale]),
                round: *round,
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
    let mut marks = vec![hatching(maze.grid)];
    for layer in 0..2 {
        let shapes: Vec<Vec<Point>> = maze
            .grid
            .cells()
            .filter_map(|cell| maze.pieces_in(cell).get(layer))
            .flat_map(|&id| outline(maze, id))
            .collect();
        marks.push(lines(shapes.clone(), PATH, Colour::BLACK));
        marks.push(lines(shapes, PATH - 2.0 * EDGE, Colour::WHITE));
    }
    let under: Vec<Vec<Point>> = maze
        .grid
        .cells()
        .filter(|&cell| maze.content(cell) == Content::Crossing)
        .flat_map(|cell| under_edges(maze, maze.pieces_in(cell)[0]))
        .collect();
    marks.push(Mark::Lines {
        lines: under,
        width: EDGE,
        colour: Colour::UNDER,
        dash: Some(UNDER_DASH),
        round: false,
    });
    marks.extend(start_mark(centre(maze.start)));
    marks.extend(finish_mark(centre(maze.finish)));
    marks
}

pub fn draw_solution(maze: &Maze) -> Vec<Mark> {
    let route = maze.solution();
    let halves = route.windows(2).flat_map(|pair| {
        let middle = side(
            maze.piece(pair[0]).cell,
            Dir::between(maze.piece(pair[0]).cell, maze.piece(pair[1]).cell)
                .expect("route pieces are in neighbouring cells"),
        );
        [(pair[0], middle), (pair[1], middle)]
    });
    let last = route.len().saturating_sub(1).max(1) as f64;
    halves
        .enumerate()
        .map(|(i, (id, edge))| {
            let along = i.div_ceil(2);
            let colour =
                Colour::hue(ROUTE_HUES.0 + (ROUTE_HUES.1 - ROUTE_HUES.0) * along as f64 / last);
            Mark::Lines {
                lines: vec![half_route(maze, id, edge)],
                width: ROUTE,
                colour,
                dash: None,
                round: true,
            }
        })
        .collect()
}

pub fn draw_zones(maze: &Maze, colours: &[Colour]) -> Vec<Mark> {
    let mut marks = draw(maze);
    for (id, piece) in maze.pieces().iter().enumerate() {
        marks.push(lines(
            outline(maze, id),
            PATH / 2.0,
            colours[piece.zone % colours.len()],
        ));
    }
    marks
}

fn half_route(maze: &Maze, id: PieceId, edge: Point) -> Vec<Point> {
    let cell = maze.piece(id).cell;
    let middle = centre(cell);
    let goes_under = maze.pieces_in(cell).len() == 2 && maze.pieces_in(cell)[0] == id;
    let stop = if goes_under { ROUTE_GAP } else { 0.0 };
    let toward = (edge.0 - middle.0, edge.1 - middle.1);
    let length = (toward.0 * toward.0 + toward.1 * toward.1).sqrt();
    let scale = stop / length;
    vec![
        edge,
        (middle.0 + toward.0 * scale, middle.1 + toward.1 * scale),
    ]
}

fn hatching(grid: Grid) -> Mark {
    let (width, height) = (grid.width as f64, grid.height as f64);
    let count = ((width + height) / HATCH_SPACING).ceil() as usize;
    let lines = (1..count)
        .map(|i| {
            let offset = i as f64 * HATCH_SPACING;
            let start = if offset <= height {
                (0.0, offset)
            } else {
                (offset - height, height)
            };
            let end = if offset <= width {
                (offset, 0.0)
            } else {
                (width, offset - width)
            };
            vec![start, end]
        })
        .collect();
    Mark::Lines {
        lines,
        width: HATCH_WIDTH,
        colour: Colour::HATCH,
        dash: None,
        round: false,
    }
}

fn under_edges(maze: &Maze, id: PieceId) -> Vec<Vec<Point>> {
    let piece = maze.piece(id);
    let (x, y) = centre(piece.cell);
    let inset = PATH / 2.0 - EDGE / 2.0;
    let across = PATH / 2.0;
    let runs_north_south = piece.has_side(Dir::North);
    [-inset, inset]
        .into_iter()
        .map(|offset| {
            if runs_north_south {
                vec![(x + offset, y - across), (x + offset, y + across)]
            } else {
                vec![(x - across, y + offset), (x + across, y + offset)]
            }
        })
        .collect()
}

fn start_mark(at: Point) -> Vec<Mark> {
    vec![
        Mark::Square {
            centre: at,
            half: MARK,
            colour: Colour::START,
        },
        Mark::Disc {
            centre: at,
            radius: MARK * 0.45,
            colour: Colour::WHITE,
        },
    ]
}

fn finish_mark(at: Point) -> Vec<Mark> {
    vec![
        Mark::Square {
            centre: at,
            half: MARK,
            colour: Colour::FINISH,
        },
        Mark::Disc {
            centre: at,
            radius: MARK * 0.7,
            colour: Colour::WHITE,
        },
        Mark::Disc {
            centre: at,
            radius: MARK * 0.5,
            colour: Colour::FINISH,
        },
        Mark::Disc {
            centre: at,
            radius: MARK * 0.22,
            colour: Colour::WHITE,
        },
    ]
}

fn lines(lines: Vec<Vec<Point>>, width: f64, colour: Colour) -> Mark {
    Mark::Lines {
        lines,
        width,
        colour,
        dash: None,
        round: false,
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
