use amaze::book::{Layout, book};
use amaze::drawing::Ink;
use amaze::pdf::pdf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let layout = if args.iter().any(|arg| arg == "--two-up") {
        Layout::TwoUp
    } else {
        Layout::OneUp
    };
    let ink = if args.iter().any(|arg| arg == "--black") {
        Ink::Black
    } else {
        Ink::Colour
    };
    args.retain(|arg| arg != "--two-up" && arg != "--black");
    let Some(out) = args.first() else {
        eprintln!("usage: amaze <book.pdf> [seed] [--two-up] [--black]");
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
    let pages = book(seed, layout, ink);
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
