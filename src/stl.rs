use crate::grid::{Cell, Dir};
use crate::maze::{Maze, PieceId};
use manifold_csg::Manifold;
use std::collections::{HashMap, HashSet};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

pub const CELL: f64 = 8.0;
const HALF: f64 = 0.25;
const INNER_ROUNDING: f64 = 0.1;
const OUTER_ROUNDING: f64 = 0.2;
const TURN: f64 = HALF + INNER_ROUNDING;
const GROUND: f64 = 2.0;
const ROOM: f64 = 2.4;
const DECK: f64 = 1.2;
const STEP: f64 = ROOM + DECK;
const RAIL: f64 = 1.2;
const RAIL_WIDTH: f64 = 0.8;
const RAIL_FOOT: f64 = 1.0;
const RAIL_END: f64 = 1.0;
const MARK: f64 = 1.0;
const SINK: f64 = 2.0;
const DOT: f64 = 0.9;
const RING: (f64, f64) = (0.6, 1.1);
const STEPS: usize = 16;
const HALF_STEPS: usize = 8;
const EDGE_STEPS: usize = 8;
const ARC_STEPS: usize = 8;
const RAIL_STEPS: usize = 6;
const AROUND: usize = 32;
const NEAR: f64 = 1e-3;
const ROOMIEST: f64 = 0.5;

type Point = [f64; 3];
type Post = [f64; 4];
type Triangle = [Point; 3];
type Flat = (f64, f64);
type Rule = (usize, usize, i32, Option<(PieceId, PieceId)>);

struct Station {
    at: Flat,
    along: Flat,
    half: f64,
    bottom: f64,
    top: f64,
}

pub struct Levels {
    pub of: Vec<i32>,
    pub jumps: HashMap<(PieceId, PieceId), i32>,
}

impl Levels {
    pub fn jump(&self, a: PieceId, b: PieceId) -> i32 {
        self.jumps.get(&(a.min(b), a.max(b))).copied().unwrap_or(0)
    }
}

