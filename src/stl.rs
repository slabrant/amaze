use crate::grid::{Cell, Dir};
use crate::maze::{Maze, PieceId};
use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_2, TAU};

pub const CELL: f64 = 8.0;
const HALF: f64 = 0.375;
const BASE: f64 = 1.5;
const FLOOR: f64 = BASE / 2.0;
const LOW: f64 = 2.0;
const CLEARANCE: f64 = 1.8;
const DECK: f64 = 1.2;
const HIGH: f64 = LOW + CLEARANCE + DECK;
const PIER: f64 = 0.05;
const ROUNDING: f64 = 0.8;
const FRAME_ROUNDING: f64 = 3.0;
const MARK: f64 = 1.0;
const SINK: f64 = 2.0;
const DOT: f64 = 1.6;
const RING: (f64, f64) = (1.5, 2.5);
const STEPS: usize = 16;
const HALF_STEPS: usize = 8;
const ARC_STEPS: usize = 4;
const AROUND: usize = 32;

type Point = [f64; 3];
type Triangle = [Point; 3];
type Flat = (f64, f64);

struct Station {
    at: Flat,
    along: Flat,
    bottom: f64,
    top: f64,
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

fn shells(maze: &Maze) -> Vec<Vec<Triangle>> {
    let place = |(x, y): Flat| (x * CELL, (maze.grid.height as f64 - y) * CELL);
    let mut shells = vec![plate(maze)];
    for path in paths(maze) {
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
    for cell in maze.grid.cells() {
        if let [id] = maze.pieces_in(cell)
            && maze.piece(*id).links.len() > 2
        {
            let profile = profile(0.0, HALF * CELL, FLOOR, BASE + LOW);
            shells.push(turned(place(middle(cell)), &profile));
        }
    }
    for (cell, (inner, outer)) in [(maze.start, (0.0, DOT)), (maze.finish, RING)] {
        let top = BASE + middle_level(maze, cell) + MARK;
        let bottom = top - MARK - SINK;
        let mark = [(inner, bottom), (outer, bottom), (outer, top), (inner, top)];
        shells.push(turned(place(middle(cell)), &mark));
    }
    shells.iter_mut().for_each(|shell| face_out(shell));
    shells
}

fn plate(maze: &Maze) -> Vec<Triangle> {
    let (width, height) = (
        maze.grid.width as f64 * CELL,
        maze.grid.height as f64 * CELL,
    );
    let r = FRAME_ROUNDING;
    let centres = [
        (width - r, r),
        (width - r, height - r),
        (r, height - r),
        (r, r),
    ];
    let outline: Vec<Flat> = centres
        .iter()
        .enumerate()
        .flat_map(|(i, &(cx, cy))| {
            (0..=AROUND / 4).map(move |step| {
                let angle =
                    (i as f64 - 1.0) * FRAC_PI_2 + FRAC_PI_2 * step as f64 / (AROUND / 4) as f64;
                (cx + r * angle.cos(), cy + r * angle.sin())
            })
        })
        .collect();
    let mut triangles = Vec::new();
    let point = |(x, y): Flat, z: f64| [x, y, z];
    for i in 1..outline.len() - 1 {
        triangles.push([
            point(outline[0], BASE),
            point(outline[i], BASE),
            point(outline[i + 1], BASE),
        ]);
        triangles.push([
            point(outline[0], 0.0),
            point(outline[i + 1], 0.0),
            point(outline[i], 0.0),
        ]);
    }
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
        triangles.push([point(a, 0.0), point(b, 0.0), point(b, BASE)]);
        triangles.push([point(a, 0.0), point(b, BASE), point(a, BASE)]);
    }
    triangles
}

fn paths(maze: &Maze) -> Vec<Vec<Station>> {
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
            paths.push(stations(maze, &chain));
        }
    }
    paths
}

fn stations(maze: &Maze, chain: &[PieceId]) -> Vec<Station> {
    let cell = |id: PieceId| maze.piece(id).cell;
    let towards = |from: PieceId, to: PieceId| {
        offset(Dir::between(cell(from), cell(to)).expect("linked pieces are neighbours"))
    };
    let mut stations = Vec::new();
    let (first, second) = (chain[0], chain[1]);
    let out = towards(first, second);
    for step in 0..=HALF_STEPS {
        let s = 0.5 * step as f64 / HALF_STEPS as f64;
        stations.push(station(
            maze,
            first,
            second,
            s,
            add(middle(cell(first)), scale(out, s)),
            out,
        ));
    }
    for window in chain.windows(3) {
        let [before, here, after] = [window[0], window[1], window[2]];
        let (a, b) = (towards(here, before), towards(here, after));
        let centre = middle(cell(here));
        let straight = a.0 == -b.0 && a.1 == -b.1;
        for step in 1..=STEPS {
            let t = step as f64 / STEPS as f64;
            let (at, along) = if straight {
                (add(centre, scale(b, t - 0.5)), b)
            } else {
                let corner = add(centre, scale(add(a, b), 0.5));
                let angle = t * FRAC_PI_2;
                let (cos, sin) = (angle.cos(), angle.sin());
                (
                    add(corner, scale(add(scale(b, -cos), scale(a, -sin)), 0.5)),
                    add(scale(b, sin), scale(a, -cos)),
                )
            };
            let s = t - 0.5;
            let neighbour = if s < 0.0 { before } else { after };
            let mut made = station(maze, here, neighbour, s.abs(), at, along);
            if !in_crossing(maze, here) {
                let (from, to) = (
                    side_level(maze, here, before),
                    side_level(maze, here, after),
                );
                made.top = BASE + from + (to - from) * eased(t);
            }
            stations.push(made);
        }
    }
    let (last, before) = (chain[chain.len() - 1], chain[chain.len() - 2]);
    let back = towards(last, before);
    for step in 1..=HALF_STEPS {
        let s = 0.5 - 0.5 * step as f64 / HALF_STEPS as f64;
        let along = scale(back, -1.0);
        stations.push(station(
            maze,
            last,
            before,
            s,
            add(middle(cell(last)), scale(back, s)),
            along,
        ));
    }
    stations
}

