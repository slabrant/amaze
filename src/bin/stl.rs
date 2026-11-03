use amaze::book::{levels, maze, single};
use amaze::drawing::Ink;
use amaze::grid::Grid;
use amaze::pdf::pdf;
use amaze::stl::stl;
use std::process::ExitCode;

const EASY: usize = 1;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: stl <maze.stl> [seed] [width height]");
        return ExitCode::FAILURE;
    };
    let number = |i: usize, default: u64| args.get(i).map_or(Ok(default), |a| a.parse::<u64>());
    let easy = levels(1)[EASY];
    let (Ok(seed), Ok(width), Ok(height)) = (
        number(1, rand::random::<u32>() as u64),
        number(2, easy.width as u64),
        number(3, easy.height as u64),
    ) else {
        eprintln!("seed, width and height must be whole numbers");
        return ExitCode::FAILURE;
    };
    if width < 4 || height < 4 {
        eprintln!("grids smaller than 4 by 4 are refused");
        return ExitCode::FAILURE;
    }
    let grid = Grid {
        width: width as usize,
        height: height as usize,
    };
    let maze = maze(grid, seed);
    let page = std::path::Path::new(out).with_extension("pdf");
    let written = std::fs::write(out, stl(&maze))
        .and_then(|()| std::fs::write(&page, pdf(&single(&maze, EASY, Ink::Colour))));
    match written {
        Ok(()) => {
            println!("seed {seed}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not write {out} or {}: {error}", page.display());
            ExitCode::FAILURE
        }
    }
}
