use amaze::book::{Contents, book, shaped_book};
use amaze::pdf::pdf;
use amaze::shapes::{SHAPES, shape};
use std::process::ExitCode;

const PER_LEVEL: usize = 2;
const MINIS_PER_LEVEL: usize = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: amaze <book.pdf> [seed] [shape]");
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
        Some(name) => {
            let Some(shape) = shape(name) else {
                let names: Vec<&str> = SHAPES.iter().map(|shape| shape.name).collect();
                eprintln!("shape must be one of: {}", names.join(" "));
                return ExitCode::FAILURE;
            };
            let Some(pages) = shaped_book(seed, shape) else {
                eprintln!("no {name} maze from seed {seed}; try another seed");
                return ExitCode::FAILURE;
            };
            pages
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