fn station(maze: &Maze, id: PieceId, neighbour: PieceId, s: f64, at: Flat, along: Flat) -> Station {
    let top = BASE + surface(maze, id, neighbour, s);
    Station {
        at,
        along,
        bottom: underside(maze, id, s).min(top - DECK),
        top,
    }
}

fn middle_level(maze: &Maze, cell: Cell) -> f64 {
    let id = maze.pieces_in(cell)[0];
    match maze.piece(id).links.as_slice() {
        [(_, a), (_, b)] => (side_level(maze, id, *a) + side_level(maze, id, *b)) / 2.0,
        _ => level(maze, id),
    }
}

fn in_crossing(maze: &Maze, id: PieceId) -> bool {
    maze.pieces_in(maze.piece(id).cell).len() == 2
}

fn is_bridge(maze: &Maze, id: PieceId) -> bool {
    matches!(maze.pieces_in(maze.piece(id).cell), [_, top] if *top == id)
}

fn level(maze: &Maze, id: PieceId) -> f64 {
    if is_bridge(maze, id) { HIGH } else { LOW }
}

fn side_level(maze: &Maze, id: PieceId, other: PieceId) -> f64 {
    let (mine, theirs) = (level(maze, id), level(maze, other));
    if in_crossing(maze, id) && in_crossing(maze, other) {
        (mine + theirs) / 2.0
    } else {
        mine.max(theirs)
    }
}

fn surface(maze: &Maze, id: PieceId, neighbour: PieceId, s: f64) -> f64 {
    let centre = level(maze, id);
    let flat = if in_crossing(maze, id) { HALF } else { 0.0 };
    let share = ((s - flat) / (0.5 - flat)).clamp(0.0, 1.0);
    centre + (side_level(maze, id, neighbour) - centre) * eased(share)
}

fn eased(share: f64) -> f64 {
    share * share * (3.0 - 2.0 * share)
}

fn underside(maze: &Maze, id: PieceId, s: f64) -> f64 {
    if !is_bridge(maze, id) {
        return FLOOR;
    }
    let arch = ((s - HALF) / (0.5 - PIER - HALF)).clamp(0.0, 1.0);
    FLOOR + (BASE + HIGH - DECK - FLOOR) * (1.0 - arch * arch).sqrt()
}

fn profile(inner: f64, outer: f64, bottom: f64, top: f64) -> Vec<Flat> {
    let mut points = vec![(inner, bottom), (outer, bottom)];
    let r = ROUNDING.min(top - bottom).min(outer - inner);
    for step in 0..=ARC_STEPS {
        let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
        points.push((outer - r + r * angle.cos(), top - r + r * angle.sin()));
    }
    points.push((inner, top));
    points
}

fn section(station: &Station) -> Vec<Flat> {
    let half = HALF * CELL;
    let right = profile(0.0, half, station.bottom, station.top);
    let mut points: Vec<Flat> = right[1..right.len() - 1].to_vec();
    points.extend(
        right[1..right.len() - 1]
            .iter()
            .rev()
            .map(|&(l, z)| (-l, z)),
    );
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
    for (ring, flip) in [(&rings[0], true), (&rings[rings.len() - 1], false)] {
        for i in 1..ring.len() - 1 {
            if flip {
                triangles.push([ring[0], ring[i + 1], ring[i]]);
            } else {
                triangles.push([ring[0], ring[i], ring[i + 1]]);
            }
        }
    }
    triangles
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
    use crate::book::{TRIVIAL, maze};
    use std::collections::HashMap;

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
    fn bridges_leave_room_underneath() {
        let maze = maze(TRIVIAL, 3);
        let [lower, upper] = *maze
            .grid
            .cells()
            .map(|cell| maze.pieces_in(cell))
            .find(|ids| ids.len() == 2)
            .expect("a crossing")
        else {
            unreachable!()
        };
        let other = maze.piece(lower).links[0].1;
        for s in [0.0, HALF / 2.0, HALF] {
            let room =
                underside(&maze, upper, s) - (BASE + surface(&maze, lower, other, s.min(HALF)));
            assert!(room >= CLEARANCE - 1e-9);
        }
    }
}
