#[cfg(test)]
use crate::drawing::Colour;
use crate::drawing::{Mark, Point, draw, draw_solution};
use crate::generate::{generate, generate_shaped};
use crate::grid::Grid;
use crate::maze::Maze;
use crate::pdf::LETTER;
use crate::shapes::Shape;

const MILLIMETRE: f64 = 72.0 / 25.4;
const MARGIN: f64 = 15.0 * MILLIMETRE;
const MINI_GAP: f64 = 10.0 * MILLIMETRE;
const SIDE_PER_ZONE: f64 = 4.5;
const RETRIES: u64 = 20;

pub const TRIVIAL: Grid = Grid {
    width: 12,
    height: 15,
};
pub const EASY: Grid = Grid {
    width: 16,
    height: 20,
};
pub const STANDARD: Grid = Grid {
    width: 20,
    height: 26,
};
pub const HARD: Grid = Grid {
    width: 24,
    height: 31,
};
pub const MINI_TRIVIAL: Grid = Grid {
    width: 8,
    height: 10,
};
pub const MINI_EASY: Grid = Grid {
    width: 10,
    height: 13,
};
pub const MINI_STANDARD: Grid = Grid {
    width: 12,
    height: 15,
};
pub const MINI_HARD: Grid = Grid {
    width: 13,
    height: 16,
};

pub struct Contents {
    pub per_level: usize,
    pub minis_per_level: usize,
}

struct Slot {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

pub fn book(seed: u64, contents: &Contents) -> Vec<Vec<Mark>> {
    let mut seeds = seed..;
    let mut pages = Vec::new();
    for grid in [TRIVIAL, EASY, STANDARD, HARD] {
        for _ in 0..contents.per_level {
            let maze = maze(grid, seeds.next().unwrap_or(seed));
            pages.extend(sheet(&[maze], &[whole_page()]));
        }
    }
    let minis: Vec<Maze> = [MINI_TRIVIAL, MINI_EASY, MINI_STANDARD, MINI_HARD]
        .into_iter()
        .flat_map(|grid| std::iter::repeat_n(grid, contents.minis_per_level))
        .map(|grid| maze(grid, seeds.next().unwrap_or(seed)))
        .collect();
    for page in minis.chunks(4) {
        pages.extend(sheet(page, &quarter_pages()));
    }
    pages
}

pub fn shaped_book(seed: u64, shape: Shape) -> Option<Vec<Vec<Mark>>> {
    let maze = (0..RETRIES)
        .find_map(|retry| generate_shaped(STANDARD, seed * RETRIES + retry, shape).ok())?;
    Some(sheet(&[maze], &[whole_page()]).to_vec())
}

pub fn zones_for(grid: Grid) -> usize {
    let side = ((grid.width * grid.height) as f64).sqrt();
    ((side / SIDE_PER_ZONE).round() as usize).max(2)
}

fn maze(grid: Grid, seed: u64) -> Maze {
    (0..RETRIES)
        .find_map(|retry| generate(grid, seed * RETRIES + retry, zones_for(grid)).ok())
        .expect("a maze within the retries")
}

fn sheet(mazes: &[Maze], slots: &[Slot]) -> [Vec<Mark>; 2] {
    let front = mazes
        .iter()
        .zip(slots)
        .flat_map(|(maze, slot)| place(maze, &draw(maze), slot))
        .collect();
    let back = mazes
        .iter()
        .zip(slots)
        .flat_map(|(maze, slot)| place(maze, &draw_solution(maze), slot))
        .map(|mark| mark.moved(1.0, |(x, y)| (LETTER.0 - x, y)))
        .collect();
    [front, back]
}

fn place(maze: &Maze, marks: &[Mark], slot: &Slot) -> Vec<Mark> {
    let (columns, rows) = (maze.grid.width as f64, maze.grid.height as f64);
    let scale = (slot.width / columns).min(slot.height / rows);
    let left = slot.left + (slot.width - columns * scale) / 2.0;
    let top = slot.top + (slot.height - rows * scale) / 2.0;
    let at = |(x, y): Point| (left + x * scale, top + y * scale);
    marks.iter().map(|mark| mark.moved(scale, at)).collect()
}

fn whole_page() -> Slot {
    Slot {
        left: MARGIN,
        top: MARGIN,
        width: LETTER.0 - 2.0 * MARGIN,
        height: LETTER.1 - 2.0 * MARGIN,
    }
}

fn quarter_pages() -> [Slot; 4] {
    let page = whole_page();
    let width = (page.width - MINI_GAP) / 2.0;
    let height = (page.height - MINI_GAP) / 2.0;
    let slot = |column: f64, row: f64| Slot {
        left: page.left + column * (width + MINI_GAP),
        top: page.top + row * (height + MINI_GAP),
        width,
        height,
    };
    [
        slot(0.0, 0.0),
        slot(1.0, 0.0),
        slot(0.0, 1.0),
        slot(1.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigger_levels_have_more_zones() {
        let zones: Vec<usize> = [
            MINI_TRIVIAL,
            MINI_EASY,
            MINI_STANDARD,
            MINI_HARD,
            TRIVIAL,
            EASY,
            STANDARD,
            HARD,
        ]
        .into_iter()
        .map(zones_for)
        .collect();
        assert_eq!(zones, vec![2, 3, 3, 3, 3, 4, 5, 6]);
    }

    #[test]
    fn each_solution_starts_behind_its_start() {
        let pages = book(
            0,
            &Contents {
                per_level: 1,
                minis_per_level: 1,
            },
        );
        assert_eq!(pages.len(), 10);
        for sheet in pages.chunks(2) {
            let starts: Vec<Point> = sheet[0]
                .windows(2)
                .filter_map(|pair| match pair {
                    [
                        Mark::Polygon {
                            colour: Colour::START,
                            ..
                        },
                        Mark::Disc { centre, .. },
                    ] => Some(*centre),
                    _ => None,
                })
                .collect();
            let route_points: Vec<Point> = sheet[1]
                .iter()
                .flat_map(|mark| match mark {
                    Mark::Lines { lines, .. } => lines.concat(),
                    _ => Vec::new(),
                })
                .collect();
            assert!(!starts.is_empty());
            for (x, y) in starts {
                assert!(route_points.iter().any(|&(bx, by)| {
                    (x + bx - LETTER.0).abs() < 1e-9 && (y - by).abs() < 1e-9
                }));
            }
        }
    }
}
