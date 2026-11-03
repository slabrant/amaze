use crate::grid::{Cell, Dir};
use crate::maze::{Maze, PieceId};
use manifold_csg::{CrossSection, FillRule, JoinType, Manifold};
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;

pub const CELL: f64 = 8.0;
const HALF: f64 = 0.25;
const INNER_ROUNDING: f64 = 0.1;
const OUTER_ROUNDING: f64 = 0.2;
const GROUND: f64 = 2.0;
const ROOM: f64 = 2.4;
const DECK: f64 = 1.2;
const STEP: f64 = ROOM + DECK;
const RAIL: f64 = 1.2;
const RAIL_WIDTH: f64 = 0.8;
const RAIL_FOOT: f64 = 1.0;
const MARK: f64 = 1.0;
const SINK: f64 = 2.0;
const DOT: f64 = 0.9;
const RING: (f64, f64) = (0.6, 1.1);
const ARC_STEPS: usize = 8;
const AROUND: i32 = 32;
const REFINE: f64 = 0.75;
const SIMPLIFY: f64 = 0.005;
const ROOMIEST: f64 = 0.5;
const OVERLAP: f64 = 0.02;
const SCRAP: f64 = 1e-6;

type Flat = (f64, f64);
type Rule = (usize, usize, i32, Option<(PieceId, PieceId)>);

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
    let mut bytes = vec![0u8; 80];
    bytes.extend(((corners.len() / 3) as u32).to_le_bytes());
    for corner in corners.chunks(3) {
        let [a, b, c] = [at(corner[0]), at(corner[1]), at(corner[2])];
        for point in [normal(a, b, c), a, b, c] {
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
    let levels = levels(maze);
    let rows = maze.grid.height as f64;
    let place = |(x, y): Flat| [x * CELL, (rows - y) * CELL];
    let plan = |id: PieceId, reach: f64| {
        let points: Vec<[f64; 2]> = outline(maze, id, reach).into_iter().map(place).collect();
        CrossSection::from_simple_polygon_with_fill_rule(&points, FillRule::NonZero)
    };
    let mut parts = Vec::new();
    for (id, piece) in maze.pieces().iter().enumerate() {
        let centre = middle(piece.cell);
        let local = |x: f64, y: f64| (x / CELL - centre.0, rows - y / CELL - centre.1);
        let surface = |x: f64, y: f64| {
            let (toward, s) = arm(maze, id, local(x, y));
            height(maze, &levels, id, toward, s)
        };
        let ramps = piece
            .links
            .iter()
            .any(|&(_, other)| levels.of[other] != levels.of[id]);
        let bridge = is_bridge(maze, id);
        let walk = plan(id, OVERLAP);
        let inside =
            plan(id, OVERLAP + RAIL_WIDTH / CELL).offset(-RAIL_WIDTH, JoinType::Round, 2.0, AROUND);
        let mut rails = walk.difference(&inside);
        parts.push(shaped(&walk, ramps, |x, y, t| {
            let top = surface(x, y);
            let bottom = if bridge { top - DECK } else { 0.0 };
            bottom + (top - bottom) * t
        }));
        if let [lower, upper] = maze.pieces_in(piece.cell)
            && *lower == id
        {
            let deck = mm(levels.of[*upper]) - DECK / 2.0;
            let over = plan(*upper, 0.0);
            parts.push(shaped(&rails.intersection(&over), false, |x, y, t| {
                let bottom = surface(x, y) - RAIL_FOOT;
                bottom + (deck - bottom) * t
            }));
            rails = rails.difference(&over);
        }
        parts.push(shaped(&rails, ramps, |x, y, t| {
            let walkway = surface(x, y);
            let bottom = walkway - RAIL_FOOT;
            bottom + (walkway + RAIL - bottom) * t
        }));
    }
    for (cell, (inner, outer)) in [(maze.start, (0.0, DOT)), (maze.finish, RING)] {
        let id = maze.pieces_in(cell)[0];
        let top = height(maze, &levels, id, maze.piece(id).links[0].1, 0.0) + MARK;
        let tall = MARK + SINK;
        let disc = |radius: f64| Manifold::cylinder(tall, radius, radius, AROUND, false);
        let mark = if inner > 0.0 {
            disc(outer).difference(&disc(inner))
        } else {
            disc(outer)
        };
        let [x, y] = place(middle(cell));
        parts.push(mark.translate(x, y, top - tall));
    }
    let joined = Manifold::batch_union(&parts).simplify(SIMPLIFY);
    let solids: Vec<Manifold> = joined
        .decompose()
        .into_iter()
        .filter(|part| part.volume() > SCRAP)
        .collect();
    Manifold::compose(&solids)
}

fn shaped(plan: &CrossSection, refine: bool, z: impl Fn(f64, f64, f64) -> f64) -> Manifold {
    let block = plan.extrude(1.0);
    let block = if refine {
        block.refine_to_length(REFINE)
    } else {
        block
    };
    block.warp(|x, y, t| [x, y, z(x, y, t)])
}

fn outline(maze: &Maze, id: PieceId, reach: f64) -> Vec<Flat> {
    let piece = maze.piece(id);
    let centre = middle(piece.cell);
    let joined = |dir: Dir| piece.has_side(dir);
    let corner = |a: Dir, b: Dir| add(centre, scale(add(offset(a), offset(b)), HALF));
    let arc = |from: Flat, one: Flat, two: Flat, radius: f64| -> Vec<Flat> {
        (0..=ARC_STEPS)
            .map(|step| {
                let angle = FRAC_PI_2 * step as f64 / ARC_STEPS as f64;
                add(
                    from,
                    add(
                        scale(one, radius * angle.cos()),
                        scale(two, radius * angle.sin()),
                    ),
                )
            })
            .collect()
    };
    let out = 0.5 - HALF + reach;
    let mut points = Vec::new();
    for (i, &a) in Dir::ALL.iter().enumerate() {
        let before = Dir::ALL[(i + 3) % 4];
        let after = Dir::ALL[(i + 1) % 4];
        let at = corner(before, a);
        let (ob, oa) = (offset(before), offset(a));
        match (joined(before), joined(a)) {
            (true, true) => {
                let pivot = add(at, scale(add(ob, oa), INNER_ROUNDING));
                points.extend(arc(pivot, scale(oa, -1.0), scale(ob, -1.0), INNER_ROUNDING));
            }
            (false, false) => {
                let pivot = add(at, scale(add(ob, oa), -OUTER_ROUNDING));
                points.extend(arc(pivot, ob, oa, OUTER_ROUNDING));
            }
            _ => points.push(at),
        }
        if joined(a) {
            points.push(add(at, scale(oa, out)));
            points.push(add(corner(a, after), scale(oa, out)));
        }
    }
    points
}

fn arm(maze: &Maze, id: PieceId, (u, v): Flat) -> (PieceId, f64) {
    maze.piece(id)
        .links
        .iter()
        .map(|&(dir, other)| {
            let (dx, dy) = offset(dir);
            (other, u * dx + v * dy)
        })
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(other, s)| (other, s.max(0.0)))
        .expect("a piece has links")
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

fn normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let (u, v) = (
        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
    );
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length == 0.0 {
        n
    } else {
        n.map(|value| value / length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::maze;
    use crate::grid::Grid;

    const TRIVIAL: Grid = Grid {
        width: 12,
        height: 15,
    };
    const EASY: Grid = Grid {
        width: 16,
        height: 20,
    };

    #[test]
    fn the_print_is_one_solid() {
        for (grid, seed) in [(TRIVIAL, 3), (EASY, 7), (EASY, 35), (EASY, 48)] {
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
