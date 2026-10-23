use amaze::book::{Contents, book};
use amaze::pdf::pdf;
use std::process::ExitCode;

const PER_LEVEL: usize = 2;
const MINIS_PER_LEVEL: usize = 3;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: amaze <book.pdf> [seed]");
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
    let contents = Contents {
        per_level: PER_LEVEL,
        minis_per_level: MINIS_PER_LEVEL,
    };
    match std::fs::write(out, pdf(&book(seed, &contents))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("could not write {out}: {error}");
            ExitCode::FAILURE
        }
    }
}
