//! Drawing mazes onto PDF pages.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use crate::maze::{walk_length, Maze, Orient, DC, DR};
use crate::pdf::{text_width, Canvas, PAGE_H, PAGE_W};

const PATH_WIDTH: f64 = 0.75; // paths are this fraction of a cell; thin walls take the rest
const HATCH: [f64; 3] = [0.7, 0.7, 0.7]; // faint hatching for the solid ground (easy on ink)
const HIDDEN: [f64; 3] = [0.6, 0.6, 0.6]; // dashes for path edges hidden under a bridge
const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
const CAPTION: [f64; 3] = [0.45, 0.45, 0.45]; // the code and signature under a maze
const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
const ZONE_TINTS: [[f64; 3]; 8] = [
    [0.98, 0.8, 0.8],
    [0.78, 0.87, 0.98],
    [0.8, 0.95, 0.8],
    [0.98, 0.92, 0.72],
    [0.9, 0.82, 0.97],
    [0.75, 0.94, 0.94],
    [0.98, 0.84, 0.92],
    [0.88, 0.88, 0.8],
];

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i64 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// Solution colour at fraction t of the way: red, orange, green, blue, violet.
pub fn path_colour(t: f64) -> [f64; 3] {
    hsv_to_rgb(0.78 * t, 0.9, 0.85 - 0.15 * (PI * t.min(0.4) / 0.4).sin())
}

/// One step of the solution and how far along the route it starts and ends.
type Seg = (usize, usize, f64, f64);

