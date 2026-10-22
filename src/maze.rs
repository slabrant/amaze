use crate::grid::{Cell, Dir, Grid};
use std::collections::HashSet;

pub type PieceId = usize;

#[derive(Clone, Debug)]
pub struct Piece {
    pub cell: Cell,
    pub links: Vec<(Dir, PieceId)>,
}

impl Piece {
    pub fn has_side(&self, dir: Dir) -> bool {
        self.links.iter().any(|&(side, _)| side == dir)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    Empty,
    Path,
    Crossing,
    Junction,
}

#[derive(Clone, Copy, Debug)]
pub struct StepCell {
    pub cell: Cell,
    pub on_top: bool,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub from: PieceId,
    pub cells: Vec<StepCell>,
    pub to: PieceId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepError {
    TooShort,
    OutOfGrid,
    NotNeighbours,
    RepeatedCell,
    CellTaken,
    EndNotJoinable,
    SideTaken,
}

#[derive(Clone, Debug)]
pub struct Maze {
    pub grid: Grid,
    pub start: Cell,
    pub finish: Cell,
    pieces: Vec<Piece>,
    layers: Vec<Vec<PieceId>>,
}

impl Maze {
    pub fn with_loop(grid: Grid, cells: &[Cell]) -> Result<Maze, StepError> {
        if cells.len() < 4 {
            return Err(StepError::TooShort);
        }
        if cells.iter().any(|&cell| !grid.contains(cell)) {
            return Err(StepError::OutOfGrid);
        }
        if cells.iter().collect::<HashSet<_>>().len() != cells.len() {
            return Err(StepError::RepeatedCell);
        }
        let mut maze = Maze {
            grid,
            start: cells[0],
            finish: cells[0],
            pieces: Vec::new(),
            layers: vec![Vec::new(); grid.width * grid.height],
        };
        let ids: Vec<PieceId> = cells
            .iter()
            .map(|&cell| maze.add_piece(cell, true))
            .collect();
        for (i, &id) in ids.iter().enumerate() {
            let next = ids[(i + 1) % ids.len()];
            if Dir::between(maze.pieces[id].cell, maze.pieces[next].cell).is_none() {
                return Err(StepError::NotNeighbours);
            }
            maze.link(id, next);
        }
        Ok(maze)
    }

    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    pub fn piece(&self, id: PieceId) -> &Piece {
        &self.pieces[id]
    }

    pub fn pieces_in(&self, cell: Cell) -> &[PieceId] {
        &self.layers[self.grid.index(cell)]
    }

    pub fn content(&self, cell: Cell) -> Content {
        match self.pieces_in(cell) {
            [] => Content::Empty,
            [id] if self.pieces[*id].links.len() > 2 => Content::Junction,
            [_] => Content::Path,
            _ => Content::Crossing,
        }
    }

    pub fn joined(&self, a: Cell, b: Cell) -> bool {
        self.pieces_in(a).iter().any(|&id| {
            self.pieces[id]
                .links
                .iter()
                .any(|&(_, other)| self.pieces[other].cell == b)
        })
    }

    pub fn check_step(&self, step: &Step) -> Result<(), StepError> {
        let (Some(first), Some(last)) = (step.cells.first(), step.cells.last()) else {
            return Err(StepError::TooShort);
        };
        if step.cells.iter().any(|c| !self.grid.contains(c.cell)) {
            return Err(StepError::OutOfGrid);
        }
        if step
            .cells
            .windows(2)
            .any(|pair| Dir::between(pair[0].cell, pair[1].cell).is_none())
        {
            return Err(StepError::NotNeighbours);
        }
        let from = &self.pieces[step.from];
        let to = &self.pieces[step.to];
        let mut seen = HashSet::from([from.cell, to.cell]);
        if step.cells.iter().any(|c| !seen.insert(c.cell)) {
            return Err(StepError::RepeatedCell);
        }
        if step
            .cells
            .iter()
            .any(|c| !matches!(self.content(c.cell), Content::Empty | Content::Path))
        {
            return Err(StepError::CellTaken);
        }
        for end in [from, to] {
            if self.pieces_in(end.cell).len() != 1 {
                return Err(StepError::EndNotJoinable);
            }
        }
        let (Some(out), Some(back)) = (
            Dir::between(from.cell, first.cell),
            Dir::between(to.cell, last.cell),
        ) else {
            return Err(StepError::NotNeighbours);
        };
        if from.has_side(out) || to.has_side(back) || (step.from == step.to && out == back) {
            return Err(StepError::SideTaken);
        }
        Ok(())
    }

    pub fn make_step(&mut self, step: &Step) -> Result<(), StepError> {
        self.check_step(step)?;
        let ids: Vec<PieceId> = step
            .cells
            .iter()
            .map(|c| self.add_piece(c.cell, c.on_top))
            .collect();
        for pair in ids.windows(2) {
            self.link(pair[0], pair[1]);
        }
        self.link(step.from, ids[0]);
        self.link(ids[ids.len() - 1], step.to);
        Ok(())
    }

    fn add_piece(&mut self, cell: Cell, on_top: bool) -> PieceId {
        let id = self.pieces.len();
        self.pieces.push(Piece {
            cell,
            links: Vec::new(),
        });
        let layer = &mut self.layers[self.grid.index(cell)];
        if on_top {
            layer.push(id);
        } else {
            layer.insert(0, id);
        }
        id
    }

