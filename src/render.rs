use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use crate::maze::{walk_length, Maze, Orient, DC, DR};
use crate::pdf::{helvetica_width_in_points, Canvas, LineCap, PAGE_H, PAGE_W};

const PATH_SHARE_OF_CELL: f64 = 0.75;
const SOLID_GROUND_HATCHING: [f64; 3] = [0.7, 0.7, 0.7];
const HIDDEN_EDGE_DASHES: [f64; 3] = [0.6, 0.6, 0.6];
const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
const LABEL_GREY: [f64; 3] = [0.3, 0.3, 0.3];
const CAPTION_GREY: [f64; 3] = [0.45, 0.45, 0.45];
const GRAPH_PAPER_BLUE: [f64; 3] = [0.72, 0.82, 0.93];
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

pub fn red_to_violet_along_route(fraction_of_route: f64) -> [f64; 3] {
    let t = fraction_of_route;
    hsv_to_rgb(0.78 * t, 0.9, 0.85 - 0.15 * (PI * t.min(0.4) / 0.4).sin())
}

fn path_sides_within_cell(cell: f64) -> (f64, f64) {
    let near = cell * (1.0 - PATH_SHARE_OF_CELL) / 2.0;
    (near, cell - near)
}

fn solution_line_width(cell: f64) -> f64 {
    cell * 0.35
}

#[derive(Clone, Copy)]
struct SolutionStep {
    from: usize,
    to: usize,
    starts_at: f64,
    ends_at: f64,
}

pub struct Placement {
    pub left: f64,
    pub top: f64,
    pub cell: f64,
}

pub struct Look {
    pub graph_paper: bool,
    pub zone_tints: bool,
    pub back_of_sheet: bool,
}