/// Draw the maze as thin paths through solid ground. With `back`, draw
/// only the solution line, cut where it passes under bridges, for the back
/// of the sheet. A bridge is just a
/// path drawn on top: whatever runs under it stops at one side and comes
/// out the other, its hidden edges shown as faint dashes.
pub fn draw_maze(
    cv: &mut Canvas,
    m: &Maze,
    x0: f64,
    y0: f64,
    s: f64,
    solution: Option<&[usize]>,
    graph_paper: bool,
    show_zones: bool,
    back: bool,
) {
    let (rows, cols) = (m.rows as f64, m.cols as f64);
    let p0 = s * (1.0 - PATH_WIDTH) / 2.0; // a path's edges, measured into its cell
    let p1 = s - p0;
    let wall_w = (s * 0.06).max(0.6);
    let sol_w = solution_width(s);
    let rc = |c: usize| ((c / m.cols) as f64, (c % m.cols) as f64);
    let center = |c: usize| {
        let (r, k) = rc(c);
        (x0 + (k + 0.5) * s, y0 + (r + 0.5) * s)
    };
    let line = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| cv.line(x0 + x1, y0 + y1, x0 + x2, y0 + y2);
    let rect = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| {
        cv.rect(x0 + x1.min(x2), y0 + y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs())
    };
    let ink = |cv: &mut Canvas, colour: [f64; 3], width: f64| {
        cv.colour(colour);
        cv.width(width);
        cv.cap(2);
    };
    // Solid ground for drawing: under a single bridge with no passage.
    let solid = |c: usize| m.solid[c] && m.under[c] != 3;

    // Path floors are white, or tinted by zone if asked.
    let floor = |c: usize| if show_zones { ZONE_TINTS[m.zone[c] as usize % ZONE_TINTS.len()] } else { WHITE };

    if !back {
        draw_ground(cv, m, x0, y0, s, graph_paper, &floor);
    }

    // How far along the solution each step starts and ends, from 0 to 1,
    // so the line can shift colour from entrance to exit.
    let edge = |a: usize, b: usize| (a.min(b), a.max(b));
    let bridge_pairs: HashSet<(usize, usize)> = m.bridges.iter().map(|b| (b.a, b.b)).collect();
    let mut segs: Vec<Seg> = Vec::new();
    if let Some(sol) = solution {
        let total = walk_length(sol, m.cols).max(1) as f64;
        let mut done = 0.0;
        for w in sol.windows(2) {
            let step = walk_length(w, m.cols) as f64;
            segs.push((w[0], w[1], done / total, (done + step) / total));
            done += step;
        }
    }
    let stroke_segments = |cv: &mut Canvas, segs: &[Seg]| {
        cv.width(sol_w);
        cv.cap(1);
        for &(p, q, t0, t1) in segs {
            let ((px, py), (qx, qy)) = (center(p), center(q));
            // Shade along the step in short pieces; round caps hide the joins.
            let n = if p == q { 1 } else { 4 * walk_length(&[p, q], m.cols) };
            for i in 0..n {
                let (f0, f1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
                cv.colour(path_colour(t0 + (t1 - t0) * (f0 + f1) / 2.0));
                cv.line(px + (qx - px) * f0, py + (qy - py) * f0, px + (qx - px) * f1, py + (qy - py) * f1);
                cv.stroke();
            }
        }
    };

    // The solution on the ground goes down first: bridges cover it.
    if solution.is_some() {
        let mut ground: Vec<Seg> =
            segs.iter().copied().filter(|&(p, q, _, _)| !bridge_pairs.contains(&edge(p, q))).collect();
        ground.push((m.start, m.start, 0.0, 0.0));
        stroke_segments(cv, &ground);
    }
    if !back {
        draw_path_edges(cv, m, x0, y0, s);
    }

    // Bridges: a deck from landing to landing (white on the back of the
    // sheet, where it only cuts the line), then its two edges.
    let on_bridge: HashMap<(usize, usize), Seg> = segs
        .iter()
        .copied()
        .filter(|&(p, q, _, _)| bridge_pairs.contains(&edge(p, q)))
        .map(|sg| (edge(sg.0, sg.1), sg))
        .collect();
    let draw_deck = |cv: &mut Canvas, i: usize| {
        let b = &m.bridges[i];
        let ((ra, ca), (rb, cb)) = (rc(b.a), rc(b.b));
        cv.colour(if back { WHITE } else { floor(b.a) });
        let (x1, y1, x2, y2) = match b.o {
            Orient::H => (ca * s + p1, ra * s + p0, cb * s + p0, ra * s + p1),
            Orient::V => (ca * s + p0, ra * s + p1, ca * s + p1, rb * s + p0),
        };
        rect(cv, x1, y1, x2, y2);
        cv.fill();
        if back {
            return;
        }
        ink(cv, BLACK, wall_w);
        match b.o {
            Orient::H => {
                line(cv, x1, y1, x2, y1);
                line(cv, x1, y2, x2, y2);
            }
            Orient::V => {
                line(cv, x1, y1, x1, y2);
                line(cv, x2, y1, x2, y2);
            }
        }
        cv.stroke();
    };
    let deck_line = |cv: &mut Canvas, i: usize| {
        let b = &m.bridges[i];
        if let Some(&sg) = on_bridge.get(&(b.a, b.b)) {
            stroke_segments(cv, &[sg]);
        }
    };
    // Hidden edges: where something passes under a bridge, its edges show
    // through as faint dashes, the way drawings show hidden lines.
    let hidden_edges = |cv: &mut Canvas, c: usize| {
        let (r, k) = rc(c);
        let (x, y) = (k * s, r * s);
        let hidden = if m.under[c] == 3 {
            m.bridges[m.on_top[&c]].o.other() // the lower bridge, under the top one
        } else if m.under[c] == 1 {
            Orient::V // the passage under the bridge
        } else {
            Orient::H
        };
        if hidden == Orient::V {
            for ex in [x + p0, x + p1] {
                line(cv, ex, y + p0, ex, y + p1);
            }
        } else {
            for ey in [y + p0, y + p1] {
                line(cv, x + p0, ey, x + p1, ey);
            }
        }
    };
    let dashes = |cv: &mut Canvas| {
        ink(cv, HIDDEN, (wall_w * 0.7).max(0.5));
        cv.dash(&[s * 0.08, s * 0.07]);
    };
    let clip_cell = |cv: &mut Canvas, c: usize| {
        let (r, k) = rc(c);
        cv.save();
        rect(cv, k * s, r * s, (k + 1.0) * s, (r + 1.0) * s);
        cv.clip();
    };

    for i in 0..m.bridges.len() {
        draw_deck(cv, i);
    }
    // Where two bridges cross, redraw just that cell of whichever one is on
    // top there. Alternating tops along each bridge gives the basket weave.
    let mut tops: Vec<(usize, usize)> = m.on_top.iter().map(|(&c, &i)| (c, i)).collect();
    tops.sort();
    for &(c, i) in &tops {
        clip_cell(cv, c);
        draw_deck(cv, i);
        cv.restore();
    }
    if !back {
        dashes(cv);
        for c in 0..m.zone.len() {
            if m.under[c] != 0 && !solid(c) {
                hidden_edges(cv, c);
            }
        }
        cv.stroke();
        cv.dash(&[]);
    }
    // The solution along bridges goes over the dashes.
    for i in 0..m.bridges.len() {
        deck_line(cv, i);
    }
    // ...except where it runs along the lower of two crossing bridges:
    // there the top bridge covers it again.
    for &(c, top) in &tops {
        let lower = m.bridges.iter().position(|b| b.o != m.bridges[top].o && b.mids.contains(&c)).unwrap();
        let (b, t) = (&m.bridges[lower], &m.bridges[top]);
        if !on_bridge.contains_key(&(b.a, b.b)) {
            continue;
        }
        clip_cell(cv, c);
        draw_deck(cv, top);
        if !back {
            dashes(cv);
            hidden_edges(cv, c);
            cv.stroke();
            cv.dash(&[]);
        }
        if on_bridge.contains_key(&(t.a, t.b)) {
            deck_line(cv, top);
        }
        cv.restore();
    }
    if back {
        return;
    }

    // Start arrow and finish bullseye, white, sized to the path, on top.
    let w = s * 0.6; // symbols a little narrower than the path
    let (gx, gy) = center(m.goal);
    for (radius, colour) in [(0.42, WHITE), (0.27, path_colour(1.0)), (0.12, WHITE)] {
        cv.colour(colour);
        cv.circle(gx, gy, radius * w);
        cv.fill();
    }
    let (cx, cy) = center(m.start);
    let d = m.openings(m.start)[0]; // point out through the one way out
    let (dr, dc) = (DR[d] as f64, DC[d] as f64);
    let tip = (cx + dc * 0.4 * w, cy + dr * 0.4 * w);
    let back = (cx - dc * 0.32 * w, cy - dr * 0.32 * w);
    cv.move_to(tip.0, tip.1);
    cv.line_to(back.0 - dr * 0.36 * w, back.1 + dc * 0.36 * w);
    cv.line_to(back.0 + dr * 0.36 * w, back.1 - dc * 0.36 * w);
    cv.close();
    cv.colour(WHITE);
    cv.fill();

    // A frame around it all.
    ink(cv, BLACK, wall_w);
    rect(cv, 0.0, 0.0, cols * s, rows * s);
    cv.stroke();
}

/// Solid ground as faint diagonal hatching, then the path floors over it,
/// and the start and finish squares.
fn draw_ground(cv: &mut Canvas, m: &Maze, x0: f64, y0: f64, s: f64, graph_paper: bool, floor: &dyn Fn(usize) -> [f64; 3]) {
    let (rows, cols) = (m.rows as f64, m.cols as f64);
    let p0 = s * (1.0 - PATH_WIDTH) / 2.0;
    let p1 = s - p0;
    let rc = |c: usize| ((c / m.cols) as f64, (c % m.cols) as f64);
    let line = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| cv.line(x0 + x1, y0 + y1, x0 + x2, y0 + y2);
    let rect = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| {
        cv.rect(x0 + x1.min(x2), y0 + y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs())
    };
    let solid = |c: usize| m.solid[c] && m.under[c] != 3;
    cv.save();
    rect(cv, 0.0, 0.0, cols * s, rows * s);
    cv.clip();
    cv.colour(HATCH);
    cv.width((s * 0.02).max(0.25));
    cv.cap(2);
    let step = (s * 0.14).max(1.5);
    let mut k = -rows * s;
    while k < cols * s {
        line(cv, k, 0.0, k + rows * s, rows * s);
        k += step;
    }
    cv.stroke();
    cv.restore();
    if graph_paper {
        cv.colour([0.72, 0.82, 0.93]);
        cv.width(0.3);
        for r in 0..=m.rows {
            line(cv, 0.0, r as f64 * s, cols * s, r as f64 * s);
        }
        for c in 0..=m.cols {
            line(cv, c as f64 * s, 0.0, c as f64 * s, rows * s);
        }
        cv.stroke();
    }

    for c in 0..m.zone.len() {
        let (r, k) = rc(c);
        cv.colour(floor(c));
        match m.under[c] {
            0 => rect(cv, k * s + p0, r * s + p0, k * s + p1, r * s + p1),
            1 if !solid(c) => rect(cv, k * s + p0, r * s, k * s + p1, (r + 1.0) * s),
            2 if !solid(c) => rect(cv, k * s, r * s + p0, (k + 1.0) * s, r * s + p1),
            _ => {}
        }
        for d in [1, 3] {
            if m.is_open(c, d) {
                let (r2, k2) = rc(m.step(c, d).unwrap());
                rect(cv, k * s + p0, r * s + p0, k2 * s + p1, r2 * s + p1);
            }
        }
        cv.fill();
    }

    // Start and finish fill their whole path square.
    for (cell, t) in [(m.start, 0.0), (m.goal, 1.0)] {
        let (r, k) = rc(cell);
        cv.colour(path_colour(t));
        rect(cv, k * s + p0, r * s + p0, k * s + p1, r * s + p1);
        cv.fill();
    }
}

/// Path edges on the ground. Each side of a path square is either open
/// (two short edges run out to the cell's side) or closed (one edge).
fn draw_path_edges(cv: &mut Canvas, m: &Maze, x0: f64, y0: f64, s: f64) {
    let p0 = s * (1.0 - PATH_WIDTH) / 2.0;
    let p1 = s - p0;
    let line = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| cv.line(x0 + x1, y0 + y1, x0 + x2, y0 + y2);
    cv.colour(BLACK);
    cv.width((s * 0.06).max(0.6));
    cv.cap(2);
    for c in 0..m.zone.len() {
        if m.floorless(c) {
            continue; // solid, or two bridges cross here: no ground at all
        }
        let (x, y) = ((c % m.cols) as f64 * s, (c / m.cols) as f64 * s);
        if m.under[c] != 0 {
            // A passage under a bridge: its edges stop at the bridge.
            if m.under[c] == 1 {
                for ex in [x + p0, x + p1] {
                    line(cv, ex, y, ex, y + p0);
                    line(cv, ex, y + p1, ex, y + s);
                }
            } else {
                for ey in [y + p0, y + p1] {
                    line(cv, x, ey, x + p0, ey);
                    line(cv, x + p1, ey, x + s, ey);
                }
            }
            continue;
        }
        let opens = m.openings(c);
        for d in 0..4 {
            let (dr, dc) = (DR[d], DC[d]);
            if opens.contains(&d) {
                if dr != 0 {
                    let (ey0, ey1) = if dr < 0 { (y, y + p0) } else { (y + p1, y + s) };
                    line(cv, x + p0, ey0, x + p0, ey1);
                    line(cv, x + p1, ey0, x + p1, ey1);
                } else {
                    let (ex0, ex1) = if dc < 0 { (x, x + p0) } else { (x + p1, x + s) };
                    line(cv, ex0, y + p0, ex1, y + p0);
                    line(cv, ex0, y + p1, ex1, y + p1);
                }
            } else if dr != 0 {
                let ey = y + if dr < 0 { p0 } else { p1 };
                line(cv, x + p0, ey, x + p1, ey);
            } else {
                let ex = x + if dc < 0 { p0 } else { p1 };
                line(cv, ex, y + p0, ex, y + p1);
            }
        }
    }
    cv.stroke();
}

pub struct Item<'a> {
    pub maze: &'a Maze,
    /// Its code is printed small beside the label, so it can be made again.
    pub seed: Option<u64>,
    pub solution: Option<&'a [usize]>,
    /// An answer page: the solution is drawn on the maze itself.
    pub answer: bool,
    pub label: String,
}

