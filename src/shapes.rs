use crate::grid::{Cell, Grid};

const LINE_FRAME: (usize, usize) = (4, 6);
const BUBBLE_FRAME: (usize, usize) = (15, 21);
const MARGIN: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub name: &'static str,
    drawing: Drawing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drawing {
    Line(&'static [(usize, usize)]),
    Bubble(&'static [(usize, usize)]),
}

pub const SHAPES: [Shape; 40] = [
    Shape {
        name: "C",
        drawing: Drawing::Line(&[(4, 0), (0, 0), (0, 6), (4, 6)]),
    },
    Shape {
        name: "G",
        drawing: Drawing::Line(&[(4, 0), (0, 0), (0, 6), (4, 6), (4, 3), (2, 3)]),
    },
    Shape {
        name: "J",
        drawing: Drawing::Line(&[(4, 0), (4, 6), (0, 6), (0, 4)]),
    },
    Shape {
        name: "L",
        drawing: Drawing::Line(&[(0, 0), (0, 6), (4, 6)]),
    },
    Shape {
        name: "M",
        drawing: Drawing::Line(&[(0, 6), (0, 0), (2, 3), (4, 0), (4, 6)]),
    },
    Shape {
        name: "N",
        drawing: Drawing::Line(&[(0, 6), (0, 0), (4, 6), (4, 0)]),
    },
    Shape {
        name: "S",
        drawing: Drawing::Line(&[(4, 0), (0, 0), (0, 3), (4, 3), (4, 6), (0, 6)]),
    },
    Shape {
        name: "U",
        drawing: Drawing::Line(&[(0, 0), (0, 6), (4, 6), (4, 0)]),
    },
    Shape {
        name: "V",
        drawing: Drawing::Line(&[(0, 0), (2, 6), (4, 0)]),
    },
    Shape {
        name: "W",
        drawing: Drawing::Line(&[(0, 0), (0, 6), (2, 3), (4, 6), (4, 0)]),
    },
    Shape {
        name: "Z",
        drawing: Drawing::Line(&[(0, 0), (4, 0), (0, 6), (4, 6)]),
    },
    Shape {
        name: "2",
        drawing: Drawing::Line(&[(0, 0), (4, 0), (4, 3), (0, 3), (0, 6), (4, 6)]),
    },
    Shape {
        name: "7",
        drawing: Drawing::Line(&[(0, 0), (4, 0), (1, 6)]),
    },
    Shape {
        name: "spiral",
        drawing: Drawing::Line(&[(0, 0), (4, 0), (4, 6), (0, 6), (0, 2), (2, 2), (2, 4)]),
    },
    Shape {
        name: "bubble-A",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 21),
            (10, 21),
            (10, 14),
            (9, 14),
            (9, 9),
            (10, 9),
            (10, 5),
            (5, 5),
            (5, 9),
            (7, 9),
            (7, 14),
            (5, 14),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-B",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 6),
            (10, 6),
            (10, 5),
            (5, 5),
            (5, 9),
            (10, 9),
            (10, 8),
            (15, 8),
            (15, 13),
            (10, 13),
            (10, 12),
            (5, 12),
            (5, 16),
            (10, 16),
            (10, 15),
            (15, 15),
            (15, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-C",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (5, 5),
            (5, 16),
            (15, 16),
            (15, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-D",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (12, 0),
            (15, 3),
            (15, 18),
            (12, 21),
            (9, 21),
            (9, 16),
            (10, 16),
            (10, 5),
            (5, 5),
            (5, 16),
            (7, 16),
            (7, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-E",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (5, 5),
            (5, 8),
            (12, 8),
            (12, 13),
            (5, 13),
            (5, 16),
            (15, 16),
            (15, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-F",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (5, 5),
            (5, 8),
            (12, 8),
            (12, 13),
            (5, 13),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-G",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (5, 5),
            (5, 16),
            (10, 16),
            (10, 13),
            (8, 13),
            (8, 9),
            (15, 9),
            (15, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-H",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (5, 0),
            (5, 8),
            (10, 8),
            (10, 0),
            (15, 0),
            (15, 21),
            (10, 21),
            (10, 13),
            (5, 13),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-I",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (10, 5),
            (10, 16),
            (15, 16),
            (15, 21),
            (0, 21),
            (0, 16),
            (5, 16),
            (5, 5),
            (0, 5),
        ]),
    },
    Shape {
        name: "bubble-J",
        drawing: Drawing::Bubble(&[
            (10, 0),
            (15, 0),
            (15, 21),
            (0, 21),
            (0, 13),
            (5, 13),
            (5, 16),
            (10, 16),
        ]),
    },
    Shape {
        name: "bubble-K",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (5, 0),
            (5, 7),
            (10, 0),
            (15, 0),
            (9, 10),
            (15, 21),
            (10, 21),
            (5, 14),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-L",
        drawing: Drawing::Bubble(&[(0, 0), (5, 0), (5, 16), (15, 16), (15, 21), (0, 21)]),
    },
    Shape {
        name: "bubble-M",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (5, 0),
            (7, 6),
            (8, 6),
            (10, 0),
            (15, 0),
            (15, 21),
            (10, 21),
            (10, 10),
            (8, 15),
            (7, 15),
            (5, 10),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-N",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (5, 0),
            (10, 9),
            (10, 0),
            (15, 0),
            (15, 21),
            (10, 21),
            (5, 12),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-O",
        drawing: Drawing::Bubble(&[
            (3, 0),
            (12, 0),
            (15, 3),
            (15, 18),
            (12, 21),
            (9, 21),
            (9, 16),
            (10, 16),
            (10, 5),
            (5, 5),
            (5, 16),
            (7, 16),
            (7, 21),
            (3, 21),
            (0, 18),
            (0, 3),
        ]),
    },
    Shape {
        name: "bubble-P",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 13),
            (9, 13),
            (9, 9),
            (10, 9),
            (10, 5),
            (5, 5),
            (5, 9),
            (7, 9),
            (7, 13),
            (5, 13),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-Q",
        drawing: Drawing::Bubble(&[
            (3, 0),
            (12, 0),
            (15, 3),
            (15, 21),
            (9, 21),
            (9, 16),
            (10, 16),
            (10, 5),
            (5, 5),
            (5, 16),
            (7, 16),
            (7, 21),
            (3, 21),
            (0, 18),
            (0, 3),
        ]),
    },
    Shape {
        name: "bubble-R",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (7, 0),
            (7, 5),
            (5, 5),
            (5, 9),
            (10, 9),
            (10, 5),
            (9, 5),
            (9, 0),
            (15, 0),
            (15, 13),
            (11, 13),
            (15, 21),
            (10, 21),
            (6, 13),
            (5, 13),
            (5, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-S",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (5, 5),
            (5, 8),
            (15, 8),
            (15, 21),
            (0, 21),
            (0, 16),
            (10, 16),
            (10, 13),
            (0, 13),
        ]),
    },
    Shape {
        name: "bubble-T",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (10, 5),
            (10, 21),
            (5, 21),
            (5, 5),
            (0, 5),
        ]),
    },
    Shape {
        name: "bubble-U",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (5, 0),
            (5, 16),
            (10, 16),
            (10, 0),
            (15, 0),
            (15, 21),
            (0, 21),
        ]),
    },
    Shape {
        name: "bubble-V",
        drawing: Drawing::Bubble(&[
            (10, 0),
            (15, 0),
            (10, 21),
            (5, 21),
            (0, 0),
            (5, 0),
            (7, 12),
            (8, 12),
        ]),
    },
    Shape {
        name: "bubble-W",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (4, 0),
            (5, 11),
            (7, 5),
            (8, 5),
            (10, 11),
            (11, 0),
            (15, 0),
            (13, 21),
            (10, 21),
            (8, 15),
            (7, 15),
            (5, 21),
            (2, 21),
        ]),
    },
    Shape {
        name: "bubble-X",
        drawing: Drawing::Bubble(&[
            (10, 0),
            (15, 0),
            (11, 10),
            (11, 11),
            (15, 21),
            (10, 21),
            (8, 14),
            (7, 14),
            (5, 21),
            (0, 21),
            (4, 11),
            (4, 10),
            (0, 0),
            (5, 0),
            (7, 7),
            (8, 7),
        ]),
    },
    Shape {
        name: "bubble-Y",
        drawing: Drawing::Bubble(&[
            (10, 0),
            (15, 0),
            (10, 11),
            (10, 21),
            (5, 21),
            (5, 11),
            (0, 0),
            (5, 0),
            (7, 6),
            (8, 6),
        ]),
    },
    Shape {
        name: "bubble-Z",
        drawing: Drawing::Bubble(&[
            (0, 0),
            (15, 0),
            (15, 5),
            (7, 16),
            (15, 16),
            (15, 21),
            (0, 21),
            (0, 16),
            (8, 5),
            (0, 5),
        ]),
    },
];

