use crate::grid::{Cell, Dir};
use crate::maze::{Maze, PieceId};
use std::collections::{HashMap, HashSet};
use std::f64::consts::{FRAC_PI_2, TAU};

pub const CELL: f64 = 8.0;
const HALF: f64 = 0.375;
const INNER_ROUNDING: f64 = 0.1;
const OUTER_ROUNDING: f64 = 0.25;
const TURN: f64 = HALF + INNER_ROUNDING;
const FLOOR: f64 = 0.0;
const LOW: f64 = 2.0;
const CLEARANCE: f64 = 1.8;
const DECK: f64 = 1.2;
const SLOPE: f64 = 0.75;
const SLOPE_GROWTH: f64 = 1.1;
const THINNEST: f64 = 1.6;
const SEARCH_STEPS: usize = 6;
const NARROWEST: f64 = 0.25;
const PIER: f64 = 0.05;
const ROUNDING: f64 = 0.8;
const TUCK: f64 = 0.01;
const MARK: f64 = 1.0;
const SINK: f64 = 2.0;
const DOT: f64 = 1.6;
const RING: (f64, f64) = (1.5, 2.5);
const STEPS: usize = 16;
const HALF_STEPS: usize = 8;
const TURN_STEPS: usize = 24;
const ARC_STEPS: usize = 4;
const CORNER_STEPS: usize = 6;
const AROUND: usize = 32;

type Point = [f64; 3];
type Triangle = [Point; 3];
type Flat = (f64, f64);

struct Station {
    at: Flat,
    along: Flat,
    half: f64,
    bottom: f64,
    top: f64,
}

pub struct Heights {
    pub of: Vec<f64>,
    pub deck: Vec<f64>,
    pub room: Vec<f64>,
    pub widths: Vec<f64>,
    pub slopes: HashMap<(PieceId, PieceId), f64>,
}

impl Heights {
    pub fn slope(&self, a: PieceId, b: PieceId) -> f64 {
        self.slopes
            .get(&(a.min(b), a.max(b)))
            .copied()
            .unwrap_or(SLOPE)
    }

    pub fn steepest(&self) -> f64 {
        self.slopes.values().copied().fold(SLOPE, f64::max)
    }
}

#[derive(Clone, Copy)]
enum Rule {
    Link(PieceId, PieceId),
    Over,
}

