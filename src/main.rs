use amaze::book::{Contents, book, shaped_book};
use amaze::pdf::pdf;
use amaze::shapes::{SHAPES, Shape, shape};
use std::process::ExitCode;

const PER_LEVEL: usize = 2;
const MINIS_PER_LEVEL: usize = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: amaze <book.pdf> [seed] [shape,shape,...|all]");
        return ExitCode::FAILURE;
    };
    let seed = match args.get(1).map(|seed| seed.parse::<u64>()) {
        None => 0,
        Some(Ok(seed)) => seed,
        Some(Err(_)) => {
            eprintln!("seed must be a whole number");
            return ExitCode::FAILURE;
        }
    };
    let pages = match args.get(2) {
        None => book(
            seed,
            &Contents {
                per_level: PER_LEVEL,
                minis_per_level: MINIS_PER_LEVEL,
            },
        ),
        Some(names) if names == "all" => match shaped_book(seed, &SHAPES) {
            Ok(pages) => pages,
            Err(failed) => return no_maze(failed, seed),
        },
        Some(names) => {
            let Some(shapes) = names.split(',').map(shape).collect::<Option<Vec<Shape>>>() else {
                let names: Vec<&str> = SHAPES.iter().map(|shape| shape.name).collect();
                eprintln!("shapes must be all, or from: {}", names.join(" "));
                return ExitCode::FAILURE;
            };
            match shaped_book(seed, &shapes) {
                Ok(pages) => pages,
                Err(failed) => return no_maze(failed, seed),
            }
        }
    };
    match std::fs::write(out, pdf(&pages)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("could not write {out}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn no_maze(shape: Shape, seed: u64) -> ExitCode {
    eprintln!("no {} maze from seed {seed}; try another seed", shape.name);
    ExitCode::FAILURE
}
