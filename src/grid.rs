#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cell {
    pub x: usize,
    pub y: usize,
}

impl Cell {
    pub fn squared_distance(self, other: Cell) -> usize {
        self.x.abs_diff(other.x).pow(2) + self.y.abs_diff(other.y).pow(2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

    pub fn opposite(self) -> Dir {
        match self {
            Dir::North => Dir::South,
            Dir::East => Dir::West,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
        }
    }

    pub fn between(from: Cell, to: Cell) -> Option<Dir> {
        Dir::ALL
            .into_iter()
            .find(|&dir| step(from, dir) == Some(to))
    }
}

fn step(cell: Cell, dir: Dir) -> Option<Cell> {
    let Cell { x, y } = cell;
    match dir {
        Dir::North => y.checked_sub(1).map(|y| Cell { x, y }),
        Dir::East => Some(Cell { x: x + 1, y }),
        Dir::South => Some(Cell { x, y: y + 1 }),
        Dir::West => x.checked_sub(1).map(|x| Cell { x, y }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
}

impl Grid {
    pub fn contains(&self, cell: Cell) -> bool {
        cell.x < self.width && cell.y < self.height
    }

    pub fn neighbour(&self, cell: Cell, dir: Dir) -> Option<Cell> {
        step(cell, dir).filter(|&next| self.contains(next))
    }

    pub fn neighbours(&self, cell: Cell) -> impl Iterator<Item = (Dir, Cell)> + '_ {
        Dir::ALL
            .into_iter()
            .filter_map(move |dir| self.neighbour(cell, dir).map(|next| (dir, next)))
    }

    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.height).flat_map(move |y| (0..self.width).map(move |x| Cell { x, y }))
    }

    pub fn index(&self, cell: Cell) -> usize {
        cell.y * self.width + cell.x
    }
}
