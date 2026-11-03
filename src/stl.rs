use crate::grid::Dir;
use crate::maze::{Maze, PieceId};
use std::collections::HashMap;

pub const CELL: f64 = 8.0;
const UNITS: usize = 8;
const MARGIN: usize = 1;
const BASE: f64 = 1.5;
const LOW: f64 = 2.0;
const CLEARANCE: f64 = 2.0;
const DECK: f64 = 1.5;
const HIGH: f64 = LOW + CLEARANCE + DECK;
const MARK: f64 = 0.8;
const HALF: f64 = 0.5 - MARGIN as f64 / UNITS as f64;

type Point = [f64; 3];
type Span = (f64, f64);

struct Columns {
    width: usize,
    height: usize,
    spans: Vec<Vec<Span>>,
}

impl Columns {
    fn get(&self, x: isize, y: isize) -> &[Span] {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return &[];
        }
        &self.spans[y as usize * self.width + x as usize]
    }

    fn add(&mut self, x: usize, y: usize, span: Span) {
        let spans = &mut self.spans[y * self.width + x];
        spans.push(span);
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<Span> = Vec::new();
        for &(from, to) in spans.iter() {
            match merged.last_mut() {
                Some(last) if from <= last.1 => last.1 = last.1.max(to),
                _ => merged.push((from, to)),
            }
        }
        *spans = merged;
    }

    fn raise(&mut self, x: usize, y: usize, by: f64) {
        let spans = &mut self.spans[y * self.width + x];
        if let Some(last) = spans.last_mut() {
            last.1 += by;
        }
    }
}

pub fn stl(maze: &Maze) -> Vec<u8> {
    let columns = columns(maze);
    let triangles = mesh(&columns);
    let mut bytes = vec![0u8; 80];
    bytes.extend((triangles.len() as u32).to_le_bytes());
    for [a, b, c] in &triangles {
        let normal = unit(cross(minus(*b, *a), minus(*c, *a)));
        for point in [normal, *a, *b, *c] {
            for value in point {
                bytes.extend((value as f32).to_le_bytes());
            }
        }
        bytes.extend(0u16.to_le_bytes());
    }
    bytes
}

fn columns(maze: &Maze) -> Columns {
    let (width, height) = (maze.grid.width * UNITS, maze.grid.height * UNITS);
    let mut columns = Columns {
        width,
        height,
        spans: vec![vec![(0.0, BASE)]; width * height],
    };
    for cell in maze.grid.cells() {
        let ids = maze.pieces_in(cell);
        for (layer, &id) in ids.iter().enumerate() {
            let deck = ids.len() == 2 && layer == 1;
            for (a, b) in footprint(maze, id) {
                let height = BASE + surface(maze, id, a, b);
                let floor = if deck && inside(a) && inside(b) {
                    height - DECK
                } else {
                    0.0
                };
                columns.add(cell.x * UNITS + a, cell.y * UNITS + b, (floor, height));
            }
        }
    }
    for (a, b) in start_mark() {
        columns.raise(maze.start.x * UNITS + a, maze.start.y * UNITS + b, MARK);
    }
    for (a, b) in finish_mark() {
        columns.raise(maze.finish.x * UNITS + a, maze.finish.y * UNITS + b, MARK);
    }
    columns
}

fn inside(unit: usize) -> bool {
    (MARGIN..UNITS - MARGIN).contains(&unit)
}

fn start_mark() -> Vec<(usize, usize)> {
    let middle = UNITS / 2 - 1..UNITS / 2 + 1;
    middle
        .clone()
        .flat_map(|a| middle.clone().map(move |b| (a, b)))
        .collect()
}

fn finish_mark() -> Vec<(usize, usize)> {
    let ring = UNITS / 2 - 2..UNITS / 2 + 2;
    let dot = start_mark();
    ring.clone()
        .flat_map(|a| ring.clone().map(move |b| (a, b)))
        .filter(|unit| !dot.contains(unit))
        .collect()
}

fn footprint(maze: &Maze, id: PieceId) -> Vec<(usize, usize)> {
    let piece = maze.piece(id);
    (0..UNITS)
        .flat_map(|a| (0..UNITS).map(move |b| (a, b)))
        .filter(|&(a, b)| match (inside(a), inside(b)) {
            (true, true) => true,
            (true, false) => piece.has_side(if b < MARGIN { Dir::North } else { Dir::South }),
            (false, true) => piece.has_side(if a < MARGIN { Dir::West } else { Dir::East }),
            (false, false) => false,
        })
        .collect()
}

fn in_crossing(maze: &Maze, id: PieceId) -> bool {
    maze.pieces_in(maze.piece(id).cell).len() == 2
}

fn level(maze: &Maze, id: PieceId) -> f64 {
    match maze.pieces_in(maze.piece(id).cell) {
        [_, top] if *top == id => HIGH,
        _ => LOW,
    }
}

fn side_level(maze: &Maze, id: PieceId, other: PieceId) -> f64 {
    let (mine, theirs) = (level(maze, id), level(maze, other));
    if in_crossing(maze, id) && in_crossing(maze, other) {
        (mine + theirs) / 2.0
    } else {
        mine.max(theirs)
    }
}

fn surface(maze: &Maze, id: PieceId, a: usize, b: usize) -> f64 {
    let place = |unit: usize| (unit as f64 + 0.5) / UNITS as f64 - 0.5;
    let (u, v) = (place(a), place(b));
    let centre = level(maze, id);
    let flat = if in_crossing(maze, id) { HALF } else { 0.0 };
    maze.piece(id)
        .links
        .iter()
        .map(|&(dir, other)| {
            let (dx, dy) = toward(dir);
            (u * dx + v * dy, other)
        })
        .max_by(|x, y| x.0.total_cmp(&y.0))
        .map_or(centre, |(progress, other)| {
            let share = ((progress - flat) / (0.5 - flat)).clamp(0.0, 1.0);
            centre + (side_level(maze, id, other) - centre) * share
        })
}