pub fn stl(maze: &Maze) -> Vec<u8> {
    let triangles: Vec<Triangle> = shells(maze).into_iter().flatten().collect();
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

pub fn heights(maze: &Maze) -> Heights {
    let full = CLEARANCE + DECK;
    let count = maze.pieces().len();
    let crossings: Vec<usize> = maze
        .grid
        .cells()
        .filter(|&cell| maze.pieces_in(cell).len() == 2)
        .map(|cell| maze.grid.index(cell))
        .collect();
    let mut widths = vec![HALF; maze.grid.width * maze.grid.height];
    let mut gaps = vec![THINNEST; widths.len()];
    for &index in &crossings {
        widths[index] = NARROWEST;
    }
    let mut slopes: HashMap<(PieceId, PieceId), f64> = HashMap::new();
    while let Err(cycle) = solve(count, &rules(maze, &widths, &gaps, &slopes)) {
        for rule in cycle {
            if let Rule::Link(a, b) = rule {
                *slopes.entry((a.min(b), a.max(b))).or_insert(SLOPE) *= SLOPE_GROWTH;
            }
        }
    }
    for &index in &crossings {
        let fits = |gap: f64| {
            let mut gaps = gaps.clone();
            gaps[index] = gap;
            solve(count, &rules(maze, &widths, &gaps, &slopes)).is_ok()
        };
        gaps[index] = widest(THINNEST, full, fits);
    }
    for &index in &crossings {
        let fits = |width: f64| {
            let mut widths = widths.clone();
            widths[index] = width;
            solve(count, &rules(maze, &widths, &gaps, &slopes)).is_ok()
        };
        widths[index] = widest(NARROWEST, HALF, fits);
    }
    let Ok(of) = solve(count, &rules(maze, &widths, &gaps, &slopes)) else {
        unreachable!("every width and gap was checked to fit")
    };
    let mut deck = vec![0.0; count];
    let mut room = vec![0.0; count];
    for cell in maze.grid.cells() {
        if let [lower, upper] = maze.pieces_in(cell) {
            let gap = (of[*upper] - of[*lower]).min(full);
            deck[*upper] = gap * DECK / full;
            room[*upper] = gap - deck[*upper];
        }
    }
    Heights {
        of,
        deck,
        room,
        widths,
        slopes,
    }
}

fn widest(least: f64, most: f64, fits: impl Fn(f64) -> bool) -> f64 {
    if fits(most) {
        return most;
    }
    let (mut fitting, mut failing) = (least, most);
    for _ in 0..SEARCH_STEPS {
        let middle = (fitting + failing) / 2.0;
        if fits(middle) {
            fitting = middle;
        } else {
            failing = middle;
        }
    }
    fitting
}

fn rules(
    maze: &Maze,
    widths: &[f64],
    gaps: &[f64],
    slopes: &HashMap<(PieceId, PieceId), f64>,
) -> Vec<(PieceId, PieceId, f64, Rule)> {
    let mut rules = Vec::new();
    for (id, piece) in maze.pieces().iter().enumerate() {
        for &(_, other) in &piece.links {
            let slope = slopes
                .get(&(id.min(other), id.max(other)))
                .copied()
                .unwrap_or(SLOPE);
            let fall = -slope * CELL * run(maze, widths, id, other);
            rules.push((id, other, fall, Rule::Link(id, other)));
        }
    }
    for cell in maze.grid.cells() {
        if let [lower, upper] = maze.pieces_in(cell) {
            rules.push((*lower, *upper, gaps[maze.grid.index(cell)], Rule::Over));
        }
    }
    rules
}

fn solve(count: usize, rules: &[(PieceId, PieceId, f64, Rule)]) -> Result<Vec<f64>, Vec<Rule>> {
    let mut of = vec![LOW; count];
    let mut came_by = vec![usize::MAX; count];
    let mut moved = 0;
    for _ in 0..=count {
        let mut changed = false;
        for (index, &(from, to, rise, _)) in rules.iter().enumerate() {
            if of[from] + rise > of[to] + 1e-9 {
                of[to] = of[from] + rise;
                came_by[to] = index;
                moved = to;
                changed = true;
            }
        }
        if !changed {
            return Ok(of);
        }
    }
    for _ in 0..count {
        moved = rules[came_by[moved]].0;
    }
    let mut cycle = Vec::new();
    let mut at = moved;
    loop {
        let (from, _, _, rule) = rules[came_by[at]];
        cycle.push(rule);
        at = from;
        if at == moved {
            return Err(cycle);
        }
    }
}

fn shells(maze: &Maze) -> Vec<Vec<Triangle>> {
    let heights = heights(maze);
    let rows = maze.grid.height as f64;
    let place = |(x, y): Flat| (x * CELL, (rows - y) * CELL);
    let mut shells = Vec::new();
    for path in paths(maze, &heights) {
        let stations: Vec<Station> = path
            .into_iter()
            .map(|station| Station {
                at: place(station.at),
                along: (station.along.0, -station.along.1),
                ..station
            })
            .collect();
        shells.push(sweep(&stations));
    }
    for (id, piece) in maze.pieces().iter().enumerate() {
        let centre = middle(piece.cell);
        let outline: Option<Vec<Flat>> = match piece.links.as_slice() {
            [(a, _), (b, _)] if *a != b.opposite() => Some(corner(centre, *a, *b)),
            [_, _] => None,
            _ => Some(
                [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .map(|(x, y)| add(centre, (x * HALF, y * HALF)))
                    .to_vec(),
            ),
        };
        if let Some(outline) = outline {
            let outline: Vec<Flat> = outline.into_iter().map(place).collect();
            shells.push(prism(&outline, FLOOR, heights.of[id] - TUCK));
        }
    }
    for (cell, (inner, outer)) in [(maze.start, (0.0, DOT)), (maze.finish, RING)] {
        let top = heights.of[maze.pieces_in(cell)[0]] + MARK;
        let bottom = top - MARK - SINK;
        let mark = [(inner, bottom), (outer, bottom), (outer, top), (inner, top)];
        shells.push(turned(place(middle(cell)), &mark));
    }
    shells.iter_mut().for_each(|shell| face_out(shell));
    shells
}

fn corner(centre: Flat, a: Dir, b: Dir) -> Vec<Flat> {
    let (a, b) = (offset(a), offset(b));
    let at = |x: Flat| add(centre, x);
    let inner = scale(add(a, b), HALF);
    let outer = scale(inner, -1.0);
    let round = add(outer, scale(add(a, b), OUTER_ROUNDING));
    let mut outline = vec![at(add(scale(a, HALF), scale(b, -HALF)))];
    for step in 0..=CORNER_STEPS {
        let angle = FRAC_PI_2 * step as f64 / CORNER_STEPS as f64;
        let (cos, sin) = (angle.cos(), angle.sin());
        outline.push(at(add(
            round,
            add(
                scale(b, -OUTER_ROUNDING * cos),
                scale(a, -OUTER_ROUNDING * sin),
            ),
        )));
    }
    outline.push(at(add(scale(b, HALF), scale(a, -HALF))));
    outline.push(at(add(inner, scale(a, -INNER_ROUNDING))));
    outline.push(at(add(inner, scale(b, -INNER_ROUNDING))));
    outline
}

fn paths(maze: &Maze, heights: &Heights) -> Vec<Vec<Station>> {
    let is_junction = |id: PieceId| maze.piece(id).links.len() > 2;
    let mut done: HashSet<(PieceId, PieceId)> = HashSet::new();
    let mut paths = Vec::new();
    for (first, piece) in maze.pieces().iter().enumerate() {
        if !is_junction(first) {
            continue;
        }
        for &(_, second) in &piece.links {
            if done.contains(&(first, second)) {
                continue;
            }
            let mut chain = vec![first, second];
            while !is_junction(chain[chain.len() - 1]) {
                let (before, here) = (chain[chain.len() - 2], chain[chain.len() - 1]);
                let next = maze
                    .piece(here)
                    .links
                    .iter()
                    .map(|&(_, next)| next)
                    .find(|&next| next != before)
                    .expect("a path piece has two links");
                chain.push(next);
            }
            done.insert((first, second));
            done.insert((chain[chain.len() - 1], chain[chain.len() - 2]));
            paths.push(stations(maze, heights, &chain));
        }
    }
    paths
}

fn stations(maze: &Maze, heights: &Heights, chain: &[PieceId]) -> Vec<Station> {
    let cell = |id: PieceId| maze.piece(id).cell;
    let towards = |from: PieceId, to: PieceId| {
        offset(Dir::between(cell(from), cell(to)).expect("linked pieces are neighbours"))
    };
    let station = |id: PieceId, neighbour: PieceId, s: f64, at: Flat, along: Flat| {
        let top = height(maze, heights, id, neighbour, s);
        Station {
            at,
            along,
            half: width(maze, heights, id, neighbour, s) * CELL,
            bottom: underside(maze, heights, id, s).min(top - heights.deck[id].max(DECK)),
            top,
        }
    };
    let mut stations = Vec::new();
    let (first, second) = (chain[0], chain[1]);
    let out = towards(first, second);
    for step in 0..=HALF_STEPS {
        let s = 0.5 * step as f64 / HALF_STEPS as f64;
        let at = add(middle(cell(first)), scale(out, s));
        stations.push(station(first, second, s, at, out));
    }
    for window in chain.windows(3) {
        let [before, here, after] = [window[0], window[1], window[2]];
        let (a, b) = (towards(here, before), towards(here, after));
        let centre = middle(cell(here));
        let straight = a.0 == -b.0 && a.1 == -b.1;
        let steps = if straight { STEPS } else { TURN_STEPS };
        let lead = 0.5 - TURN;
        let bend = TURN * FRAC_PI_2;
        let length = if straight { 1.0 } else { 2.0 * lead + bend };
        for step in 1..=steps {
            let gone = length * step as f64 / steps as f64;
            let (at, along) = if straight {
                (add(centre, scale(b, gone - 0.5)), b)
            } else if gone < lead {
                (add(centre, scale(a, 0.5 - gone)), scale(a, -1.0))
            } else if gone > lead + bend {
                (add(centre, scale(b, TURN + gone - lead - bend)), b)
            } else {
                let angle = (gone - lead) / TURN;
                let (cos, sin) = (angle.cos(), angle.sin());
                let pivot = add(centre, scale(add(a, b), TURN));
                (
                    add(pivot, add(scale(b, -TURN * cos), scale(a, -TURN * sin))),
                    add(scale(b, sin), scale(a, -cos)),
                )
            };
            let s = gone / length - 0.5;
            let neighbour = if s < 0.0 { before } else { after };
            stations.push(station(here, neighbour, s.abs(), at, along));
        }
    }
    let (last, before) = (chain[chain.len() - 1], chain[chain.len() - 2]);
    let back = towards(last, before);
    for step in 1..=HALF_STEPS {
        let s = 0.5 - 0.5 * step as f64 / HALF_STEPS as f64;
        let at = add(middle(cell(last)), scale(back, s));
        stations.push(station(last, before, s, at, scale(back, -1.0)));
    }
    stations
}

fn flat(maze: &Maze, widths: &[f64], id: PieceId) -> f64 {
    let cell = maze.piece(id).cell;
    if maze.pieces_in(cell).len() == 2 {
        widths[maze.grid.index(cell)]
    } else if maze.piece(id).is_straight() {
        0.0
    } else {
        HALF
    }
}

fn half(maze: &Maze, widths: &[f64], id: PieceId) -> f64 {
    let cell = maze.piece(id).cell;
    if maze.pieces_in(cell).len() == 2 {
        widths[maze.grid.index(cell)]
    } else {
        HALF
    }
}

fn run(maze: &Maze, widths: &[f64], a: PieceId, b: PieceId) -> f64 {
    1.0 - flat(maze, widths, a) - flat(maze, widths, b)
}

fn tangent(maze: &Maze, heights: &Heights, id: PieceId, toward: PieceId) -> f64 {
    let widths = &heights.widths;
    if flat(maze, widths, id) > 0.0 {
        return 0.0;
    }
    let Some(&(_, away)) = maze
        .piece(id)
        .links
        .iter()
        .find(|&&(_, other)| other != toward)
    else {
        return 0.0;
    };
    let of = &heights.of;
    let ahead = (of[toward] - of[id]) / run(maze, widths, id, toward);
    let behind = (of[id] - of[away]) / run(maze, widths, away, id);
    if ahead * behind > 0.0 {
        ahead.signum() * ahead.abs().min(behind.abs())
    } else {
        0.0
    }
}

fn share(maze: &Maze, heights: &Heights, id: PieceId, toward: PieceId, s: f64) -> Option<f64> {
    let (near, far) = (
        flat(maze, &heights.widths, id),
        flat(maze, &heights.widths, toward),
    );
    if s <= near {
        None
    } else if s >= 1.0 - far {
        Some(1.0)
    } else {
        Some((s - near) / (1.0 - near - far))
    }
}

fn height(maze: &Maze, heights: &Heights, id: PieceId, toward: PieceId, s: f64) -> f64 {
    let of = &heights.of;
    let Some(x) = share(maze, heights, id, toward, s) else {
        return of[id];
    };
    let span = run(maze, &heights.widths, id, toward);
    let start = tangent(maze, heights, id, toward) * span;
    let end = -tangent(maze, heights, toward, id) * span;
    let (x2, x3) = (x * x, x * x * x);
    of[id] * (2.0 * x3 - 3.0 * x2 + 1.0)
        + start * (x3 - 2.0 * x2 + x)
        + of[toward] * (3.0 * x2 - 2.0 * x3)
        + end * (x3 - x2)
}

fn width(maze: &Maze, heights: &Heights, id: PieceId, toward: PieceId, s: f64) -> f64 {
    let (near, far) = (
        half(maze, &heights.widths, id),
        half(maze, &heights.widths, toward),
    );
    let x = ((s - HALF) / (1.0 - 2.0 * HALF)).clamp(0.0, 1.0);
    near + (far - near) * x * x * (3.0 - 2.0 * x)
}

fn underside(maze: &Maze, heights: &Heights, id: PieceId, s: f64) -> f64 {
    if !matches!(maze.pieces_in(maze.piece(id).cell), [_, top] if *top == id) {
        return FLOOR;
    }
    let flat = flat(maze, &heights.widths, id);
    let arch = ((s - flat) / (0.5 - PIER - flat)).clamp(0.0, 1.0);
    FLOOR + (heights.of[id] - heights.deck[id] - FLOOR) * (1.0 - arch * arch).sqrt()
}

fn section(station: &Station) -> Vec<Flat> {
    let half = station.half;
    let r = ROUNDING.min(station.top - station.bottom);
    let mut right = vec![(half, station.bottom)];
    for step in 0..=ARC_STEPS {
        let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
        right.push((
            half - r + r * angle.cos(),
            station.top - r + r * angle.sin(),
        ));
    }
    let mut points = right.clone();
    points.extend(right.iter().rev().map(|&(l, z)| (-l, z)));
    points
}

fn sweep(stations: &[Station]) -> Vec<Triangle> {
    let rings: Vec<Vec<Point>> = stations
        .iter()
        .map(|station| {
            let (x, y) = station.at;
            let (tx, ty) = station.along;
            let length = (tx * tx + ty * ty).sqrt();
            let (nx, ny) = (-ty / length, tx / length);
            section(station)
                .into_iter()
                .map(|(l, z)| [x + nx * l, y + ny * l, z])
                .collect()
        })
        .collect();
    let mut triangles = Vec::new();
    for pair in rings.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        for i in 0..a.len() {
            let j = (i + 1) % a.len();
            triangles.push([a[i], a[j], b[j]]);
            triangles.push([a[i], b[j], b[i]]);
        }
    }
    let (first, last) = (&rings[0], &rings[rings.len() - 1]);
    for i in 1..first.len() - 1 {
        triangles.push([first[0], first[i + 1], first[i]]);
        triangles.push([last[0], last[i], last[i + 1]]);
    }
    triangles
}

fn prism(outline: &[Flat], bottom: f64, top: f64) -> Vec<Triangle> {
    let r = ROUNDING.min(top - bottom);
    let mut layers: Vec<(f64, f64)> = vec![(0.0, bottom)];
    for step in 0..=ARC_STEPS {
        let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
        layers.push((r - r * angle.cos(), top - r + r * angle.sin()));
    }
    let rings: Vec<Vec<Point>> = layers
        .iter()
        .map(|&(by, z)| {
            inset(outline, by)
                .into_iter()
                .map(|(x, y)| [x, y, z])
                .collect()
        })
        .collect();
    let mut triangles = Vec::new();
    for pair in rings.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        for i in 0..a.len() {
            let j = (i + 1) % a.len();
            triangles.push([a[i], a[j], b[j]]);
            triangles.push([a[i], b[j], b[i]]);
        }
    }
    let (first, last) = (&rings[0], &rings[rings.len() - 1]);
    for i in 1..first.len() - 1 {
        triangles.push([first[0], first[i + 1], first[i]]);
        triangles.push([last[0], last[i], last[i + 1]]);
    }
    triangles
}

