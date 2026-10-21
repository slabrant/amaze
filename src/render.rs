use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use crate::maze::{walk_length, Maze, Orient, DC, DR, EAST, SOUTH};
use crate::pdf::{helvetica_width_in_points, Canvas, LineCap, PAGE_H, PAGE_W};

type Colour = [f64; 3];

const BLACK: Colour = [0.0, 0.0, 0.0];
const WHITE: Colour = [1.0, 1.0, 1.0];
const LABEL_GREY: Colour = [0.3, 0.3, 0.3];
const CAPTION_GREY: Colour = [0.45, 0.45, 0.45];
const HIDDEN_EDGE_GREY: Colour = [0.6, 0.6, 0.6];
const SOLID_GROUND_HATCHING: Colour = [0.7, 0.7, 0.7];
const GRAPH_PAPER_BLUE: Colour = [0.72, 0.82, 0.93];
const ZONE_TINTS: [Colour; 8] = [
    [0.98, 0.8, 0.8],
    [0.78, 0.87, 0.98],
    [0.8, 0.95, 0.8],
    [0.98, 0.92, 0.72],
    [0.9, 0.82, 0.97],
    [0.75, 0.94, 0.94],
    [0.98, 0.84, 0.92],
    [0.88, 0.88, 0.8],
];

const PATH_SHARE_OF_CELL: f64 = 0.75;
const WALL_SHARE_OF_CELL: f64 = 0.06;
const THINNEST_WALL: f64 = 0.6;
const SOLUTION_LINE_SHARE_OF_CELL: f64 = 0.35;
const SOLUTION_SHADES_PER_CELL_WALKED: usize = 4;
const HIDDEN_EDGE_SHARE_OF_WALL: f64 = 0.7;
const THINNEST_HIDDEN_EDGE: f64 = 0.5;
const HIDDEN_EDGE_DASH_SHARE_OF_CELL: f64 = 0.08;
const HIDDEN_EDGE_GAP_SHARE_OF_CELL: f64 = 0.07;
const HATCH_LINE_SHARE_OF_CELL: f64 = 0.02;
const THINNEST_HATCH_LINE: f64 = 0.25;
const HATCH_SPACING_SHARE_OF_CELL: f64 = 0.14;
const CLOSEST_HATCH_SPACING: f64 = 1.5;
const GRAPH_PAPER_LINE: f64 = 0.3;

const SYMBOL_SHARE_OF_CELL: f64 = 0.6;
const BULLSEYE_OUTER_RING: f64 = 0.42;
const BULLSEYE_MIDDLE_RING: f64 = 0.27;
const BULLSEYE_CENTRE: f64 = 0.12;
const ARROW_TIP_AHEAD: f64 = 0.4;
const ARROW_TAIL_BEHIND: f64 = 0.32;
const ARROW_HALF_WIDTH: f64 = 0.36;

const HUE_FROM_RED_TO_VIOLET: f64 = 0.78;
const SOLUTION_SATURATION: f64 = 0.9;
const SOLUTION_BRIGHTNESS: f64 = 0.85;
const DARKER_THROUGH_THE_YELLOWS_BY: f64 = 0.15;
const YELLOWS_END_AT_FRACTION_OF_ROUTE: f64 = 0.4;

const PAGE_MARGIN: f64 = 0.5 * 72.0;
const GUTTER_BETWEEN_MAZES: f64 = 0.3 * 72.0;
const CAPTION_LINES_OF_SPACE: f64 = 1.6;
const CAPTION_BASELINE_DOWN_ITS_SPACE: f64 = 0.8;
const CODE_AND_SIGNATURE_SHARE_OF_FONT: f64 = 0.85;
const FONT_ONE_PER_PAGE: f64 = 9.0;
const FONT_FOUR_PER_PAGE: f64 = 7.5;

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> Colour {
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

pub fn red_to_violet_along_route(fraction_of_route: f64) -> Colour {
    let t = fraction_of_route;
    let through_the_yellows = (PI * t.min(YELLOWS_END_AT_FRACTION_OF_ROUTE) / YELLOWS_END_AT_FRACTION_OF_ROUTE).sin();
    let brightness = SOLUTION_BRIGHTNESS - DARKER_THROUGH_THE_YELLOWS_BY * through_the_yellows;
    hsv_to_rgb(HUE_FROM_RED_TO_VIOLET * t, SOLUTION_SATURATION, brightness)
}

pub struct Placement {
    pub left: f64,
    pub top: f64,
    pub cell_size: f64,
}

struct Grid<'a> {
    maze: &'a Maze,
    left: f64,
    top: f64,
    cell_size: f64,
    path_near_side: f64,
    path_far_side: f64,
    wall_width: f64,
}

