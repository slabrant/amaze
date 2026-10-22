use crate::drawing::{Colour, Mark, Point, draw, draw_solution};
use crate::generate::generate;
use crate::grid::Grid;
use crate::maze::Maze;
use crate::pdf::A4;

const MILLIMETRE: f64 = 72.0 / 25.4;
const MARGIN: f64 = 15.0 * MILLIMETRE;
const SMALL_GAP: f64 = 10.0 * MILLIMETRE;
const CELLS_PER_ZONE: usize = 400;
const RETRIES: u64 = 20;

pub const TRIVIAL: Grid = Grid {
    width: 10,
    height: 15,
};
pub const EASY: Grid = Grid {
    width: 15,
    height: 22,
};
pub const STANDARD: Grid = Grid {
    width: 20,
    height: 30,
};
pub const HARD: Grid = Grid {
    width: 26,
    height: 38,
};
pub const SMALL: Grid = Grid {
    width: 10,
    height: 14,
};

pub struct Contents {
    pub per_level: usize,
    pub small_pages: usize,
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
    for _ in 0..contents.small_pages {
        let mazes: Vec<Maze> = (0..4)
            .map(|_| maze(SMALL, seeds.next().unwrap_or(seed)))
            .collect();
        pages.extend(sheet(&mazes, &quarter_pages()));
    }
    pages
}

pub fn zones_for(grid: Grid) -> usize {
    2 + grid.width * grid.height / CELLS_PER_ZONE
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
        .flat_map(|(maze, slot)| place(maze, &draw_solution(maze, Colour::SOLUTION), slot))
        .map(|mark| mark.moved(1.0, |(x, y)| (A4.0 - x, y)))
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
        width: A4.0 - 2.0 * MARGIN,
        height: A4.1 - 2.0 * MARGIN,
    }
}

fn quarter_pages() -> [Slot; 4] {
    let page = whole_page();
    let width = (page.width - SMALL_GAP) / 2.0;
    let height = (page.height - SMALL_GAP) / 2.0;
    let slot = |column: f64, row: f64| Slot {
        left: page.left + column * (width + SMALL_GAP),
        top: page.top + row * (height + SMALL_GAP),
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
    fn each_front_page_has_a_mirrored_back() {
        let pages = book(
            0,
            &Contents {
                per_level: 1,
                small_pages: 1,
            },
        );
        assert_eq!(pages.len(), 10);
        let dots = |page: &Vec<Mark>| -> Vec<Point> {
            page.iter()
                .filter_map(|mark| match mark {
                    Mark::Disc { centre, .. } => Some(*centre),
                    _ => None,
                })
                .collect()
        };
        for sheet in pages.chunks(2) {
            let front = dots(&sheet[0]);
            let back = dots(&sheet[1]);
            assert_eq!(front.len(), back.len());
            for ((fx, fy), (bx, by)) in front.into_iter().zip(back) {
                assert!((fx + bx - A4.0).abs() < 1e-9);
                assert!((fy - by).abs() < 1e-9);
            }
        }
    }
}
