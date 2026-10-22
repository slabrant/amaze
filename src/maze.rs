use crate::grid::{Cell, Dir, Grid};
use std::collections::{HashSet, VecDeque};

pub type PieceId = usize;

pub const SHORTEST_CIRCUIT: usize = 8;

#[derive(Clone, Debug)]
pub struct Piece {
    pub cell: Cell,
    pub zone: usize,
    pub links: Vec<(Dir, PieceId)>,
}

impl Piece {
    pub fn has_side(&self, dir: Dir) -> bool {
        self.links.iter().any(|&(side, _)| side == dir)
    }

    pub fn is_straight(&self) -> bool {
        matches!(self.links.as_slice(), [(a, _), (b, _)] if *a == b.opposite())
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
    pub zone: usize,
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
    TurnInCrossing,
    WrongZone,
    CircuitTooShort,
}

#[derive(Clone, Debug)]
pub struct Maze {
    pub grid: Grid,
    pub start: Cell,
    pub finish: Cell,
    zones: usize,
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
            zones: 1,
            pieces: Vec::new(),
            layers: vec![Vec::new(); grid.width * grid.height],
        };
        let ids: Vec<PieceId> = cells
            .iter()
            .map(|&cell| maze.add_piece(cell, 0, true))
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

    pub fn zones(&self) -> usize {
        self.zones
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

    pub fn solution(&self) -> Vec<PieceId> {
        let mut came_from: Vec<Option<PieceId>> = vec![None; self.pieces.len()];
        let mut queue: VecDeque<PieceId> = self.pieces_in(self.start).iter().copied().collect();
        let mut seen: HashSet<PieceId> = queue.iter().copied().collect();
        while let Some(id) = queue.pop_front() {
            if self.pieces[id].cell == self.finish {
                let mut route = vec![id];
                while let Some(before) = came_from[route[route.len() - 1]] {
                    route.push(before);
                }
                route.reverse();
                return route;
            }
            for &(_, next) in &self.pieces[id].links {
                if seen.insert(next) {
                    came_from[next] = Some(id);
                    queue.push_back(next);
                }
            }
        }
        Vec::new()
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
        let route: Vec<Cell> = std::iter::once(from.cell)
            .chain(step.cells.iter().map(|c| c.cell))
            .chain([to.cell])
            .collect();
        for (i, c) in step.cells.iter().enumerate() {
            if let [crossed] = self.pieces_in(c.cell) {
                let goes_straight = Dir::between(c.cell, route[i]).map(Dir::opposite)
                    == Dir::between(c.cell, route[i + 2]);
                if !goes_straight || !self.pieces[*crossed].is_straight() {
                    return Err(StepError::TurnInCrossing);
                }
            }
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
        let starts_zone = step.zone == self.zones;
        let zone_fits = if starts_zone {
            step.from == step.to
                && from.zone + 1 == step.zone
                && self.content(from.cell) == Content::Path
        } else {
            from.zone == step.zone && to.zone == step.zone
        };
        if !zone_fits {
            return Err(StepError::WrongZone);
        }
        let route_needed = SHORTEST_CIRCUIT.saturating_sub(step.cells.len() + 1);
        if self.route_shorter_than(step.from, step.to, route_needed) {
            return Err(StepError::CircuitTooShort);
        }
        Ok(())
    }

    fn route_shorter_than(&self, from: PieceId, to: PieceId, length: usize) -> bool {
        let mut reached = HashSet::from([from]);
        let mut edge = vec![from];
        for _ in 0..length {
            if edge.contains(&to) {
                return true;
            }
            edge = edge
                .iter()
                .flat_map(|&id| self.pieces[id].links.iter().map(|&(_, next)| next))
                .filter(|&next| reached.insert(next))
                .collect();
        }
        false
    }

    pub fn make_step(&mut self, step: &Step) -> Result<(), StepError> {
        self.check_step(step)?;
        let ids: Vec<PieceId> = step
            .cells
            .iter()
            .map(|c| self.add_piece(c.cell, step.zone, c.on_top))
            .collect();
        for pair in ids.windows(2) {
            self.link(pair[0], pair[1]);
        }
        self.link(step.from, ids[0]);
        self.link(ids[ids.len() - 1], step.to);
        self.zones = self.zones.max(step.zone + 1);
        Ok(())
    }

    fn add_piece(&mut self, cell: Cell, zone: usize, on_top: bool) -> PieceId {
        let id = self.pieces.len();
        self.pieces.push(Piece {
            cell,
            zone,
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
    fn step_makes_junctions_at_its_ends() {
        let mut maze = square();
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 0), cell(3, 0)]),
            to: id_at(&maze, cell(2, 3)),
            zone: 0,
        };
        assert_eq!(maze.check_step(&step), Err(StepError::NotNeighbours));
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[
                cell(2, 0),
                cell(1, 0),
                cell(0, 0),
                cell(0, 1),
                cell(0, 2),
                cell(0, 3),
                cell(0, 4),
                cell(1, 4),
                cell(2, 4),
            ]),
            to: id_at(&maze, cell(2, 3)),
            zone: 0,
        };
        maze.make_step(&step).unwrap();
        assert_eq!(maze.content(cell(2, 1)), Content::Junction);
        assert_eq!(maze.content(cell(2, 3)), Content::Junction);
        assert!(maze.joined(cell(2, 1), cell(2, 0)));
        assert!(maze.joined(cell(2, 4), cell(2, 3)));
    }

    #[test]
    fn step_over_a_path_makes_a_crossing() {
        let mut maze = square();
        let step = Step {
            from: id_at(&maze, cell(1, 1)),
            cells: over(&[cell(0, 1), cell(0, 2), cell(1, 2), cell(2, 2)]),
            to: id_at(&maze, cell(3, 2)),
            zone: 0,
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
            zone: 0,
        };
        maze.make_step(&step).unwrap();
        let across_crossing = Step {
            from: id_at(&maze, cell(1, 3)),
            cells: over(&[cell(1, 2)]),
            to: id_at(&maze, cell(1, 1)),
            zone: 0,
        };
        assert_eq!(maze.check_step(&across_crossing), Err(StepError::CellTaken));
        let across_junction = Step {
            from: id_at(&maze, cell(3, 3)),
            cells: over(&[cell(4, 3), cell(4, 2), cell(3, 2), cell(3, 1)]),
            to: id_at(&maze, cell(2, 1)),
            zone: 0,
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
            zone: 0,
        };
        maze.make_step(&step).unwrap();
        let onto_crossing = Step {
            from: id_at(&maze, cell(1, 3)),
            cells: over(&[cell(0, 3)]),
            to: id_at(&maze, cell(1, 2)),
            zone: 0,
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
            zone: 0,
        };
        assert_eq!(maze.check_step(&step), Err(StepError::SideTaken));
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[
                cell(2, 0),
                cell(1, 0),
                cell(0, 0),
                cell(0, 1),
                cell(0, 2),
                cell(0, 3),
            ]),
            to: id_at(&maze, cell(1, 3)),
            zone: 0,
        };
        assert_eq!(maze.check_step(&step), Ok(()));
        let step = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 2)]),
            to: id_at(&maze, cell(2, 1)),
            zone: 0,
        };
        assert_eq!(maze.check_step(&step), Err(StepError::SideTaken));
    }

    #[test]
    fn new_zone_starts_from_one_cell_of_the_last_zone() {
        let mut maze = square();
        let from = id_at(&maze, cell(2, 1));
        let loop_off = over(&[
            cell(2, 0),
            cell(1, 0),
            cell(0, 0),
            cell(0, 1),
            cell(0, 2),
            cell(1, 2),
            cell(2, 2),
        ]);
        let into_new_zone = Step {
            from,
            cells: loop_off[..4].to_vec(),
            to: id_at(&maze, cell(1, 1)),
            zone: 1,
        };
        assert_eq!(maze.check_step(&into_new_zone), Err(StepError::WrongZone));
        let skipping_a_zone = Step {
            from,
            cells: loop_off.clone(),
            to: from,
            zone: 2,
        };
        assert_eq!(maze.check_step(&skipping_a_zone), Err(StepError::WrongZone));
        let new_zone = Step {
            from,
            cells: loop_off,
            to: from,
            zone: 1,
        };
        maze.make_step(&new_zone).unwrap();
        assert_eq!(maze.zones(), 2);
        assert_eq!(maze.content(cell(2, 1)), Content::Junction);
        let across_zones = Step {
            from: id_at(&maze, cell(0, 2)),
            cells: over(&[cell(0, 3)]),
            to: id_at(&maze, cell(1, 3)),
            zone: 1,
        };
        assert_eq!(maze.check_step(&across_zones), Err(StepError::WrongZone));
    }

    #[test]
    fn paths_go_straight_through_a_crossing() {
        let maze = square();
        let turning_over_a_path = Step {
            from: id_at(&maze, cell(1, 1)),
            cells: over(&[cell(0, 1), cell(0, 2), cell(1, 2)]),
            to: id_at(&maze, cell(1, 3)),
            zone: 0,
        };
        assert_eq!(
            maze.check_step(&turning_over_a_path),
            Err(StepError::TurnInCrossing)
        );
        let over_a_turning_path = Step {
            from: id_at(&maze, cell(3, 2)),
            cells: over(&[cell(4, 2), cell(4, 3), cell(3, 3)]),
            to: id_at(&maze, cell(2, 3)),
            zone: 0,
        };
        assert_eq!(
            maze.check_step(&over_a_turning_path),
            Err(StepError::TurnInCrossing)
        );
    }

    #[test]
    fn circuit_must_be_long_enough() {
        let maze = square();
        let short = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 0), cell(3, 0), cell(4, 0), cell(4, 1)]),
            to: id_at(&maze, cell(3, 1)),
            zone: 0,
        };
        assert_eq!(maze.check_step(&short), Err(StepError::CircuitTooShort));
        let long_enough = Step {
            from: id_at(&maze, cell(2, 1)),
            cells: over(&[cell(2, 0), cell(3, 0), cell(4, 0), cell(4, 1), cell(4, 2)]),
            to: id_at(&maze, cell(3, 2)),
            zone: 0,
        };
        assert_eq!(maze.check_step(&long_enough), Ok(()));
    }
}