impl<'a> Grid<'a> {
    fn new(maze: &'a Maze, at: &Placement) -> Grid<'a> {
        let s = at.cell_size;
        let near = s * (1.0 - PATH_SHARE_OF_CELL) / 2.0;
        Grid {
            maze,
            left: at.left,
            top: at.top,
            cell_size: s,
            path_near_side: near,
            path_far_side: s - near,
            wall_width: (s * WALL_SHARE_OF_CELL).max(THINNEST_WALL),
        }
    }

    fn corner(&self, cell: usize) -> (f64, f64) {
        let (row, col) = (cell / self.maze.cols, cell % self.maze.cols);
        (col as f64 * self.cell_size, row as f64 * self.cell_size)
    }

    fn centre(&self, cell: usize) -> (f64, f64) {
        let (x, y) = self.corner(cell);
        (self.left + x + self.cell_size / 2.0, self.top + y + self.cell_size / 2.0)
    }

    fn width(&self) -> f64 {
        self.maze.cols as f64 * self.cell_size
    }

    fn height(&self) -> f64 {
        self.maze.rows as f64 * self.cell_size
    }

    fn line(&self, cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64) {
        cv.line(self.left + x1, self.top + y1, self.left + x2, self.top + y2);
    }

    fn rect(&self, cv: &mut Canvas, x1: f64, y1: f64, x2: f64, y2: f64) {
        cv.rect(self.left + x1.min(x2), self.top + y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs());
    }

    fn path_square(&self, cv: &mut Canvas, cell: usize) {
        let (x, y) = self.corner(cell);
        let (near, far) = (self.path_near_side, self.path_far_side);
        self.rect(cv, x + near, y + near, x + far, y + far);
    }

    fn ink(&self, cv: &mut Canvas, colour: Colour, width: f64) {
        cv.colour(colour);
        cv.width(width);
        cv.cap(LineCap::Square);
    }
}

#[derive(Clone, Copy)]
struct SolutionStep {
    from: usize,
    to: usize,
    starts_at: f64,
    ends_at: f64,
}

pub struct Look {
    pub graph_paper: bool,
    pub zone_tints: bool,
    pub back_of_sheet: bool,
}

fn solution_steps(maze: &Maze, route: &[usize]) -> Vec<SolutionStep> {
    let total = walk_length(route, maze.cols).max(1) as f64;
    let mut walked = 0.0;
    let mut steps = Vec::new();
    for pair in route.windows(2) {
        let length = walk_length(pair, maze.cols) as f64;
        steps.push(SolutionStep { from: pair[0], to: pair[1], starts_at: walked / total, ends_at: (walked + length) / total });
        walked += length;
    }
    steps
}

fn stroke_solution(cv: &mut Canvas, grid: &Grid, steps: &[SolutionStep]) {
    cv.width(grid.cell_size * SOLUTION_LINE_SHARE_OF_CELL);
    cv.cap(LineCap::Round);
    for step in steps {
        let ((px, py), (qx, qy)) = (grid.centre(step.from), grid.centre(step.to));
        let shades = if step.from == step.to {
            1
        } else {
            SOLUTION_SHADES_PER_CELL_WALKED * walk_length(&[step.from, step.to], grid.maze.cols)
        };
        for i in 0..shades {
            let (f0, f1) = (i as f64 / shades as f64, (i + 1) as f64 / shades as f64);
            let fraction = step.starts_at + (step.ends_at - step.starts_at) * (f0 + f1) / 2.0;
            cv.colour(red_to_violet_along_route(fraction));
            cv.line(px + (qx - px) * f0, py + (qy - py) * f0, px + (qx - px) * f1, py + (qy - py) * f1);
            cv.stroke();
        }
    }
}

pub fn draw_maze(cv: &mut Canvas, maze: &Maze, at: &Placement, solution: Option<&[usize]>, look: &Look) {
    let grid = Grid::new(maze, at);
    let s = grid.cell_size;
    let (near, far) = (grid.path_near_side, grid.path_far_side);
    let floor_colour =
        |cell: usize| if look.zone_tints { ZONE_TINTS[maze.zone[cell] as usize % ZONE_TINTS.len()] } else { WHITE };

    if !look.back_of_sheet {
        draw_ground(cv, &grid, look.graph_paper, &floor_colour);
    }

    let ordered = |a: usize, b: usize| (a.min(b), a.max(b));
    let bridge_ends: HashSet<(usize, usize)> = maze.bridges.iter().map(|b| (b.top_or_left, b.bottom_or_right)).collect();
    let steps = solution.map(|route| solution_steps(maze, route)).unwrap_or_default();
    if solution.is_some() {
        let mut steps_on_ground: Vec<SolutionStep> =
            steps.iter().copied().filter(|st| !bridge_ends.contains(&ordered(st.from, st.to))).collect();
        steps_on_ground.push(SolutionStep { from: maze.start, to: maze.start, starts_at: 0.0, ends_at: 0.0 });
        stroke_solution(cv, &grid, &steps_on_ground);
    }
    if !look.back_of_sheet {
        draw_path_edges(cv, &grid);
    }

    let steps_on_bridges: HashMap<(usize, usize), SolutionStep> = steps
        .iter()
        .copied()
        .filter(|st| bridge_ends.contains(&ordered(st.from, st.to)))
        .map(|st| (ordered(st.from, st.to), st))
        .collect();
    let draw_deck = |cv: &mut Canvas, i: usize| {
        let b = &maze.bridges[i];
        let ((ax, ay), (bx, by)) = (grid.corner(b.top_or_left), grid.corner(b.bottom_or_right));
        cv.colour(if look.back_of_sheet { WHITE } else { floor_colour(b.top_or_left) });
        let (x1, y1, x2, y2) = match b.orientation {
            Orient::H => (ax + far, ay + near, bx + near, ay + far),
            Orient::V => (ax + near, ay + far, ax + far, by + near),
        };
        grid.rect(cv, x1, y1, x2, y2);
        cv.fill();
        if look.back_of_sheet {
            return;
        }
        grid.ink(cv, BLACK, grid.wall_width);
        match b.orientation {
            Orient::H => {
                grid.line(cv, x1, y1, x2, y1);
                grid.line(cv, x1, y2, x2, y2);
            }
            Orient::V => {
                grid.line(cv, x1, y1, x1, y2);
                grid.line(cv, x2, y1, x2, y2);
            }
        }
        cv.stroke();
    };
    let solution_along_bridge = |cv: &mut Canvas, i: usize| {
        let b = &maze.bridges[i];
        if let Some(&step) = steps_on_bridges.get(&(b.top_or_left, b.bottom_or_right)) {
            stroke_solution(cv, &grid, &[step]);
        }
    };
    let outline_what_runs_underneath = |cv: &mut Canvas, cell: usize| {
        let (x, y) = grid.corner(cell);
        match maze.overhead(cell).unwrap().other() {
            Orient::V => {
                for edge in [x + near, x + far] {
                    grid.line(cv, edge, y + near, edge, y + far);
                }
            }
            Orient::H => {
                for edge in [y + near, y + far] {
                    grid.line(cv, x + near, edge, x + far, edge);
                }
            }
        }
    };
    let start_dashing = |cv: &mut Canvas| {
        grid.ink(cv, HIDDEN_EDGE_GREY, (grid.wall_width * HIDDEN_EDGE_SHARE_OF_WALL).max(THINNEST_HIDDEN_EDGE));
        cv.dash(&[s * HIDDEN_EDGE_DASH_SHARE_OF_CELL, s * HIDDEN_EDGE_GAP_SHARE_OF_CELL]);
    };
    for i in 0..maze.bridges.len() {
        draw_deck(cv, i);
    }
    if !look.back_of_sheet {
        start_dashing(cv);
        for cell in 0..maze.zone.len() {
            if maze.bridges_overhead[cell] != 0 {
                outline_what_runs_underneath(cv, cell);
            }
        }
        cv.stroke();
        cv.dash(&[]);
    }
    for i in 0..maze.bridges.len() {
        solution_along_bridge(cv, i);
    }
    if look.back_of_sheet {
        return;
    }
    draw_finish_bullseye(cv, &grid);
    draw_start_arrow(cv, &grid);
    grid.ink(cv, BLACK, grid.wall_width);
    grid.rect(cv, 0.0, 0.0, grid.width(), grid.height());
    cv.stroke();
}

fn draw_finish_bullseye(cv: &mut Canvas, grid: &Grid) {
    let symbol_size = grid.cell_size * SYMBOL_SHARE_OF_CELL;
    let (gx, gy) = grid.centre(grid.maze.goal);
    let finish_colour = red_to_violet_along_route(1.0);
    for (radius, colour) in [(BULLSEYE_OUTER_RING, WHITE), (BULLSEYE_MIDDLE_RING, finish_colour), (BULLSEYE_CENTRE, WHITE)] {
        cv.colour(colour);
        cv.circle(gx, gy, radius * symbol_size);
        cv.fill();
    }
}

fn draw_start_arrow(cv: &mut Canvas, grid: &Grid) {
    let w = grid.cell_size * SYMBOL_SHARE_OF_CELL;
    let (cx, cy) = grid.centre(grid.maze.start);
    let only_way_out = grid.maze.openings_including_bridges(grid.maze.start)[0];
    let (dr, dc) = (DR[only_way_out] as f64, DC[only_way_out] as f64);
    let tip = (cx + dc * ARROW_TIP_AHEAD * w, cy + dr * ARROW_TIP_AHEAD * w);
    let tail = (cx - dc * ARROW_TAIL_BEHIND * w, cy - dr * ARROW_TAIL_BEHIND * w);
    cv.move_to(tip.0, tip.1);
    cv.line_to(tail.0 - dr * ARROW_HALF_WIDTH * w, tail.1 + dc * ARROW_HALF_WIDTH * w);
    cv.line_to(tail.0 + dr * ARROW_HALF_WIDTH * w, tail.1 - dc * ARROW_HALF_WIDTH * w);
    cv.close();
    cv.colour(WHITE);
    cv.fill();
}

fn draw_ground(cv: &mut Canvas, grid: &Grid, graph_paper: bool, floor_colour: &dyn Fn(usize) -> Colour) {
    let maze = grid.maze;
    let s = grid.cell_size;
    let (width, height) = (grid.width(), grid.height());
    let (near, far) = (grid.path_near_side, grid.path_far_side);
    cv.save();
    grid.rect(cv, 0.0, 0.0, width, height);
    cv.clip();
    cv.colour(SOLID_GROUND_HATCHING);
    cv.width((s * HATCH_LINE_SHARE_OF_CELL).max(THINNEST_HATCH_LINE));
    cv.cap(LineCap::Square);
    let hatch_spacing = (s * HATCH_SPACING_SHARE_OF_CELL).max(CLOSEST_HATCH_SPACING);
    let mut x = -height;
    while x < width {
        grid.line(cv, x, 0.0, x + height, height);
        x += hatch_spacing;
    }
    cv.stroke();
    cv.restore();
    if graph_paper {
        cv.colour(GRAPH_PAPER_BLUE);
        cv.width(GRAPH_PAPER_LINE);
        for row in 0..=maze.rows {
            grid.line(cv, 0.0, row as f64 * s, width, row as f64 * s);
        }
        for col in 0..=maze.cols {
            grid.line(cv, col as f64 * s, 0.0, col as f64 * s, height);
        }
        cv.stroke();
    }

    for cell in (0..maze.zone.len()).filter(|&cell| !maze.is_solid(cell)) {
        let (x, y) = grid.corner(cell);
        cv.colour(floor_colour(cell));
        if maze.bridges_overhead[cell] == 0 {
            grid.path_square(cv, cell);
        } else {
            match maze.overhead(cell).unwrap() {
                Orient::H => grid.rect(cv, x + near, y, x + far, y + s),
                Orient::V => grid.rect(cv, x, y + near, x + s, y + far),
            }
        }
        for direction in [SOUTH, EAST] {
            if maze.is_open(cell, direction) {
                let (x2, y2) = grid.corner(maze.step(cell, direction).unwrap());
                grid.rect(cv, x + near, y + near, x2 + far, y2 + far);
            }
        }
        cv.fill();
    }

    for (end, fraction_of_route) in [(maze.start, 0.0), (maze.goal, 1.0)] {
        cv.colour(red_to_violet_along_route(fraction_of_route));
        grid.path_square(cv, end);
        cv.fill();
    }
}

fn draw_path_edges(cv: &mut Canvas, grid: &Grid) {
    let maze = grid.maze;
    let s = grid.cell_size;
    let (near, far) = (grid.path_near_side, grid.path_far_side);
    cv.colour(BLACK);
    cv.width(grid.wall_width);
    cv.cap(LineCap::Square);
    for cell in (0..maze.zone.len()).filter(|&cell| !maze.is_solid(cell)) {
        let (x, y) = grid.corner(cell);
        if let Some(bridge_overhead) = maze.overhead(cell) {
            match bridge_overhead {
                Orient::H => {
                    for edge in [x + near, x + far] {
                        grid.line(cv, edge, y, edge, y + near);
                        grid.line(cv, edge, y + far, edge, y + s);
                    }
                }
                Orient::V => {
                    for edge in [y + near, y + far] {
                        grid.line(cv, x, edge, x + near, edge);
                        grid.line(cv, x + far, edge, x + s, edge);
                    }
                }
            }
            continue;
        }
        let opens = maze.openings_including_bridges(cell);
        for direction in 0..4 {
            let (dr, dc) = (DR[direction], DC[direction]);
            if opens.contains(&direction) {
                if dr != 0 {
                    let (y0, y1) = if dr < 0 { (y, y + near) } else { (y + far, y + s) };
                    grid.line(cv, x + near, y0, x + near, y1);
                    grid.line(cv, x + far, y0, x + far, y1);
                } else {
                    let (x0, x1) = if dc < 0 { (x, x + near) } else { (x + far, x + s) };
                    grid.line(cv, x0, y + near, x1, y + near);
                    grid.line(cv, x0, y + far, x1, y + far);
                }
            } else if dr != 0 {
                let wall_y = y + if dr < 0 { near } else { far };
                grid.line(cv, x + near, wall_y, x + far, wall_y);
            } else {
                let wall_x = x + if dc < 0 { near } else { far };
                grid.line(cv, wall_x, y + near, wall_x, y + far);
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
    item: &Item,
    opts: &Options,
    area: Area,
    font: f64,
    back_of_sheet: bool,
    signature: Option<&str>,
) {
    let maze = item.maze;
    let caption_space = font * CAPTION_LINES_OF_SPACE;
    let cell_size = (area.width / maze.cols as f64).min((area.height - caption_space) / maze.rows as f64);
    let (maze_width, maze_height) = (cell_size * maze.cols as f64, cell_size * maze.rows as f64);
    let left = area.left + (area.width - maze_width) / 2.0;
    let top = area.top + (area.height - caption_space - maze_height) / 2.0;
    let at = Placement { left, top, cell_size };
    if back_of_sheet {
        let look = Look { graph_paper: false, zone_tints: false, back_of_sheet: true };
        draw_maze(cv, maze, &at, item.solution, &look);
        return;
    }
    let caption_baseline = top + maze_height + caption_space * CAPTION_BASELINE_DOWN_ITS_SPACE;
    let shown = if item.solution_drawn_on_maze { item.solution } else { None };
    let look = Look {
        graph_paper: opts.graph_paper,
        zone_tints: opts.zone_tints_on_answers && item.solution_drawn_on_maze,
        back_of_sheet: false,
    };
    draw_maze(cv, maze, &at, shown, &look);
    cv.colour(LABEL_GREY);
    cv.text(left, caption_baseline, font, &item.label);
    let small = font * CODE_AND_SIGNATURE_SHARE_OF_FONT;
    cv.colour(CAPTION_GREY);
    if let Some(seed) = item.seed_printed_as_code {
        let code = crate::seed_code::encode(seed);
        cv.text(left + maze_width - helvetica_width_in_points(&code, small), caption_baseline, small, &code);
    }
    if let Some(signature) = signature {
        let centred = left + (maze_width - helvetica_width_in_points(signature, small)) / 2.0;
        cv.text(centred, caption_baseline, small, signature);
    }
}

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
        font: if per_page == 1 { FONT_ONE_PER_PAGE } else { FONT_FOUR_PER_PAGE },
    }
}

#[cfg(test)]
pub struct SpaceForMaze {
    pub width: f64,
    pub height: f64,
}

#[cfg(test)]
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
            for (k, item) in chunk.iter().enumerate() {
                let (row, col) = (k / grid.cols, k % grid.cols);
                let area = Area {
                    left: PAGE_MARGIN + col as f64 * (grid.box_width + GUTTER_BETWEEN_MAZES),
                    top: PAGE_MARGIN + row as f64 * (grid.box_height + GUTTER_BETWEEN_MAZES),
                    width: grid.box_width,
                    height: grid.box_height,
                };
                let signature = opts.signature_once_a_page.as_deref().filter(|_| k == bottom_left);
                draw_item_centred_with_caption(&mut cv, item, opts, area, grid.font, back_of_sheet, signature);
            }
            cv.restore();
            cv
        })
        .collect()
}