pub struct Options {
    pub per_page: usize,
    pub graph_paper: bool,
    pub show_zones: bool,
    /// Signed once on each front page, under the bottom-left maze.
    pub sign: Option<String>,
}

/// Draw one maze centred in box (x, y, w, h), with its label below on the
/// left and its code (if any) below on the right.
/// With `sign`, the signature goes between the two.
fn render_in_box(
    cv: &mut Canvas,
    it: &Item,
    opts: &Options,
    bx: (f64, f64, f64, f64),
    font: f64,
    back: bool,
    sign: Option<&str>,
) {
    let (bx, by, bw, bh) = bx;
    let m = it.maze;
    let gap = font * 1.6;
    let s = (bw / m.cols as f64).min((bh - gap) / m.rows as f64);
    let x0 = bx + (bw - s * m.cols as f64) / 2.0;
    let y0 = by + (bh - gap - s * m.rows as f64) / 2.0;
    if back {
        draw_maze(cv, m, x0, y0, s, it.solution, false, false, true);
        return;
    }
    let base = y0 + s * m.rows as f64 + gap * 0.8;
    let shown = if it.answer { it.solution } else { None };
    draw_maze(cv, m, x0, y0, s, shown, opts.graph_paper, opts.show_zones && it.answer, false);
    cv.colour([0.3, 0.3, 0.3]);
    cv.text(x0, base, font, &it.label);
    // The maze's code, small, so it can be made again.
    let size = font * 0.85;
    cv.colour(CAPTION);
    if let Some(seed) = it.seed {
        let text = crate::code::encode(seed);
        cv.text(x0 + s * m.cols as f64 - text_width(&text, size), base, size, &text);
    }
    if let Some(sign) = sign {
        cv.text(x0 + (s * m.cols as f64 - text_width(sign, size)) / 2.0, base, size, sign);
    }
}