    fn link(&mut self, a: PieceId, b: PieceId) {
        let dir = Dir::between(self.pieces[a].cell, self.pieces[b].cell)
            .expect("linked pieces are in neighbouring cells");
        self.pieces[a].links.push((dir, b));
        self.pieces[b].links.push((dir.opposite(), a));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRID: Grid = Grid {
        width: 5,
        height: 5,
    };

    fn cell(x: usize, y: usize) -> Cell {
        Cell { x, y }
    }

    fn over(cells: &[Cell]) -> Vec<StepCell> {
        cells
            .iter()
            .map(|&cell| StepCell { cell, on_top: true })
            .collect()
    }

    fn square() -> Maze {
        let cells = [
            cell(1, 1),
            cell(2, 1),
            cell(3, 1),
            cell(3, 2),
            cell(3, 3),
            cell(2, 3),
            cell(1, 3),
            cell(1, 2),
        ];
        Maze::with_loop(GRID, &cells).unwrap()
    }

    fn id_at(maze: &Maze, at: Cell) -> PieceId {
        maze.pieces_in(at)[0]
    }

    #[test]
    fn loop_cells_must_be_neighbours() {
        let cells = [cell(0, 0), cell(1, 0), cell(1, 1), cell(3, 3)];
        assert_eq!(
            Maze::with_loop(GRID, &cells).unwrap_err(),
            StepError::NotNeighbours
        );
    }

    #[test]
    fn step_across_the_loop_makes_a_crossing_and_junctions() {
        let mut maze = square();
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[
                cell(2, 0),
                cell(3, 0),
                cell(4, 0),
                cell(4, 1),
                cell(4, 2),
                cell(3, 2),
            ]),
            to: id_at(&maze, cell(2, 3)),
        };
        assert_eq!(maze.check_step(&step), Err(StepError::NotNeighbours));
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 2)]),
            to: id_at(&maze, cell(2, 3)),
        };
        maze.make_step(&step).unwrap();
        assert_eq!(maze.content(cell(2, 1)), Content::Junction);
        assert_eq!(maze.content(cell(2, 3)), Content::Junction);
        assert!(maze.joined(cell(2, 1), cell(2, 2)));
    }

    #[test]
    fn step_over_a_path_makes_a_crossing() {
        let mut maze = square();
        let step = Step {
            from: id_at(&maze, cell(1, 1)),
            cells: over(&[cell(0, 1), cell(0, 2), cell(1, 2), cell(2, 2)]),
            to: id_at(&maze, cell(3, 2)),
        };
        maze.make_step(&step).unwrap();
        assert_eq!(maze.content(cell(1, 2)), Content::Crossing);
    }

    #[test]
    fn step_cannot_cross_a_junction_or_a_crossing() {
        let mut maze = square();
        let step = Step {
            from: id_at(&maze, cell(1, 1)),
            cells: over(&[cell(0, 1), cell(0, 2), cell(1, 2), cell(2, 2)]),
            to: id_at(&maze, cell(3, 2)),
        };
        maze.make_step(&step).unwrap();
        let across_crossing = Step {
            from: id_at(&maze, cell(1, 3)),
            cells: over(&[cell(1, 2)]),
            to: id_at(&maze, cell(1, 1)),
        };
        assert_eq!(maze.check_step(&across_crossing), Err(StepError::CellTaken));
        let across_junction = Step {
            from: id_at(&maze, cell(3, 3)),
            cells: over(&[cell(4, 3), cell(4, 2), cell(3, 2), cell(3, 1)]),
            to: id_at(&maze, cell(2, 1)),
        };
        assert_eq!(maze.check_step(&across_junction), Err(StepError::CellTaken));
    }

    #[test]
    fn step_cannot_end_on_a_crossing() {
        let mut maze = square();
        let step = Step {
            from: id_at(&maze, cell(1, 1)),
            cells: over(&[cell(0, 1), cell(0, 2), cell(1, 2), cell(2, 2)]),
            to: id_at(&maze, cell(3, 2)),
        };
        maze.make_step(&step).unwrap();
        let onto_crossing = Step {
            from: id_at(&maze, cell(1, 3)),
            cells: over(&[cell(0, 3)]),
            to: id_at(&maze, cell(1, 2)),
        };
        assert_eq!(
            maze.check_step(&onto_crossing),
            Err(StepError::EndNotJoinable)
        );
    }

    #[test]
    fn step_cannot_reuse_a_side() {
        let maze = square();
        let step = Step {
            from: id_at(&maze, cell(1, 1)),
            cells: over(&[cell(2, 1)]),
            to: id_at(&maze, cell(3, 1)),
        };
        assert_eq!(maze.check_step(&step), Err(StepError::SideTaken));
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 0), cell(1, 0), cell(0, 0), cell(0, 1)]),
            to: id_at(&maze, cell(1, 1)),
        };
        assert_eq!(maze.check_step(&step), Ok(()));
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 2)]),
            to: id_at(&maze, cell(2, 1)),
        };
        assert_eq!(maze.check_step(&step), Err(StepError::SideTaken));
    }
}