pub fn draw_maze(cv: &mut Canvas, m: &Maze, at: &Placement, solution: Option<&[usize]>, look: &Look) {
    let (x0, y0, s) = (at.left, at.top, at.cell);
    let back_of_sheet = look.back_of_sheet;
    let (rows, cols) = (m.rows as f64, m.cols as f64);
    let (near, far) = path_sides_within_cell(s);
    let wall_width = (s * 0.06).max(0.6);
    let row_col = |c: usize| ((c / m.cols) as f64, (c % m.cols) as f64);
    let centre = |c: usize| {
        let (r, k) = row_col(c);
        (x0 + (k + 0.5) * s, y0 + (r + 0.5) * s)
    };
    let line = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| cv.line(x0 + x1, y0 + y1, x0 + x2, y0 + y2);
    let rect = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| {
        cv.rect(x0 + x1.min(x2), y0 + y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs())
    };
    let ink = |cv: &mut Canvas, colour: [f64; 3], width: f64| {
        cv.colour(colour);
        cv.width(width);
        cv.cap(LineCap::Square);
    };
    let solid_under_single_bridge = |c: usize| m.solid_under_bridge[c] && !m.is_crossing(c);
    let floor_colour =
        |c: usize| if look.zone_tints { ZONE_TINTS[m.zone[c] as usize % ZONE_TINTS.len()] } else { WHITE };

    if !back_of_sheet {
        draw_ground(cv, m, at, look.graph_paper, &floor_colour);
    }

    let ordered = |a: usize, b: usize| (a.min(b), a.max(b));
    let bridge_ends: HashSet<(usize, usize)> = m.bridges.iter().map(|b| (b.top_or_left, b.bottom_or_right)).collect();
    let mut steps: Vec<SolutionStep> = Vec::new();
    if let Some(route) = solution {
        let total = walk_length(route, m.cols).max(1) as f64;
        let mut walked = 0.0;
        for pair in route.windows(2) {
            let length = walk_length(pair, m.cols) as f64;
            steps.push(SolutionStep {
                from: pair[0],
                to: pair[1],
                starts_at: walked / total,
                ends_at: (walked + length) / total,
            });
            walked += length;
        }
    }
    let stroke_solution = |cv: &mut Canvas, steps: &[SolutionStep]| {
        cv.width(solution_line_width(s));
        cv.cap(LineCap::Round);
        for step in steps {
            let ((px, py), (qx, qy)) = (centre(step.from), centre(step.to));
            let pieces = if step.from == step.to { 1 } else { 4 * walk_length(&[step.from, step.to], m.cols) };
            for i in 0..pieces {
                let (f0, f1) = (i as f64 / pieces as f64, (i + 1) as f64 / pieces as f64);
                let fraction = step.starts_at + (step.ends_at - step.starts_at) * (f0 + f1) / 2.0;
                cv.colour(red_to_violet_along_route(fraction));
                cv.line(px + (qx - px) * f0, py + (qy - py) * f0, px + (qx - px) * f1, py + (qy - py) * f1);
                cv.stroke();
            }
        }
    };

    if solution.is_some() {
        let mut steps_on_ground: Vec<SolutionStep> =
            steps.iter().copied().filter(|st| !bridge_ends.contains(&ordered(st.from, st.to))).collect();
        steps_on_ground.push(SolutionStep { from: m.start, to: m.start, starts_at: 0.0, ends_at: 0.0 });
        stroke_solution(cv, &steps_on_ground);
    }
    if !back_of_sheet {
        draw_path_edges(cv, m, at);
    }

    let steps_on_bridges: HashMap<(usize, usize), SolutionStep> = steps
        .iter()
        .copied()
        .filter(|st| bridge_ends.contains(&ordered(st.from, st.to)))
        .map(|st| (ordered(st.from, st.to), st))
        .collect();
    let draw_deck = |cv: &mut Canvas, i: usize| {
        let b = &m.bridges[i];
        let ((ra, ca), (rb, cb)) = (row_col(b.top_or_left), row_col(b.bottom_or_right));
        cv.colour(if back_of_sheet { WHITE } else { floor_colour(b.top_or_left) });
        let (x1, y1, x2, y2) = match b.orientation {
            Orient::H => (ca * s + far, ra * s + near, cb * s + near, ra * s + far),
            Orient::V => (ca * s + near, ra * s + far, ca * s + far, rb * s + near),
        };
        rect(cv, x1, y1, x2, y2);
        cv.fill();
        if back_of_sheet {
            return;
        }
        ink(cv, BLACK, wall_width);
        match b.orientation {
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
    let solution_along_bridge = |cv: &mut Canvas, i: usize| {
        let b = &m.bridges[i];
        if let Some(&step) = steps_on_bridges.get(&(b.top_or_left, b.bottom_or_right)) {
            stroke_solution(cv, &[step]);
        }
    };
    let outline_what_runs_underneath = |cv: &mut Canvas, c: usize| {
        let (r, k) = row_col(c);
        let (x, y) = (k * s, r * s);
        let underneath = if m.is_crossing(c) {
            m.bridges[m.top_bridge_at_crossing[&c]].orientation.other()
        } else {
            m.overhead(c).unwrap().other()
        };
        match underneath {
            Orient::V => {
                for ex in [x + near, x + far] {
                    line(cv, ex, y + near, ex, y + far);
                }
            }
            Orient::H => {
                for ey in [y + near, y + far] {
                    line(cv, x + near, ey, x + far, ey);
                }
            }
        }
    };
    let start_dashing = |cv: &mut Canvas| {
        ink(cv, HIDDEN_EDGE_DASHES, (wall_width * 0.7).max(0.5));
        cv.dash(&[s * 0.08, s * 0.07]);
    };
    let clip_to_cell = |cv: &mut Canvas, c: usize| {
        let (r, k) = row_col(c);
        cv.save();
        rect(cv, k * s, r * s, (k + 1.0) * s, (r + 1.0) * s);
        cv.clip();
    };

    for i in 0..m.bridges.len() {
        draw_deck(cv, i);
    }
    let mut crossings_with_top_bridge: Vec<(usize, usize)> = m.top_bridge_at_crossing.iter().map(|(&c, &i)| (c, i)).collect();
    crossings_with_top_bridge.sort();
    for &(c, top) in &crossings_with_top_bridge {
        clip_to_cell(cv, c);
        draw_deck(cv, top);
        cv.restore();
    }
    if !back_of_sheet {
        start_dashing(cv);
        for c in 0..m.zone.len() {
            if m.bridges_overhead[c] != 0 && !solid_under_single_bridge(c) {
                outline_what_runs_underneath(cv, c);
            }
        }
        cv.stroke();
        cv.dash(&[]);
    }
    for i in 0..m.bridges.len() {
        solution_along_bridge(cv, i);
    }
    for &(c, top) in &crossings_with_top_bridge {
        let lower = m.bridges.iter().position(|b| b.orientation != m.bridges[top].orientation && b.spans.contains(&c)).unwrap();
        let (lower_bridge, top_bridge) = (&m.bridges[lower], &m.bridges[top]);
        let solution_runs_underneath = steps_on_bridges.contains_key(&(lower_bridge.top_or_left, lower_bridge.bottom_or_right));
        if !solution_runs_underneath {
            continue;
        }
        clip_to_cell(cv, c);
        draw_deck(cv, top);
        if !back_of_sheet {
            start_dashing(cv);
            outline_what_runs_underneath(cv, c);
            cv.stroke();
            cv.dash(&[]);
        }
        if steps_on_bridges.contains_key(&(top_bridge.top_or_left, top_bridge.bottom_or_right)) {
            solution_along_bridge(cv, top);
        }
        cv.restore();
    }
    if back_of_sheet {
        return;
    }

    let symbol_size = s * 0.6;
    let (gx, gy) = centre(m.goal);
    for (radius, colour) in [(0.42, WHITE), (0.27, red_to_violet_along_route(1.0)), (0.12, WHITE)] {
        cv.colour(colour);
        cv.circle(gx, gy, radius * symbol_size);
        cv.fill();
    }
    let (cx, cy) = centre(m.start);
    let only_way_out = m.openings_including_bridges(m.start)[0];
    let (dr, dc) = (DR[only_way_out] as f64, DC[only_way_out] as f64);
    let w = symbol_size;
    let tip = (cx + dc * 0.4 * w, cy + dr * 0.4 * w);
    let tail = (cx - dc * 0.32 * w, cy - dr * 0.32 * w);
    cv.move_to(tip.0, tip.1);
    cv.line_to(tail.0 - dr * 0.36 * w, tail.1 + dc * 0.36 * w);
    cv.line_to(tail.0 + dr * 0.36 * w, tail.1 - dc * 0.36 * w);
    cv.close();
    cv.colour(WHITE);
    cv.fill();

    ink(cv, BLACK, wall_width);
    rect(cv, 0.0, 0.0, cols * s, rows * s);
    cv.stroke();
}

fn draw_ground(cv: &mut Canvas, m: &Maze, at: &Placement, graph_paper: bool, floor_colour: &dyn Fn(usize) -> [f64; 3]) {
    let (x0, y0, s) = (at.left, at.top, at.cell);
    let (rows, cols) = (m.rows as f64, m.cols as f64);
    let (near, far) = path_sides_within_cell(s);
    let row_col = |c: usize| ((c / m.cols) as f64, (c % m.cols) as f64);
    let line = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| cv.line(x0 + x1, y0 + y1, x0 + x2, y0 + y2);
    let rect = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| {
        cv.rect(x0 + x1.min(x2), y0 + y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs())
    };
    let solid_under_single_bridge = |c: usize| m.solid_under_bridge[c] && !m.is_crossing(c);
    cv.save();
    rect(cv, 0.0, 0.0, cols * s, rows * s);
    cv.clip();
    cv.colour(SOLID_GROUND_HATCHING);
    cv.width((s * 0.02).max(0.25));
    cv.cap(LineCap::Square);
    let hatch_spacing = (s * 0.14).max(1.5);
    let mut k = -rows * s;
    while k < cols * s {
        line(cv, k, 0.0, k + rows * s, rows * s);
        k += hatch_spacing;
    }
    cv.stroke();
    cv.restore();
    if graph_paper {
        cv.colour(GRAPH_PAPER_BLUE);
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
        let (r, k) = row_col(c);
        cv.colour(floor_colour(c));
        if m.bridges_overhead[c] == 0 {
            rect(cv, k * s + near, r * s + near, k * s + far, r * s + far);
        } else if !solid_under_single_bridge(c) {
            match m.overhead(c) {
                Some(Orient::H) => rect(cv, k * s + near, r * s, k * s + far, (r + 1.0) * s),
                Some(Orient::V) => rect(cv, k * s, r * s + near, (k + 1.0) * s, r * s + far),
                None => {}
            }
        }
        for d in [1, 3] {
            if m.is_open(c, d) {
                let (r2, k2) = row_col(m.step(c, d).unwrap());
                rect(cv, k * s + near, r * s + near, k2 * s + far, r2 * s + far);
            }
        }
        cv.fill();
    }

    for (end, fraction_of_route) in [(m.start, 0.0), (m.goal, 1.0)] {
        let (r, k) = row_col(end);
        cv.colour(red_to_violet_along_route(fraction_of_route));
        rect(cv, k * s + near, r * s + near, k * s + far, r * s + far);
        cv.fill();
    }
}

fn draw_path_edges(cv: &mut Canvas, m: &Maze, at: &Placement) {
    let (x0, y0, s) = (at.left, at.top, at.cell);
    let (near, far) = path_sides_within_cell(s);
    let line = |cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64| cv.line(x0 + x1, y0 + y1, x0 + x2, y0 + y2);
    cv.colour(BLACK);
    cv.width((s * 0.06).max(0.6));
    cv.cap(LineCap::Square);
    for c in 0..m.zone.len() {
        if m.floorless(c) {
            continue;
        }
        let (x, y) = ((c % m.cols) as f64 * s, (c / m.cols) as f64 * s);
        if let Some(bridge_overhead) = m.overhead(c) {
            match bridge_overhead {
                Orient::H => {
                    for ex in [x + near, x + far] {
                        line(cv, ex, y, ex, y + near);
                        line(cv, ex, y + far, ex, y + s);
                    }
                }
                Orient::V => {
                    for ey in [y + near, y + far] {
                        line(cv, x, ey, x + near, ey);
                        line(cv, x + far, ey, x + s, ey);
                    }
                }
            }
            continue;
        }
        let opens = m.openings_including_bridges(c);
        for d in 0..4 {
            let (dr, dc) = (DR[d], DC[d]);
            if opens.contains(&d) {
                if dr != 0 {
                    let (ey0, ey1) = if dr < 0 { (y, y + near) } else { (y + far, y + s) };
                    line(cv, x + near, ey0, x + near, ey1);
                    line(cv, x + far, ey0, x + far, ey1);
                } else {
                    let (ex0, ex1) = if dc < 0 { (x, x + near) } else { (x + far, x + s) };
                    line(cv, ex0, y + near, ex1, y + near);
                    line(cv, ex0, y + far, ex1, y + far);
                }
            } else if dr != 0 {
                let ey = y + if dr < 0 { near } else { far };
                line(cv, x + near, ey, x + far, ey);
            } else {
                let ex = x + if dc < 0 { near } else { far };
                line(cv, ex, y + near, ex, y + far);
            }
        }
    }
    cv.stroke();
}

pub struct Item<'a> {
    pub maze: &'a Maze,
    pub seed_printed_as_code: Option<u64>,
    pub solution: Option<&'a [usize]>,
    pub solution_drawn_on_maze: bool,
    pub label: String,
}

pub struct Options {
    pub per_page: usize,
    pub graph_paper: bool,
    pub zone_tints_on_answers: bool,
    pub signature_once_a_page: Option<String>,
}

struct Area {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

fn draw_item_centred_with_caption(
    cv: &mut Canvas,
    it: &Item,
    opts: &Options,
    area: Area,
    font: f64,
    back_of_sheet: bool,
    signature: Option<&str>,
) {
    let m = it.maze;
    let caption_space = font * CAPTION_LINES_OF_SPACE;
    let s = (area.width / m.cols as f64).min((area.height - caption_space) / m.rows as f64);
    let x0 = area.left + (area.width - s * m.cols as f64) / 2.0;
    let y0 = area.top + (area.height - caption_space - s * m.rows as f64) / 2.0;
    let at = Placement { left: x0, top: y0, cell: s };
    if back_of_sheet {
        let look = Look { graph_paper: false, zone_tints: false, back_of_sheet: true };
        draw_maze(cv, m, &at, it.solution, &look);
        return;
    }
    let caption_baseline = y0 + s * m.rows as f64 + caption_space * 0.8;
    let shown = if it.solution_drawn_on_maze { it.solution } else { None };
    let look = Look {
        graph_paper: opts.graph_paper,
        zone_tints: opts.zone_tints_on_answers && it.solution_drawn_on_maze,
        back_of_sheet: false,
    };
    draw_maze(cv, m, &at, shown, &look);
    cv.colour(LABEL_GREY);
    cv.text(x0, caption_baseline, font, &it.label);
    let small = font * 0.85;
    cv.colour(CAPTION_GREY);
    if let Some(seed) = it.seed_printed_as_code {
        let code = crate::seed_code::encode(seed);
        cv.text(x0 + s * m.cols as f64 - helvetica_width_in_points(&code, small), caption_baseline, small, &code);
    }
    if let Some(signature) = signature {
        let centred = x0 + (s * m.cols as f64 - helvetica_width_in_points(signature, small)) / 2.0;
        cv.text(centred, caption_baseline, small, signature);
    }
}

const PAGE_MARGIN: f64 = 0.5 * 72.0;
const GUTTER_BETWEEN_MAZES: f64 = 0.3 * 72.0;
const CAPTION_LINES_OF_SPACE: f64 = 1.6;

struct PageGrid {
    cols: usize,
    box_width: f64,
    box_height: f64,
    font: f64,
}

fn page_grid(per_page: usize) -> PageGrid {
    let cols = if per_page == 1 { 1 } else { 2 };
    let rows = per_page / cols;
    PageGrid {
        cols,
        box_width: (PAGE_W - 2.0 * PAGE_MARGIN - (cols - 1) as f64 * GUTTER_BETWEEN_MAZES) / cols as f64,
        box_height: (PAGE_H - 2.0 * PAGE_MARGIN - (rows - 1) as f64 * GUTTER_BETWEEN_MAZES) / rows as f64,
        font: if per_page == 1 { 9.0 } else { 7.5 },
    }
}

pub struct SpaceForMaze {
    pub width: f64,
    pub height: f64,
}

pub fn space_for_one_maze(per_page: usize) -> SpaceForMaze {
    let grid = page_grid(per_page);
    SpaceForMaze { width: grid.box_width, height: grid.box_height - grid.font * CAPTION_LINES_OF_SPACE }
}

pub fn render_pages(items: &[Item], opts: &Options, back_of_sheet: bool) -> Vec<Canvas> {
    let grid = page_grid(opts.per_page);
    items
        .chunks(opts.per_page)
        .map(|chunk| {
            let mut cv = Canvas::new();
            cv.save();
            if back_of_sheet {
                cv.mirror_left_to_right_from_here_on();
            }
            let bottom_left = (chunk.len() - 1) / grid.cols * grid.cols;
            for (k, it) in chunk.iter().enumerate() {
                let (row, col) = (k / grid.cols, k % grid.cols);
                let area = Area {
                    left: PAGE_MARGIN + col as f64 * (grid.box_width + GUTTER_BETWEEN_MAZES),
                    top: PAGE_MARGIN + row as f64 * (grid.box_height + GUTTER_BETWEEN_MAZES),
                    width: grid.box_width,
                    height: grid.box_height,
                };
                let signature = opts.signature_once_a_page.as_deref().filter(|_| k == bottom_left);
                draw_item_centred_with_caption(&mut cv, it, opts, area, grid.font, back_of_sheet, signature);
            }
            cv.restore();
            cv
        })
        .collect()
}