pub fn shape(name: &str) -> Option<Shape> {
    SHAPES
        .into_iter()
        .find(|shape| shape.name.eq_ignore_ascii_case(name))
}

impl Shape {
    pub fn cells(&self, grid: Grid) -> Vec<Cell> {
        match self.drawing {
            Drawing::Line(corners) => drawn(grid, LINE_FRAME, corners),
            Drawing::Bubble(corners) => {
                let outline = outline(grid, corners);
                outline[2..].iter().chain(&outline[..1]).copied().collect()
            }
        }
    }

    pub fn gap(&self, grid: Grid) -> Option<Cell> {
        match self.drawing {
            Drawing::Line(_) => None,
            Drawing::Bubble(corners) => Some(outline(grid, corners)[1]),
        }
    }
}

fn outline(grid: Grid, corners: &[(usize, usize)]) -> Vec<Cell> {
    let closed: Vec<(usize, usize)> = corners.iter().chain(&corners[..1]).copied().collect();
    let mut cells = drawn(grid, BUBBLE_FRAME, &closed);
    cells.pop();
    cells
}

fn drawn(grid: Grid, frame: (usize, usize), corners: &[(usize, usize)]) -> Vec<Cell> {
    let scale = |at: usize, frame: usize, side: usize| {
        let room = side.saturating_sub(2 * MARGIN + 1);
        MARGIN + (at * room + frame / 2) / frame
    };
    let corners: Vec<Cell> = corners
        .iter()
        .map(|&(x, y)| Cell {
            x: scale(x, frame.0, grid.width),
            y: scale(y, frame.1, grid.height),
        })
        .collect();
    let mut cells = corners[..1].to_vec();
    for cell in corners.windows(2).flat_map(|pair| line(pair[0], pair[1])) {
        if cells.len() > 1 && cells[cells.len() - 2] == cell {
            cells.pop();
        } else {
            cells.push(cell);
        }
    }
    cells
}

