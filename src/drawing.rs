use crate::grid::{Cell, Dir, Grid};
use crate::maze::{Maze, PieceId};

const CORRIDOR: f64 = 0.75;
const HALF: f64 = CORRIDOR / 2.0;
const EDGE: f64 = 0.06;
const LIGHT: Point = (
    std::f64::consts::FRAC_1_SQRT_2,
    std::f64::consts::FRAC_1_SQRT_2,
);
const DECK_TILES_ALONG: usize = 24;
const DECK_TILES_ACROSS: usize = 12;
const DECK_SHADE: f64 = 0.3;
const DECK_SUN_SIDE: f64 = 0.6;
const DECK_FADE: f64 = 0.25;
const TILE_OVERLAP: f64 = 0.003;
const SHADOW_DEPTH: f64 = 0.3;
const SHADOW_BANDS: usize = 12;
const SUN_SIDE_SHADOW: f64 = 0.6;
const SHADOW: f64 = 0.62;
const BACKGROUND_SHADOW_DEPTH: f64 = 0.27;
const BACKGROUND_SHADOW_LAYERS: usize = 8;
const BACKGROUND_SHADOW: f64 = 0.5;
const ROUNDING: f64 = 0.25;
const OVERLAP: f64 = 0.02;
const INNER_ROUNDING: f64 = 0.1;
const FRAME_ROUNDING: f64 = ROUNDING + 0.5 - HALF;
const ARC_STEPS: usize = 6;
const MARK: f64 = HALF;
const ROUTE: f64 = 0.3;
const ROUTE_GAP: f64 = HALF + 0.08;
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
    Polygon {
        points: Vec<Point>,
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
            Mark::Polygon { points, colour } => Mark::Polygon {
                points: points.iter().map(|&point| place(point)).collect(),
                colour: *colour,
            },
        }
    }
}

pub fn draw(maze: &Maze) -> Vec<Mark> {
    let mut fills: Vec<Vec<Point>> = Vec::new();
    let mut edges: Vec<Vec<Point>> = vec![frame(maze.grid)];
    let mut shading: Vec<Mark> = Vec::new();
    for cell in maze.grid.cells() {
        let ids = maze.pieces_in(cell);
        if ids.is_empty() {
            continue;
        }
        let joined = Dir::ALL.map(|dir| ids.iter().any(|&id| maze.piece(id).has_side(dir)));
        let ring = ring(cell, joined);
        fills.push(ring.iter().map(|&(point, _)| point).collect());
        let inner = |a: Dir, b: Dir| {
            ids.iter()
                .any(|&id| maze.piece(id).has_side(a) && maze.piece(id).has_side(b))
        };
        for (i, dir) in Dir::ALL.into_iter().enumerate() {
            if joined[i] {
                fills.push(overlapping(arm(cell, dir), dir));
                edges.extend(arm_edges(
                    cell,
                    dir,
                    inner(dir, dir.turned_left()),
                    inner(dir, dir.turned_right()),
                ));
            }
        }
        for (i, a) in Dir::ALL.into_iter().enumerate() {
            let b = Dir::ALL[(i + 1) % 4];
            if inner(a, b) {
                let arc = fillet(cell, a, b);
                let mut fill = vec![corner(cell, a, b)];
                fill.extend(arc.iter().copied());
                fills.push(fill);
                edges.push(arc);
            }
        }
        edges.extend(wall_faces(&ring));
        if let [_, top] = ids {
            let along = if maze.piece(*top).has_side(Dir::North) {
                Dir::South
            } else {
                Dir::East
            };
            let flat = |dir: Dir| {
                maze.piece(*top).links.iter().any(|&(side, next)| {
                    side == dir
                        && maze.pieces_in(maze.piece(next).cell).last() == Some(&next)
                        && maze.pieces_in(maze.piece(next).cell).len() == 2
                })
            };
            shading.extend(bridge(cell, along, [!flat(along), !flat(along.opposite())]));
            edges.extend(across(cell, maze.piece(*top).has_side(Dir::North)));
        }
    }
    let mut marks = background_shadow(&fills);
    marks.extend(outside_frame(maze.grid));
    marks.extend(fills.into_iter().map(|points| Mark::Polygon {
        points,
        colour: Colour::WHITE,
    }));
    marks.extend(shading);
    marks.extend(start_mark(maze, maze.start));
    marks.extend(finish_mark(maze, maze.finish));
    marks.push(lines(edges, EDGE, Colour::BLACK));
    marks
}

