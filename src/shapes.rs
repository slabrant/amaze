use crate::grid::{Cell, Grid};

const FRAME: (usize, usize) = (4, 6);
const MARGIN: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    pub name: &'static str,
    corners: &'static [(usize, usize)],
}

pub const SHAPES: [Shape; 14] = [
    Shape {
        name: "C",
        corners: &[(4, 0), (0, 0), (0, 6), (4, 6)],
    },
    Shape {
        name: "G",
        corners: &[(4, 0), (0, 0), (0, 6), (4, 6), (4, 3), (2, 3)],
    },
    Shape {
        name: "J",
        corners: &[(4, 0), (4, 6), (0, 6), (0, 4)],
    },
    Shape {
        name: "L",
        corners: &[(0, 0), (0, 6), (4, 6)],
    },
    Shape {
        name: "M",
        corners: &[(0, 6), (0, 0), (2, 3), (4, 0), (4, 6)],
    },
    Shape {
        name: "N",
        corners: &[(0, 6), (0, 0), (4, 6), (4, 0)],
    },
    Shape {
        name: "S",
        corners: &[(4, 0), (0, 0), (0, 3), (4, 3), (4, 6), (0, 6)],
    },
    Shape {
        name: "U",
        corners: &[(0, 0), (0, 6), (4, 6), (4, 0)],
    },
    Shape {
        name: "V",
        corners: &[(0, 0), (2, 6), (4, 0)],
    },
    Shape {
        name: "W",
        corners: &[(0, 0), (0, 6), (2, 3), (4, 6), (4, 0)],
    },
    Shape {
        name: "Z",
        corners: &[(0, 0), (4, 0), (0, 6), (4, 6)],
    },
    Shape {
        name: "2",
        corners: &[(0, 0), (4, 0), (4, 3), (0, 3), (0, 6), (4, 6)],
    },
    Shape {
        name: "7",
        corners: &[(0, 0), (4, 0), (1, 6)],
    },
    Shape {
        name: "spiral",
        corners: &[(0, 0), (4, 0), (4, 6), (0, 6), (0, 2), (2, 2), (2, 4)],
    },
];

pub fn shape(name: &str) -> Option<Shape> {
    SHAPES
        .into_iter()
        .find(|shape| shape.name.eq_ignore_ascii_case(name))
}

impl Shape {
    pub fn cells(&self, grid: Grid) -> Vec<Cell> {
        let scale = |at: usize, frame: usize, side: usize| {
            let room = side.saturating_sub(2 * MARGIN + 1);
            MARGIN + (at * room + frame / 2) / frame
        };
        let corners: Vec<Cell> = self
            .corners
            .iter()
            .map(|&(x, y)| Cell {
                x: scale(x, FRAME.0, grid.width),
                y: scale(y, FRAME.1, grid.height),
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
        }
    }

    #[test]
    fn shapes_are_found_by_name() {
        assert_eq!(shape("s").map(|shape| shape.name), Some("S"));
        assert_eq!(shape("Spiral").map(|shape| shape.name), Some("spiral"));
        assert_eq!(shape("Q"), None);
    }
}