fn inset(outline: &[Flat], by: f64) -> Vec<Flat> {
    let count = outline.len();
    let area: f64 = (0..count)
        .map(|i| {
            let (a, b) = (outline[i], outline[(i + 1) % count]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    let side = area.signum();
    let normal = |from: Flat, to: Flat| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        (-dy / length * side, dx / length * side)
    };
    (0..count)
        .map(|i| {
            let (before, here, after) = (
                outline[(i + count - 1) % count],
                outline[i],
                outline[(i + 1) % count],
            );
            let (one, two) = (normal(before, here), normal(here, after));
            let mitre = by / (1.0 + one.0 * two.0 + one.1 * two.1);
            add(here, scale(add(one, two), mitre))
        })
        .collect()
}

fn turned((x, y): Flat, profile: &[Flat]) -> Vec<Triangle> {
    let at = |(r, z): Flat, step: usize| {
        let angle = TAU * step as f64 / AROUND as f64;
        [x + r * angle.cos(), y + r * angle.sin(), z]
    };
    let mut triangles = Vec::new();
    for i in 0..profile.len() {
        let (p, q) = (profile[i], profile[(i + 1) % profile.len()]);
        if p.0 == 0.0 && q.0 == 0.0 {
            continue;
        }
        for step in 0..AROUND {
            let (a, b, c, d) = (at(p, step), at(q, step), at(q, step + 1), at(p, step + 1));
            if p.0 != 0.0 {
                triangles.push([a, b, d]);
            }
            if q.0 != 0.0 {
                triangles.push([b, c, d]);
            }
        }
    }
    triangles
}

fn face_out(shell: &mut [Triangle]) {
    let volume: f64 = shell.iter().map(|[a, b, c]| dot(*a, cross(*b, *c))).sum();
    if volume < 0.0 {
        shell.iter_mut().for_each(|triangle| triangle.swap(1, 2));
    }
}

fn middle(cell: Cell) -> Flat {
    (cell.x as f64 + 0.5, cell.y as f64 + 0.5)
}

fn offset(dir: Dir) -> Flat {
    match dir {
        Dir::North => (0.0, -1.0),
        Dir::East => (1.0, 0.0),
        Dir::South => (0.0, 1.0),
        Dir::West => (-1.0, 0.0),
    }
}

fn add(a: Flat, b: Flat) -> Flat {
    (a.0 + b.0, a.1 + b.1)
}

fn scale(a: Flat, by: f64) -> Flat {
    (a.0 * by, a.1 * by)
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
    use crate::book::{EASY, TRIVIAL, maze};

    #[test]
    fn every_shell_is_closed_and_faces_out() {
        let maze = maze(TRIVIAL, 3);
        for shell in shells(&maze) {
            let key = |p: Point| p.map(|value| (value * 1e6).round() as i64);
            let mut edges: HashMap<([i64; 3], [i64; 3]), isize> = HashMap::new();
            for triangle in &shell {
                for i in 0..3 {
                    let (a, b) = (key(triangle[i]), key(triangle[(i + 1) % 3]));
                    *edges.entry((a, b)).or_default() += 1;
                    *edges.entry((b, a)).or_default() -= 1;
                }
            }
            assert!(edges.values().all(|&count| count == 0));
            let volume: f64 = shell.iter().map(|[a, b, c]| dot(*a, cross(*b, *c))).sum();
            assert!(volume > 0.0);
        }
    }

    #[test]
    fn bridges_clear_and_ramps_keep_to_the_slope() {
        for seed in 0..4 {
            let maze = maze(EASY, seed);
            let heights = heights(&maze);
            for cell in maze.grid.cells() {
                if let [lower, upper] = maze.pieces_in(cell) {
                    let room = heights.of[*upper] - heights.deck[*upper] - heights.of[*lower];
                    assert!(room >= heights.room[*upper] - 1e-9);
                    assert!(heights.room[*upper] + heights.deck[*upper] >= THINNEST - 1e-9);
                }
            }
            for (id, piece) in maze.pieces().iter().enumerate() {
                for &(_, other) in &piece.links {
                    let rise = (heights.of[id] - heights.of[other]).abs();
                    assert!(
                        rise <= heights.slope(id, other)
                            * CELL
                            * run(&maze, &heights.widths, id, other)
                            + 1e-9
                    );
                }
            }
        }
    }
}