fn offset(dir: Dir) -> Point {
    match dir {
        Dir::North => (0.0, -1.0),
        Dir::East => (1.0, 0.0),
        Dir::South => (0.0, 1.0),
        Dir::West => (-1.0, 0.0),
    }
}

fn facing(dir: Dir) -> f64 {
    let (dx, dy) = offset(dir);
    dx * LIGHT.0 + dy * LIGHT.1
}

fn bridge(cell: Cell, along: Dir, ramps: [bool; 2]) -> Vec<Mark> {
    let (x, y) = centre(cell);
    let across = along.turned_right();
    let place = |a: f64, b: f64| {
        let ((ux, uy), (wx, wy)) = (offset(along), offset(across));
        (x + ux * a + wx * b, y + uy * a + wy * b)
    };
    let rectangle = |(a0, a1): (f64, f64), (b0, b1): (f64, f64)| {
        vec![place(a0, b0), place(a1, b0), place(a1, b1), place(a0, b1)]
    };
    let (light_along, light_across) = (facing(along), facing(across));
    let down = |a: f64| {
        let ramp = if a >= 0.0 { ramps[0] } else { ramps[1] };
        if ramp && a * light_along > 0.0 {
            if a.abs() <= 0.5 {
                2.0 * a.abs()
            } else {
                (1.0 - (a.abs() - 0.5) / DECK_FADE).max(0.0)
            }
        } else {
            0.0
        }
    };
    let away = |b: f64| 1.0 - DECK_SUN_SIDE * (0.5 - b * light_across.signum() / CORRIDOR);
    let step = |n: usize| 1.0 / n as f64;
    let mut marks = Vec::new();
    let reach = 0.5 + DECK_FADE;
    let tiles = (2.0 * reach * DECK_TILES_ALONG as f64).round() as usize;
    for i in 0..tiles {
        for j in 0..DECK_TILES_ACROSS {
            let (a0, a1) = (
                i as f64 * step(DECK_TILES_ALONG) - reach,
                (i + 1) as f64 * step(DECK_TILES_ALONG) - reach,
            );
            let (b0, b1) = (
                (j as f64 * step(DECK_TILES_ACROSS) - 0.5) * CORRIDOR,
                ((j + 1) as f64 * step(DECK_TILES_ACROSS) - 0.5) * CORRIDOR,
            );
            let darkness = DECK_SHADE * down((a0 + a1) / 2.0) * away((b0 + b1) / 2.0);
            if darkness < 0.005 {
                continue;
            }
            marks.push(Mark::Polygon {
                points: rectangle(
                    (a0 - TILE_OVERLAP, a1 + TILE_OVERLAP),
                    (b0 - TILE_OVERLAP, b1 + TILE_OVERLAP),
                ),
                colour: Colour::grey(1.0 - darkness),
            });
        }
    }
    for sign in [1.0, -1.0] {
        let darkness = if sign * light_across >= 0.0 {
            1.0 - SHADOW
        } else {
            (1.0 - SHADOW) * SUN_SIDE_SHADOW
        };
        for band in 0..SHADOW_BANDS {
            let from = HALF + SHADOW_DEPTH * band as f64 / SHADOW_BANDS as f64;
            let to = HALF + SHADOW_DEPTH * (band + 1) as f64 / SHADOW_BANDS as f64;
            let fade = band as f64 / SHADOW_BANDS as f64;
            marks.push(Mark::Polygon {
                points: rectangle((-HALF, HALF), (sign * from, sign * (to + TILE_OVERLAP))),
                colour: Colour::grey(1.0 - darkness * (1.0 - fade)),
            });
        }
    }
    marks
}