pub fn stl(maze: &Maze) -> Vec<u8> {
    let (points, _, corners) = solid(maze).to_mesh_f64();
    let at = |index: u64| {
        let i = index as usize * 3;
        [points[i], points[i + 1], points[i + 2]]
    };
    let triangles: Vec<Triangle> = corners
        .chunks(3)
        .map(|corner| [at(corner[0]), at(corner[1]), at(corner[2])])
        .collect();
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

pub fn is_ramp(maze: &Maze, id: PieceId) -> bool {
    let piece = maze.piece(id);
    maze.pieces_in(piece.cell).len() == 1 && piece.is_straight()
}

fn bent(before: PieceId, after: PieceId, angle: f64) -> (PieceId, f64) {
    let (toward_before, toward_after) = (TURN * (1.0 - angle.sin()), TURN * (1.0 - angle.cos()));
    if toward_before >= toward_after {
        (before, toward_before)
    } else {
        (after, toward_after)
    }
}

fn flat(maze: &Maze, id: PieceId) -> f64 {
    if is_ramp(maze, id) { 0.0 } else { HALF }
}

pub fn room(maze: &Maze, a: PieceId, b: PieceId) -> f64 {
    1.0 - flat(maze, a) - flat(maze, b)
}

pub fn levels(maze: &Maze) -> Levels {
    let mut links: Vec<(PieceId, PieceId)> = Vec::new();
    for (id, piece) in maze.pieces().iter().enumerate() {
        for &(_, other) in &piece.links {
            if id < other {
                links.push((id, other));
            }
        }
    }
    let mut jumps: HashMap<(PieceId, PieceId), i32> = links
        .iter()
        .filter(|&&(a, b)| room(maze, a, b) >= ROOMIEST)
        .map(|&link| (link, 1))
        .collect();
    loop {
        let mut rules: Vec<Rule> = Vec::new();
        for &(a, b) in &links {
            let jump = jumps.get(&(a, b)).copied().unwrap_or(0);
            rules.push((a, b, -jump, Some((a, b))));
            rules.push((b, a, -jump, Some((a, b))));
        }
        for cell in maze.grid.cells() {
            if let [lower, upper] = maze.pieces_in(cell) {
                rules.push((*lower, *upper, 1, None));
            }
        }
        match solve(maze.pieces().len(), &rules) {
            Ok(of) => return Levels { of, jumps },
            Err(cycle) => {
                let candidates: Vec<(PieceId, PieceId)> = cycle.into_iter().flatten().collect();
                let best = |link: &&(PieceId, PieceId)| {
                    let (a, b) = **link;
                    let used = jumps.get(&(a, b)).copied().unwrap_or(0) as f64;
                    room(maze, a, b) / (used + 1.0)
                };
                let &link = candidates
                    .iter()
                    .max_by(|x, y| best(x).total_cmp(&best(y)))
                    .expect("a cycle that rises goes through a link");
                *jumps.entry(link).or_insert(0) += 1;
            }
        }
    }
}

fn solve(count: usize, rules: &[Rule]) -> Result<Vec<i32>, Vec<Option<(PieceId, PieceId)>>> {
    let mut values = vec![0; count];
    let mut came_by = vec![usize::MAX; count];
    let mut moved = 0;
    for _ in 0..=count {
        let mut changed = false;
        for (index, &(from, to, rise, _)) in rules.iter().enumerate() {
            if values[from] + rise > values[to] {
                values[to] = values[from] + rise;
                came_by[to] = index;
                moved = to;
                changed = true;
            }
        }
        if !changed {
            return Ok(values);
        }
    }
    for _ in 0..count {
        moved = rules[came_by[moved]].0;
    }
    let mut cycle = Vec::new();
    let mut at = moved;
    loop {
        let (from, _, _, link) = rules[came_by[at]];
        cycle.push(link);
        at = from;
        if at == moved {
            return Err(cycle);
        }
    }
}

fn solid(maze: &Maze) -> Manifold {
    let solids: Vec<Manifold> = shells(maze).iter().map(|shell| solid_of(shell)).collect();
    Manifold::batch_union(&solids)
}

fn solid_of(shell: &[Triangle]) -> Manifold {
    let mut index: HashMap<[u64; 3], u64> = HashMap::new();
    let mut points: Vec<f64> = Vec::new();
    let mut corners: Vec<u64> = Vec::new();
    for triangle in shell {
        for point in triangle {
            let key = point.map(f64::to_bits);
            let next = index.len() as u64;
            let at = *index.entry(key).or_insert_with(|| {
                points.extend(point);
                next
            });
            corners.push(at);
        }
    }
    Manifold::from_mesh_f64(&points, 3, &corners).expect("every shape is a closed solid")
}

fn shells(maze: &Maze) -> Vec<Vec<Triangle>> {
    let levels = levels(maze);
    let rows = maze.grid.height as f64;
    let place = |(x, y): Flat| (x * CELL, (rows - y) * CELL);
    let mut shells = Vec::new();
    for path in paths(maze, &levels) {
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
            let top = height(maze, &levels, id, piece.links[0].1, 0.0);
            shells.push(prism(&outline, 0.0, top));
        }
    }
    let edges: Vec<Vec<Post>> = edges(maze, &levels)
        .into_iter()
        .map(|edge| {
            edge.into_iter()
                .map(|(at, z, top)| {
                    let (x, y) = place(at);
                    [x, y, z, top]
                })
                .collect()
        })
        .collect();
    for (rail, closed) in joined(edges) {
        shells.push(ridge(&rail, closed));
    }
    for (cell, (inner, outer)) in [(maze.start, (0.0, DOT)), (maze.finish, RING)] {
        let id = maze.pieces_in(cell)[0];
        let top = height(maze, &levels, id, maze.piece(id).links[0].1, 0.0) + MARK;
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
    for step in 0..=ARC_STEPS {
        let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
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

fn paths(maze: &Maze, levels: &Levels) -> Vec<Vec<Station>> {
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
            paths.push(stations(maze, levels, &chain));
        }
    }
    paths
}

fn stations(maze: &Maze, levels: &Levels, chain: &[PieceId]) -> Vec<Station> {
    let cell = |id: PieceId| maze.piece(id).cell;
    let towards = |from: PieceId, to: PieceId| {
        offset(Dir::between(cell(from), cell(to)).expect("linked pieces are neighbours"))
    };
    let station = |id: PieceId, neighbour: PieceId, s: f64, at: Flat, along: Flat| {
        let top = height(maze, levels, id, neighbour, s);
        Station {
            at,
            along,
            half: HALF * CELL,
            bottom: if is_bridge(maze, id) { top - DECK } else { 0.0 },
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
        if a.0 == -b.0 && a.1 == -b.1 {
            for step in 1..=STEPS {
                let s = step as f64 / STEPS as f64 - 0.5;
                let neighbour = if s < 0.0 { before } else { after };
                stations.push(station(
                    here,
                    neighbour,
                    s.abs(),
                    add(centre, scale(b, s)),
                    b,
                ));
            }
            continue;
        }
        for step in 1..=EDGE_STEPS {
            let s = 0.5 - (0.5 - TURN) * step as f64 / EDGE_STEPS as f64;
            stations.push(station(
                here,
                before,
                s,
                add(centre, scale(a, s)),
                scale(a, -1.0),
            ));
        }
        let pivot = add(centre, scale(add(a, b), TURN));
        for step in 1..ARC_STEPS {
            let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
            let (cos, sin) = (angle.cos(), angle.sin());
            let at = add(pivot, add(scale(b, -TURN * cos), scale(a, -TURN * sin)));
            let (neighbour, s) = bent(before, after, angle);
            stations.push(station(
                here,
                neighbour,
                s,
                at,
                add(scale(b, sin), scale(a, -cos)),
            ));
        }
        for step in 0..=EDGE_STEPS {
            let s = TURN + (0.5 - TURN) * step as f64 / EDGE_STEPS as f64;
            stations.push(station(here, after, s, add(centre, scale(b, s)), b));
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

fn edges(maze: &Maze, levels: &Levels) -> Vec<Vec<(Flat, f64, f64)>> {
    let r = RAIL_WIDTH / 2.0 / CELL;
    let mut edges = Vec::new();
    for (id, piece) in maze.pieces().iter().enumerate() {
        let centre = middle(piece.cell);
        let neighbour = |dir: Dir| {
            piece
                .links
                .iter()
                .find(|&&(side, _)| side == dir)
                .map(|&(_, other)| other)
        };
        let deck = match maze.pieces_in(piece.cell) {
            [lower, upper] if *lower == id => Some(mm(levels.of[*upper]) - DECK / 2.0),
            _ => None,
        };
        let along = |dir: Dir, side: Flat, s: f64| -> (Flat, f64, f64) {
            let other = neighbour(dir).expect("an edge runs along a joined side");
            let lateral = HALF - r;
            let at = add(centre, add(scale(offset(dir), s), scale(side, lateral)));
            let z = height(maze, levels, id, other, s);
            let top = match deck {
                Some(deck) if s <= HALF => deck,
                _ => z + RAIL,
            };
            (at, z, top)
        };
        let spread = |from: f64, to: f64| -> Vec<f64> {
            (0..=EDGE_STEPS)
                .map(|step| from + (to - from) * step as f64 / EDGE_STEPS as f64)
                .collect()
        };
        let joined = |dir: Dir| piece.has_side(dir);
        for (i, &a) in Dir::ALL.iter().enumerate() {
            let b = Dir::ALL[(i + 1) % 4];
            if joined(a) && joined(b) {
                let (oa, ob) = (offset(a), offset(b));
                let mut edge: Vec<(Flat, f64, f64)> = spread(0.5, TURN)
                    .into_iter()
                    .map(|s| along(a, ob, s))
                    .collect();
                let pivot = add(centre, scale(add(oa, ob), TURN));
                let radius = TURN - (HALF - r);
                for step in 1..ARC_STEPS {
                    let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
                    let at = add(
                        pivot,
                        add(
                            scale(ob, -radius * angle.cos()),
                            scale(oa, -radius * angle.sin()),
                        ),
                    );
                    let (toward, s) = bent(
                        neighbour(a).expect("a joined side"),
                        neighbour(b).expect("a joined side"),
                        angle,
                    );
                    let z = height(maze, levels, id, toward, s);
                    edge.push((at, z, z + RAIL));
                }
                edge.extend(spread(TURN, 0.5).into_iter().map(|s| along(b, oa, s)));
                edges.push(edge);
            }
            if !joined(a) && !joined(b) {
                let (p, q) = (a.opposite(), b.opposite());
                let (op, oq) = (offset(p), offset(q));
                let level = height(maze, levels, id, neighbour(p).expect("a turn"), 0.0);
                let wall = |dir: Dir, side: Flat, u: f64| -> (Flat, f64, f64) {
                    if u >= 0.0 {
                        along(dir, side, u)
                    } else {
                        let at = add(centre, add(scale(offset(dir), u), scale(side, HALF - r)));
                        (at, level, level + RAIL)
                    }
                };
                let start = -(HALF - OUTER_ROUNDING);
                let mut edge: Vec<(Flat, f64, f64)> = spread(0.5, start)
                    .into_iter()
                    .map(|u| wall(p, scale(oq, -1.0), u))
                    .collect();
                let round = add(centre, scale(add(op, oq), start));
                let radius = OUTER_ROUNDING - r;
                for step in 1..ARC_STEPS {
                    let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
                    let at = add(
                        round,
                        add(
                            scale(oq, -radius * angle.cos()),
                            scale(op, -radius * angle.sin()),
                        ),
                    );
                    edge.push((at, level, level + RAIL));
                }
                edge.extend(
                    spread(start, 0.5)
                        .into_iter()
                        .map(|u| wall(q, scale(op, -1.0), u)),
                );
                edges.push(edge);
            }
        }
        for &d in &Dir::ALL {
            let (left, right) = (d.turned_left(), d.turned_right());
            if !joined(d) && joined(left) && joined(right) {
                let side = offset(d);
                let mut edge: Vec<(Flat, f64, f64)> = spread(0.5, 0.0)
                    .into_iter()
                    .map(|s| along(left, side, s))
                    .collect();
                edge.extend(
                    spread(0.0, 0.5)
                        .into_iter()
                        .skip(1)
                        .map(|s| along(right, side, s)),
                );
                edges.push(edge);
            }
        }
    }
    edges
}

fn joined(edges: Vec<Vec<Post>>) -> Vec<(Vec<Post>, bool)> {
    let key = |p: Post| ((p[0] / NEAR).round() as i64, (p[1] / NEAR).round() as i64);
    let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, edge) in edges.iter().enumerate() {
        ends.entry(key(edge[0])).or_default().push(i);
        ends.entry(key(edge[edge.len() - 1])).or_default().push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut rails = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut rail = edges[start].clone();
        for _ in 0..2 {
            loop {
                let tip = key(rail[rail.len() - 1]);
                let Some(&next) = ends[&tip].iter().find(|&&i| !used[i]) else {
                    break;
                };
                used[next] = true;
                let mut more = edges[next].clone();
                if key(more[0]) != tip {
                    more.reverse();
                }
                rail.extend(more.into_iter().skip(1));
            }
            rail.reverse();
        }
        let closed = rail.len() > 2 && key(rail[0]) == key(rail[rail.len() - 1]);
        if closed {
            rail.pop();
        }
        rail.dedup_by(|a, b| key(*a) == key(*b));
        rails.push((rail, closed));
    }
    rails
}

fn mm(level: i32) -> f64 {
    GROUND + level as f64 * STEP
}

fn height(maze: &Maze, levels: &Levels, id: PieceId, toward: PieceId, s: f64) -> f64 {
    let (own, theirs) = (levels.of[id], levels.of[toward]);
    let near = flat(maze, id);
    if own == theirs || s <= near {
        return mm(own);
    }
    let x = ((s - near) / room(maze, id, toward)).clamp(0.0, 1.0);
    mm(own) + (mm(theirs) - mm(own)) * eased(x)
}

fn eased(x: f64) -> f64 {
    x * x * (3.0 - 2.0 * x)
}

fn is_bridge(maze: &Maze, id: PieceId) -> bool {
    matches!(maze.pieces_in(maze.piece(id).cell), [_, top] if *top == id)
}

fn section(station: &Station) -> Vec<Flat> {
    let half = station.half;
    vec![
        (half, station.bottom),
        (half, station.top),
        (-half, station.top),
        (-half, station.bottom),
    ]
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
    loft(&rings, false)
}

fn ridge(points: &[Post], closed: bool) -> Vec<Triangle> {
    let r = RAIL_WIDTH / 2.0;
    let count = points.len();
    let mut gone = vec![0.0; count];
    for i in 1..count {
        let (a, b) = (points[i - 1], points[i]);
        gone[i] = gone[i - 1] + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    }
    let total = gone[count - 1];
    let rings: Vec<Vec<Point>> = (0..count)
        .map(|i| {
            let (before, after) = if closed {
                (points[(i + count - 1) % count], points[(i + 1) % count])
            } else {
                (points[i.saturating_sub(1)], points[(i + 1).min(count - 1)])
            };
            let (tx, ty) = (after[0] - before[0], after[1] - before[1]);
            let length = (tx * tx + ty * ty).sqrt();
            let (nx, ny) = (-ty / length, tx / length);
            let rise = if closed {
                1.0
            } else {
                eased((gone[i].min(total - gone[i]) / RAIL_END).clamp(0.0, 1.0))
            };
            let [x, y, z, top] = points[i];
            let tall = (top - z) * rise;
            let mut profile = vec![(r, -RAIL_FOOT)];
            for step in 0..=RAIL_STEPS {
                let angle = PI * step as f64 / RAIL_STEPS as f64;
                profile.push((r * angle.cos(), tall - r * rise + r * rise * angle.sin()));
            }
            profile.push((-r, -RAIL_FOOT));
            profile
                .into_iter()
                .map(|(l, up)| [x + nx * l, y + ny * l, z + up])
                .collect()
        })
        .collect();
    loft(&rings, closed)
}

fn loft(rings: &[Vec<Point>], closed: bool) -> Vec<Triangle> {
    let mut triangles = Vec::new();
    let count = rings.len();
    let pairs = if closed { count } else { count - 1 };
    for k in 0..pairs {
        let (a, b) = (&rings[k], &rings[(k + 1) % count]);
        for i in 0..a.len() {
            let j = (i + 1) % a.len();
            triangles.push([a[i], a[j], b[j]]);
            triangles.push([a[i], b[j], b[i]]);
        }
    }
    if !closed {
        let (first, last) = (&rings[0], &rings[count - 1]);
        for i in 1..first.len() - 1 {
            triangles.push([first[0], first[i + 1], first[i]]);
            triangles.push([last[0], last[i], last[i + 1]]);
        }
    }
    triangles
}

fn prism(outline: &[Flat], bottom: f64, top: f64) -> Vec<Triangle> {
    let rings: Vec<Vec<Point>> = [bottom, top]
        .iter()
        .map(|&z| outline.iter().map(|&(x, y)| [x, y, z]).collect())
        .collect();
    loft(&rings, false)
}

fn turned((x, y): Flat, profile: &[Flat]) -> Vec<Triangle> {
    let at = |(r, z): Flat, step: usize| {
        let angle = TAU * (step % AROUND) as f64 / AROUND as f64;
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
            let key = |p: Point| p.map(f64::to_bits);
            let mut edges: HashMap<([u64; 3], [u64; 3]), isize> = HashMap::new();
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
    fn the_print_is_one_solid() {
        for (grid, seed) in [(TRIVIAL, 3), (EASY, 7)] {
            let solid = solid(&maze(grid, seed));
            assert!(solid.status().is_ok());
            assert_eq!(solid.decompose().len(), 1);
        }
    }

    #[test]
    fn levels_keep_their_rules() {
        for seed in 0..4 {
            let maze = maze(EASY, seed);
            let levels = levels(&maze);
            for cell in maze.grid.cells() {
                if let [lower, upper] = maze.pieces_in(cell) {
                    assert!(levels.of[*upper] > levels.of[*lower]);
                }
            }
            for (id, piece) in maze.pieces().iter().enumerate() {
                for &(_, other) in &piece.links {
                    let jump = (levels.of[id] - levels.of[other]).abs();
                    assert!(jump <= levels.jump(id, other));
                }
            }
        }
    }
}
