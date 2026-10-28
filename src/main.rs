use amaze::book::{Contents, book, two_up_book};
use amaze::pdf::pdf;
use std::process::ExitCode;

const PER_LEVEL: usize = 2;
const MINIS_PER_LEVEL: usize = 3;
const TWO_UP_PER_LEVEL: usize = 4;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let two_up = args.iter().any(|arg| arg == "--two-up");
    args.retain(|arg| arg != "--two-up");
    let Some(out) = args.first() else {
        eprintln!("usage: amaze <book.pdf> [seed] [--two-up]");
        return ExitCode::FAILURE;
    };
    let seed = match args.get(1).map(|seed| seed.parse::<u64>()) {
        None => rand::random::<u32>() as u64,
        Some(Ok(seed)) => seed,
        Some(Err(_)) => {
            eprintln!("seed must be a whole number");
            return ExitCode::FAILURE;
        }
    };
    let contents = Contents {
        per_level: PER_LEVEL,
        minis_per_level: MINIS_PER_LEVEL,
    };
    let pages = if two_up {
        two_up_book(seed, TWO_UP_PER_LEVEL)
    } else {
        book(seed, &contents)
    };
    match std::fs::write(out, pdf(&pages)) {
        Ok(()) => {
            println!("seed {seed}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not write {out}: {error}");
            ExitCode::FAILURE
        }
    }
}
