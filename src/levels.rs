pub const FULL_SIZE_MAZES_PER_PAGE: usize = 1;
pub const MINI_MAZES_PER_PAGE: usize = 4;

#[derive(Debug)]
pub struct Level {
    pub option_name: &'static str,
    pub label: &'static str,
    pub cols: usize,
    pub rows: usize,
    pub mazes_per_page: usize,
}

const fn level(option_name: &'static str, label: &'static str, cols: usize, rows: usize, mazes_per_page: usize) -> Level {
    Level { option_name, label, cols, rows, mazes_per_page }
}

const FULL: usize = FULL_SIZE_MAZES_PER_PAGE;
const MINI: usize = MINI_MAZES_PER_PAGE;

pub static LEVELS: [Level; 8] = [
    level("trivial", "Trivial", 12, 15, FULL),
    level("easy", "Easy", 16, 20, FULL),
    level("standard", "Standard", 20, 26, FULL),
    level("hard", "Hard", 24, 31, FULL),
    level("mini-trivial", "Mini Trivial", 8, 10, MINI),
    level("mini-easy", "Mini Easy", 10, 13, MINI),
    level("mini-standard", "Mini Standard", 12, 15, MINI),
    level("mini-hard", "Mini Hard", 13, 16, MINI),
];

pub fn full_size() -> impl Iterator<Item = &'static Level> {
    LEVELS.iter().filter(|l| l.mazes_per_page == FULL)
}

pub fn minis() -> impl Iterator<Item = &'static Level> {
    LEVELS.iter().filter(|l| l.mazes_per_page == MINI)
}

pub fn named(option_name: &str) -> Option<&'static Level> {
    LEVELS.iter().find(|l| l.option_name == option_name.to_lowercase())
}

pub fn standard(mazes_per_page: usize) -> &'static Level {
    named(if mazes_per_page == MINI { "mini-standard" } else { "standard" }).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::space_for_one_maze;

    #[test]
    fn every_level_fills_its_space_on_the_page_with_square_cells() {
        for l in &LEVELS {
            let space = space_for_one_maze(l.mazes_per_page);
            let cell = space.width / l.cols as f64;
            assert_eq!(l.rows, (space.height / cell) as usize, "{}", l.label);
        }
    }
}
