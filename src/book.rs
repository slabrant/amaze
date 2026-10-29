#[cfg(test)]
use crate::drawing::Colour;
use crate::drawing::{Ink, Mark, Point, draw, draw_solution};
use crate::generate::generate;
use crate::grid::Grid;
use crate::maze::Maze;
use crate::pdf::LETTER;
use std::sync::atomic::{AtomicUsize, Ordering};

const MILLIMETRE: f64 = 72.0 / 25.4;
const MARGIN: f64 = 15.0 * MILLIMETRE;
const GAP: f64 = 10.0 * MILLIMETRE;
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

pub const HALF_TRIVIAL: Grid = Grid {
    width: 15,
    height: 10,
};
pub const HALF_EASY: Grid = Grid {
    width: 19,
    height: 12,
};
pub const HALF_STANDARD: Grid = Grid {
    width: 23,
    height: 14,
};
pub const HALF_HARD: Grid = Grid {
    width: 27,
    height: 17,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layout {
    OneUp,
    TwoUp,
}

impl Layout {
    fn levels(self) -> [Grid; 4] {
        match self {
            Layout::OneUp => [TRIVIAL, EASY, STANDARD, HARD],
            Layout::TwoUp => [HALF_TRIVIAL, HALF_EASY, HALF_STANDARD, HALF_HARD],
        }
    }

    fn slots(self) -> Vec<Slot> {
        match self {
            Layout::OneUp => vec![whole_page()],
            Layout::TwoUp => half_pages().to_vec(),
        }
    }
}

#[derive(Clone)]
struct Slot {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

pub fn book(seed: u64, layout: Layout, per_level: usize, ink: Ink) -> Vec<Vec<Mark>> {
    let jobs: Vec<(Grid, u64)> = layout
        .levels()
        .into_iter()
        .flat_map(|grid| std::iter::repeat_n(grid, per_level))
        .zip(seed..)
        .collect();
    let slots = layout.slots();
    mazes(&jobs)
        .chunks(slots.len())
        .flat_map(|page| sheet(page, &slots, ink))
        .collect()
}

fn mazes(jobs: &[(Grid, u64)]) -> Vec<Maze> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(1, |count| count.get());
    let mut made: Vec<(usize, Maze)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut made = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(grid, seed)) = jobs.get(index) else {
                            break;
                        };
                        made.push((index, maze(grid, seed)));
                    }
                    made
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("maze worker finishes"))
            .collect()
    });
    made.sort_by_key(|&(index, _)| index);
    made.into_iter().map(|(_, maze)| maze).collect()
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

fn sheet(mazes: &[Maze], slots: &[Slot], ink: Ink) -> [Vec<Mark>; 2] {
    let front = mazes
        .iter()
        .zip(slots)
        .flat_map(|(maze, slot)| place(maze, &draw(maze, ink), slot))
        .collect();
    let back = mazes
        .iter()
        .zip(slots)
        .flat_map(|(maze, slot)| place(maze, &draw_solution(maze, ink), slot))
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

fn half_pages() -> [Slot; 2] {
    let page = whole_page();
    let height = (page.height - GAP) / 2.0;
    let slot = |row: f64| Slot {
        left: page.left,
        top: page.top + row * (height + GAP),
        width: page.width,
        height,
    };
    [slot(0.0), slot(1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_up_book_has_a_back_for_every_page_of_two() {
        assert_eq!(book(0, Layout::TwoUp, 2, Ink::Colour).len(), 8);
    }

    #[test]
    fn bigger_levels_have_more_zones() {
        let zones: Vec<usize> = [
            HALF_TRIVIAL,
            HALF_EASY,
            HALF_STANDARD,
            HALF_HARD,
            TRIVIAL,
            EASY,
            STANDARD,
            HARD,
        ]
        .into_iter()
        .map(zones_for)
        .collect();
        assert_eq!(zones, vec![3, 3, 4, 5, 3, 4, 5, 6]);
    }

    #[test]
    fn each_solution_starts_behind_its_start() {
        let pages = book(0, Layout::OneUp, 1, Ink::Colour);
        assert_eq!(pages.len(), 8);
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