/// Width of the solution line: a good part of the path, at every size.
fn solution_width(s: f64) -> f64 {
    s * 0.35
}

/// Lay items out one or four to a page. With `back`, draw each page as the
/// back of its sheet instead: only the solutions, mirrored left to right so
/// they line up with the front when printed double sided (flip on long edge).
pub fn render_pages(items: &[Item], opts: &Options, back: bool) -> Vec<Canvas> {
    let margin = 0.5 * 72.0;
    let per = opts.per_page;
    let cols = if per == 1 { 1 } else { 2 };
    let rows = per / cols;
    let gutter = 0.3 * 72.0;
    let cw = (PAGE_W - 2.0 * margin - (cols - 1) as f64 * gutter) / cols as f64;
    let ch = (PAGE_H - 2.0 * margin - (rows - 1) as f64 * gutter) / rows as f64;
    let font = if per == 1 { 9.0 } else { 7.5 };
    items
        .chunks(per)
        .map(|chunk| {
            let mut cv = Canvas::new();
            cv.save();
            if back {
                cv.mirror();
            }
            // Signed once a page, under the bottom-left maze.
            let signed = (chunk.len() - 1) / cols * cols;
            for (k, it) in chunk.iter().enumerate() {
                let (r, c) = (k / cols, k % cols);
                let bx = (margin + c as f64 * (cw + gutter), margin + r as f64 * (ch + gutter), cw, ch);
                let sign = opts.sign.as_deref().filter(|_| k == signed);
                render_in_box(&mut cv, it, opts, bx, font, back, sign);
            }
            cv.restore();
            cv
        })
        .collect()
}