fn corner(cell: Cell, a: Dir, b: Dir) -> Point {
    let (x, y) = centre(cell);
    let ((ax, ay), (bx, by)) = (offset(a), offset(b));
    (x + (ax + bx) * HALF, y + (ay + by) * HALF)
}

fn ring(cell: Cell, joined: [bool; 4]) -> Vec<(Point, bool)> {
    let mut points = Vec::new();
    for i in 0..4 {
        let before = Dir::ALL[(i + 3) % 4];
        let after = Dir::ALL[i];
        let wall_before = !joined[(i + 3) % 4];
        let wall_after = !joined[i];
        let (cx, cy) = corner(cell, before, after);
        if wall_before && wall_after {
            let ((bx, by), (ax, ay)) = (offset(before), offset(after));
            let (ox, oy) = (cx - (bx + ax) * ROUNDING, cy - (by + ay) * ROUNDING);
            for step in 0..=ARC_STEPS {
                let angle = std::f64::consts::FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
                let (toward_before, toward_after) = (angle.cos(), angle.sin());
                points.push((
                    (
                        ox + (bx * toward_before + ax * toward_after) * ROUNDING,
                        oy + (by * toward_before + ay * toward_after) * ROUNDING,
                    ),
                    true,
                ));
            }
        } else {
            points.push(((cx, cy), wall_after));
        }
    }
    points
}

fn wall_faces(ring: &[(Point, bool)]) -> Vec<Vec<Point>> {
    (0..ring.len())
        .filter(|&i| ring[i].1)
        .map(|i| vec![ring[i].0, ring[(i + 1) % ring.len()].0])
        .collect()
}

fn arm(cell: Cell, dir: Dir) -> Vec<Point> {
    let (left, right) = (dir.turned_left(), dir.turned_right());
    let (dx, dy) = offset(dir);
    let reach = 0.5 - HALF;
    let near_left = corner(cell, dir, left);
    let near_right = corner(cell, dir, right);
    vec![
        near_left,
        near_right,
        (near_right.0 + dx * reach, near_right.1 + dy * reach),
        (near_left.0 + dx * reach, near_left.1 + dy * reach),
    ]
}

fn overlapping(arm: Vec<Point>, dir: Dir) -> Vec<Point> {
    let (dx, dy) = offset(dir);
    let (ox, oy) = (dx * OVERLAP, dy * OVERLAP);
    vec![
        (arm[0].0 - ox, arm[0].1 - oy),
        (arm[1].0 - ox, arm[1].1 - oy),
        (arm[2].0 + ox, arm[2].1 + oy),
        (arm[3].0 + ox, arm[3].1 + oy),
    ]
}

fn arm_edges(cell: Cell, dir: Dir, inner_left: bool, inner_right: bool) -> Vec<Vec<Point>> {
    let points = arm(cell, dir);
    let (dx, dy) = offset(dir);
    let start = |point: Point, inner: bool| {
        let pull = if inner { INNER_ROUNDING } else { 0.0 };
        (point.0 + dx * pull, point.1 + dy * pull)
    };
    vec![
        vec![start(points[0], inner_left), points[3]],
        vec![start(points[1], inner_right), points[2]],
    ]
}

fn fillet(cell: Cell, a: Dir, b: Dir) -> Vec<Point> {
    let (cx, cy) = corner(cell, a, b);
    let ((ax, ay), (bx, by)) = (offset(a), offset(b));
    let (ox, oy) = (
        cx + (ax + bx) * INNER_ROUNDING,
        cy + (ay + by) * INNER_ROUNDING,
    );
    (0..=ARC_STEPS)
        .map(|step| {
            let angle = std::f64::consts::FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
            (
                ox - (bx * angle.cos() + ax * angle.sin()) * INNER_ROUNDING,
                oy - (by * angle.cos() + ay * angle.sin()) * INNER_ROUNDING,
            )
        })
        .collect()
}