fn line(from: Cell, to: Cell) -> Vec<Cell> {
    let across = from.x.abs_diff(to.x);
    let down = from.y.abs_diff(to.y);
    let (mut x, mut y) = (0, 0);
    let mut cells = Vec::new();
    while x < across || y < down {
        if y == down || (x < across && (1 + 2 * x) * down < (1 + 2 * y) * across) {
            x += 1;
        } else {
            y += 1;
        }
        cells.push(Cell {
            x: if to.x > from.x {
                from.x + x
            } else {
                from.x - x
            },
            y: if to.y > from.y {
                from.y + y
            } else {
                from.y - y
            },
        });
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Dir;
    use std::collections::HashSet;

    const GRID: Grid = Grid {
        width: 20,
        height: 26,
    };

    #[test]
    fn shape_cells_are_neighbours_and_never_repeat() {
        for shape in SHAPES {
            let cells = shape.cells(GRID);
            assert!(
                cells
                    .windows(2)
                    .all(|pair| Dir::between(pair[0], pair[1]).is_some()),
                "{}",
                shape.name
            );
            assert_eq!(
                cells.iter().collect::<HashSet<_>>().len(),
                cells.len(),
                "{}",
                shape.name
            );
            if let Some(gap) = shape.gap(GRID) {
                let (start, finish) = (cells[0], cells[cells.len() - 1]);
                assert_eq!(
                    Dir::between(gap, start).map(Dir::opposite),
                    Dir::between(gap, finish),
                    "{}",
                    shape.name
                );
            }
        }
    }

    #[test]
    fn shapes_are_found_by_name() {
        assert_eq!(shape("s").map(|shape| shape.name), Some("S"));
        assert_eq!(shape("Spiral").map(|shape| shape.name), Some("spiral"));
        assert_eq!(shape("Q"), None);
    }
}
