use std::sync::LazyLock;

use crate::render::space_for_one_maze;
use crate::seeded_rng::Rng;

const NARROWEST_FULL_SIZE_WIDTH: usize = 12;
const WIDTH_STEP_BETWEEN_LEVELS: usize = 4;
pub const FULL_SIZE_MAZES_PER_PAGE: usize = 1;
pub const MINI_MAZES_PER_PAGE: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneCountRange {
    pub fewest: usize,
    pub most: usize,
}

impl ZoneCountRange {
    pub fn pick(self, rng: &mut Rng) -> usize {
        rng.from_to_inclusive(self.fewest, self.most)
    }
}

#[derive(Debug)]
pub struct Level {
    pub option_name: String,
    pub label: String,
    pub cols: usize,
    pub rows: usize,
    pub zones: ZoneCountRange,
    pub mazes_per_page: usize,
}

struct Difficulty {
    name: &'static str,
    label: &'static str,
    zones: ZoneCountRange,
    mini_zones: ZoneCountRange,
}

const fn zones(fewest: usize, most: usize) -> ZoneCountRange {
    ZoneCountRange { fewest, most }
}

const EASIEST_FIRST: [Difficulty; 4] = [
    Difficulty { name: "trivial", label: "Trivial", zones: zones(2, 2), mini_zones: zones(1, 2) },
    Difficulty { name: "easy", label: "Easy", zones: zones(3, 4), mini_zones: zones(3, 4) },
    Difficulty { name: "medium", label: "Medium", zones: zones(3, 5), mini_zones: zones(3, 5) },
    Difficulty { name: "hard", label: "Hard", zones: zones(4, 7), mini_zones: zones(4, 7) },
];

fn cell_size(cols: usize, per_page: usize) -> f64 {
    space_for_one_maze(per_page).width / cols as f64
}

fn rows_filling_the_space(cols: usize, per_page: usize) -> usize {
    (space_for_one_maze(per_page).height / cell_size(cols, per_page)) as usize
}

fn cols_giving_nearest_cell_size(wanted: f64, per_page: usize) -> usize {
    let off_by = |cols: usize| (cell_size(cols, per_page) - wanted).abs();
    (1..=space_for_one_maze(per_page).width as usize).min_by(|&a, &b| off_by(a).total_cmp(&off_by(b))).unwrap()
}

fn full_size_width(step: usize) -> usize {
    NARROWEST_FULL_SIZE_WIDTH + step * WIDTH_STEP_BETWEEN_LEVELS
}

fn ladder() -> Vec<Level> {
    let full_size = EASIEST_FIRST.iter().enumerate().map(|(step, d)| {
        let cols = full_size_width(step);
        Level {
            option_name: d.name.to_string(),
            label: d.label.to_string(),
            cols,
            rows: rows_filling_the_space(cols, FULL_SIZE_MAZES_PER_PAGE),
            zones: d.zones,
            mazes_per_page: FULL_SIZE_MAZES_PER_PAGE,
        }
    });
    let minis_sized_like_the_full_size_level_one_step_up = EASIEST_FIRST.iter().enumerate().map(|(step, d)| {
        let cells_of_next_level_up = cell_size(full_size_width(step + 1), FULL_SIZE_MAZES_PER_PAGE);
        let cols = cols_giving_nearest_cell_size(cells_of_next_level_up, MINI_MAZES_PER_PAGE);
        Level {
            option_name: format!("mini-{}", d.name),
            label: format!("Mini {}", d.label),
            cols,
            rows: rows_filling_the_space(cols, MINI_MAZES_PER_PAGE),
            zones: d.mini_zones,
            mazes_per_page: MINI_MAZES_PER_PAGE,
        }
    });
    full_size.chain(minis_sized_like_the_full_size_level_one_step_up).collect()
}

pub static LEVELS: LazyLock<Vec<Level>> = LazyLock::new(ladder);

pub fn full_size() -> &'static [Level] {
    &LEVELS[..EASIEST_FIRST.len()]
}

pub fn minis() -> &'static [Level] {
    &LEVELS[EASIEST_FIRST.len()..]
}

pub fn named(option_name: &str) -> Option<&'static Level> {
    LEVELS.iter().find(|l| l.option_name == option_name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_sizes() {
        let sizes: Vec<(&str, usize, usize)> = LEVELS.iter().map(|l| (l.label.as_str(), l.cols, l.rows)).collect();
        assert_eq!(
            sizes,
            [
                ("Trivial", 12, 15),
                ("Easy", 16, 20),
                ("Medium", 20, 26),
                ("Hard", 24, 31),
                ("Mini Trivial", 8, 10),
                ("Mini Easy", 10, 13),
                ("Mini Medium", 12, 15),
                ("Mini Hard", 13, 16),
            ]
        );
    }

    #[test]
    fn mini_cells_print_within_five_percent_of_the_next_level_up() {
        for (step, mini) in minis().iter().enumerate() {
            let wanted = cell_size(full_size_width(step + 1), FULL_SIZE_MAZES_PER_PAGE);
            let got = cell_size(mini.cols, MINI_MAZES_PER_PAGE);
            assert!((got - wanted).abs() / wanted < 0.05, "{}: {got} vs {wanted}", mini.label);
        }
    }
}