fn across(cell: Cell, north_south: bool) -> Vec<Vec<Point>> {
    let (x, y) = centre(cell);
    [-HALF, HALF]
        .into_iter()
        .map(|side| {
            if north_south {
                vec![(x + side, y - HALF), (x + side, y + HALF)]
            } else {
                vec![(x - HALF, y + side), (x + HALF, y + side)]
            }
        })
        .collect()
}

fn frame_corners(grid: Grid) -> [(Point, Vec<Point>); 4] {
    let (width, height) = (grid.width as f64, grid.height as f64);
    let r = FRAME_ROUNDING;
    let quarter = std::f64::consts::FRAC_PI_2;
    let arc = |(cx, cy): Point, from: f64| -> Vec<Point> {
        (0..=ARC_STEPS)
            .map(|step| {
                let angle = from + quarter * step as f64 / ARC_STEPS as f64;
                (cx + r * angle.cos(), cy + r * angle.sin())
            })
            .collect()
    };
    [
        ((0.0, 0.0), arc((r, r), 2.0 * quarter)),
        ((width, 0.0), arc((width - r, r), 3.0 * quarter)),
        ((width, height), arc((width - r, height - r), 0.0)),
        ((0.0, height), arc((r, height - r), quarter)),
    ]
}

fn frame(grid: Grid) -> Vec<Point> {
    let mut points: Vec<Point> = frame_corners(grid)
        .into_iter()
        .flat_map(|(_, arc)| arc)
        .collect();
    points.push(points[0]);
    points
}

fn outside_frame(grid: Grid) -> Vec<Mark> {
    frame_corners(grid)
        .into_iter()
        .map(|(corner, arc)| {
            let mut points = vec![corner];
            points.extend(arc);
            Mark::Polygon {
                points,
                colour: Colour::WHITE,
            }
        })
        .collect()
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
            HALF,
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

fn background_shadow(fills: &[Vec<Point>]) -> Vec<Mark> {
    (1..=BACKGROUND_SHADOW_LAYERS)
        .rev()
        .flat_map(|layer| {
            let reach = BACKGROUND_SHADOW_DEPTH * layer as f64 / BACKGROUND_SHADOW_LAYERS as f64;
            let fade = (layer - 1) as f64 / BACKGROUND_SHADOW_LAYERS as f64;
            let colour = Colour::grey(BACKGROUND_SHADOW + (1.0 - BACKGROUND_SHADOW) * fade);
            fills.iter().map(move |points| Mark::Polygon {
                points: points
                    .iter()
                    .map(|&(x, y)| (x + LIGHT.0 * reach, y + LIGHT.1 * reach))
                    .collect(),
                colour,
            })
        })
        .collect()
}

fn cell_shape(maze: &Maze, cell: Cell, colour: Colour) -> Mark {
    let ids = maze.pieces_in(cell);
    let joined = Dir::ALL.map(|dir| ids.iter().any(|&id| maze.piece(id).has_side(dir)));
    Mark::Polygon {
        points: ring(cell, joined)
            .into_iter()
            .map(|(point, _)| point)
            .collect(),
        colour,
    }
}

fn start_mark(maze: &Maze, cell: Cell) -> Vec<Mark> {
    let at = centre(cell);
    vec![
        cell_shape(maze, cell, Colour::START),
        Mark::Disc {
            centre: at,
            radius: MARK * 0.45,
            colour: Colour::WHITE,
        },
    ]
}

fn finish_mark(maze: &Maze, cell: Cell) -> Vec<Mark> {
    let at = centre(cell);
    vec![
        cell_shape(maze, cell, Colour::FINISH),
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