fn toward(dir: Dir) -> (f64, f64) {
    match dir {
        Dir::North => (0.0, -1.0),
        Dir::East => (1.0, 0.0),
        Dir::South => (0.0, 1.0),
        Dir::West => (-1.0, 0.0),
    }
}

fn mesh(columns: &Columns) -> Vec<[Point; 3]> {
    let size = CELL / UNITS as f64;
    let rows = columns.height as isize;
    let corner = |x: isize, y: isize, z: f64| [x as f64 * size, (rows - y) as f64 * size, z];
    let mut levels: HashMap<(isize, isize), Vec<f64>> = HashMap::new();
    for y in 0..=rows {
        for x in 0..=columns.width as isize {
            let mut found: Vec<f64> = [(x - 1, y - 1), (x, y - 1), (x - 1, y), (x, y)]
                .into_iter()
                .flat_map(|(cx, cy)| columns.get(cx, cy).iter().flat_map(|&(a, b)| [a, b]))
                .collect();
            found.sort_by(f64::total_cmp);
            found.dedup();
            levels.insert((x, y), found);
        }
    }
    let mut triangles = Vec::new();
    for y in 0..rows {
        for x in 0..columns.width as isize {
            for &(from, to) in columns.get(x, y) {
                for (z, up) in [(to, 1.0), (from, -1.0)] {
                    let [a, b, c, d] = [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
                        .map(|(cx, cy)| corner(cx, cy, z));
                    let normal = [0.0, 0.0, up];
                    push(&mut triangles, normal, [a, b, c]);
                    push(&mut triangles, normal, [a, c, d]);
                }
            }
            let sides = [
                ((0, -1), (x, y), (x + 1, y)),
                ((1, 0), (x + 1, y), (x + 1, y + 1)),
                ((0, 1), (x + 1, y + 1), (x, y + 1)),
                ((-1, 0), (x, y + 1), (x, y)),
            ];
            for ((dx, dy), p, q) in sides {
                let normal = [dx as f64, -dy as f64, 0.0];
                for (from, to) in without(columns.get(x, y), columns.get(x + dx, y + dy)) {
                    let chain = |line: (isize, isize)| -> Vec<Point> {
                        levels[&line]
                            .iter()
                            .filter(|&&z| from <= z && z <= to)
                            .map(|&z| corner(line.0, line.1, z))
                            .collect()
                    };
                    zip(&mut triangles, normal, &chain(p), &chain(q));
                }
            }
        }
    }
    triangles
}

fn without(spans: &[Span], taken: &[Span]) -> Vec<Span> {
    let mut left = Vec::new();
    for &(from, to) in spans {
        let mut at = from;
        for &(start, end) in taken {
            if end <= at || start >= to {
                continue;
            }
            if start > at {
                left.push((at, start));
            }
            at = at.max(end);
        }
        if at < to {
            left.push((at, to));
        }
    }
    left
}

fn zip(triangles: &mut Vec<[Point; 3]>, normal: Point, one: &[Point], other: &[Point]) {
    let (mut i, mut j) = (0, 0);
    while i + 1 < one.len() || j + 1 < other.len() {
        if j + 1 == other.len() || (i + 1 < one.len() && one[i + 1][2] <= other[j + 1][2]) {
            push(triangles, normal, [one[i], other[j], one[i + 1]]);
            i += 1;
        } else {
            push(triangles, normal, [one[i], other[j], other[j + 1]]);
            j += 1;
        }
    }
}

fn push(triangles: &mut Vec<[Point; 3]>, normal: Point, [a, b, c]: [Point; 3]) {
    let facing = cross(minus(b, a), minus(c, a));
    if dot(facing, normal) < 0.0 {
        triangles.push([a, c, b]);
    } else {
        triangles.push([a, b, c]);
    }
}

fn minus(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: Point) -> Point {
    let length = dot(a, a).sqrt();
    if length == 0.0 {
        a
    } else {
        a.map(|value| value / length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::{TRIVIAL, maze};

    #[test]
    fn every_edge_is_met_once_each_way() {
        let maze = maze(TRIVIAL, 3);
        let triangles = mesh(&columns(&maze));
        let key = |p: Point| p.map(|value| (value * 1e6).round() as i64);
        let mut edges: HashMap<([i64; 3], [i64; 3]), isize> = HashMap::new();
        for triangle in &triangles {
            for i in 0..3 {
                let (a, b) = (key(triangle[i]), key(triangle[(i + 1) % 3]));
                *edges.entry((a, b)).or_default() += 1;
                *edges.entry((b, a)).or_default() -= 1;
            }
        }
        assert!(edges.values().all(|&count| count == 0));
    }

    #[test]
    fn bridges_leave_room_underneath() {
        let maze = maze(TRIVIAL, 3);
        let columns = columns(&maze);
        let crossing = maze
            .grid
            .cells()
            .find(|&cell| maze.pieces_in(cell).len() == 2)
            .expect("a crossing");
        let spans = columns.get(
            (crossing.x * UNITS + UNITS / 2) as isize,
            (crossing.y * UNITS + UNITS / 2) as isize,
        );
        assert_eq!(
            spans,
            [(0.0, BASE + LOW), (BASE + HIGH - DECK, BASE + HIGH)]
        );
    }
}
