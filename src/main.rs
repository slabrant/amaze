use amaze::book::book;
use amaze::drawing::Ink;
use amaze::pdf::pdf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let ink = if args.iter().any(|arg| arg == "--black") {
        Ink::Black
    } else {
        Ink::Colour
    };
    args.retain(|arg| arg != "--black");
    let per_page = match args.iter().position(|arg| arg == "--per-page") {
        None => 1,
        Some(at) => match args.get(at + 1).map(|count| count.parse::<usize>()) {
            Some(Ok(count)) if count > 0 => {
                args.drain(at..=at + 1);
                count
            }
            _ => {
                eprintln!("--per-page needs a whole number above zero");
                return ExitCode::FAILURE;
            }
        },
    };
    let Some(out) = args.first() else {
        eprintln!("usage: amaze <book.pdf> [seed] [--per-page N] [--black]");
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
    let pages = book(seed, per_page, ink);
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
