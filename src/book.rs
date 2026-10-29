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
const LEVEL_SIDES: [f64; 4] = [12.0, 16.0, 20.0, 24.0];
const LEVEL_DOT_SPACINGS: [f64; 4] = [2.2, 2.0, 1.85, 1.7];

struct Slot {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

pub fn book(seed: u64, per_page: usize, ink: Ink) -> Vec<Vec<Mark>> {
    let slots = slots(per_page);
    let jobs: Vec<(Grid, u64)> = grids(&slots[0])
        .into_iter()
        .flat_map(|grid| std::iter::repeat_n(grid, per_page))
        .zip(seed..)
        .collect();
    let spacings = LEVEL_DOT_SPACINGS
        .into_iter()
        .flat_map(|spacing| std::iter::repeat_n(spacing, per_page));
    let mazes: Vec<(Maze, f64)> = mazes(&jobs).into_iter().zip(spacings).collect();
    mazes
        .chunks(per_page)
        .flat_map(|page| sheet(page, &slots, ink))
        .collect()
}

pub fn levels(per_page: usize) -> [Grid; 4] {
    grids(&slots(per_page)[0])
}

fn grids(slot: &Slot) -> [Grid; 4] {
    let page = whole_page();
    let (short, long) = (slot.width.min(slot.height), slot.width.max(slot.height));
    let shrink = (short / page.width.min(page.height)).sqrt();
    LEVEL_SIDES.map(|side| {
        let across = (side * shrink).round().max(4.0);
        let along = (long * across / short).floor().max(4.0);
        if slot.width <= slot.height {
            Grid {
                width: across as usize,
                height: along as usize,
            }
        } else {
            Grid {
                width: along as usize,
                height: across as usize,
            }
        }
    })
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

fn sheet(mazes: &[(Maze, f64)], slots: &[Slot], ink: Ink) -> [Vec<Mark>; 2] {
    let front = mazes
        .iter()
        .zip(slots)
        .flat_map(|((maze, spacing), slot)| place(maze, &draw(maze, ink), slot, *spacing))
        .collect();
    let back = mazes
        .iter()
        .zip(slots)
        .flat_map(|((maze, spacing), slot)| place(maze, &draw_solution(maze, ink), slot, *spacing))
        .map(|mark| mark.moved(1.0, |(x, y)| (LETTER.0 - x, y)))
        .collect();
    [front, back]
}

fn place(maze: &Maze, marks: &[Mark], slot: &Slot, dots: f64) -> Vec<Mark> {
    let (columns, rows) = (maze.grid.width as f64, maze.grid.height as f64);
    let scale = (slot.width / columns).min(slot.height / rows);
    let left = slot.left + (slot.width - columns * scale) / 2.0;
    let top = slot.top + (slot.height - rows * scale) / 2.0;
    let at = |(x, y): Point| (left + x * scale, top + y * scale);
    marks
        .iter()
        .map(|mark| match mark.moved(scale, at) {
            Mark::Shade {
                points, darkness, ..
            } => Mark::Shade {
                points,
                darkness,
                spacing: dots,
            },
            placed => placed,
        })
        .collect()
}

fn whole_page() -> Slot {
    Slot {
        left: MARGIN,
        top: MARGIN,
        width: LETTER.0 - 2.0 * MARGIN,
        height: LETTER.1 - 2.0 * MARGIN,
    }
}

fn slots(per_page: usize) -> Vec<Slot> {
    let page = whole_page();
    let size = |columns: usize| {
        let rows = per_page.div_ceil(columns);
        (
            (page.width - GAP * (columns - 1) as f64) / columns as f64,
            (page.height - GAP * (rows - 1) as f64) / rows as f64,
        )
    };
    let columns = (1..=per_page)
        .rev()
        .max_by(|&a, &b| {
            let (aw, ah) = size(a);
            let (bw, bh) = size(b);
            aw.min(ah).total_cmp(&bw.min(bh))
        })
        .expect("at least one maze to a page");
    let (width, height) = size(columns);
    (0..per_page)
        .map(|i| Slot {
            left: page.left + (i % columns) as f64 * (width + GAP),
            top: page.top + (i / columns) as f64 * (height + GAP),
            width,
            height,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_book_has_eight_pages() {
        assert_eq!(book(0, 2, Ink::Colour).len(), 8);
    }

    #[test]
    fn levels_grow_and_fill_their_slot() {
        assert_eq!(
            levels(2),
            [
                Grid {
                    width: 15,
                    height: 10
                },
                Grid {
                    width: 20,
                    height: 13
                },
                Grid {
                    width: 24,
                    height: 16
                },
                Grid {
                    width: 29,
                    height: 19
                },
            ]
        );
        for per_page in 1..=6 {
            let zones = levels(per_page).map(zones_for);
            assert!(zones.windows(2).all(|pair| pair[0] <= pair[1]));
        }
    }

    #[test]
    fn four_to_a_page_is_two_by_two() {
        let slots = slots(4);
        assert_eq!(slots[0].top, slots[1].top);
        assert_eq!(slots[0].left, slots[2].left);
    }

    #[test]
    fn each_solution_starts_behind_its_start() {
        let pages = book(0, 1, Ink::Colour);
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
