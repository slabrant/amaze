use amaze::book::{STANDARD, zones_for};
use amaze::generate::{Choice, Event, Kind, generate_watched, total};
use amaze::goals::{Measures, measure};
use amaze::grid::{Cell, Grid};
use amaze::maze::{Maze, Step};
use std::fmt::Write;
use std::process::ExitCode;

const SEEDS: usize = 8;
const SHOWN: usize = 5;
const TRIES: u64 = 5;
const PAGE: &str = include_str!("steps.html");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first() else {
        eprintln!("usage: steps <report.html> [seeds] [first seed] [width height]");
        return ExitCode::FAILURE;
    };
    let number = |i: usize, default: u64| args.get(i).map_or(Ok(default), |a| a.parse::<u64>());
    let (Ok(seeds), Ok(first), Ok(width), Ok(height)) = (
        number(1, SEEDS as u64),
        number(2, rand::random::<u32>() as u64),
        number(3, STANDARD.width as u64),
        number(4, STANDARD.height as u64),
    ) else {
        eprintln!("seeds, first seed, width and height must be whole numbers");
        return ExitCode::FAILURE;
    };
    let grid = Grid {
        width: width as usize,
        height: height as usize,
    };
    let mut reports: Vec<String> = Vec::new();
    let mut failed: Vec<u64> = Vec::new();
    let mut next = first;
    while (reports.len() as u64) < seeds && next < first + TRIES * seeds {
        let batch = next..next + seeds - reports.len() as u64;
        next = batch.end;
        let made: Vec<(u64, Option<String>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = batch
                .map(|seed| scope.spawn(move || (seed, report(grid, seed))))
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().expect("report worker finishes"))
                .collect()
        });
        for (seed, made) in made {
            match made {
                Some(report) => reports.push(report),
                None => failed.push(seed),
            }
        }
    }
    let data = format!(
        r#"{{"width":{},"height":{},"zones":{},"stepTerms":{},"zoneStartTerms":{},"failed":{failed:?},"seeds":[{}]}}"#,
        grid.width,
        grid.height,
        zones_for(grid),
        names(&amaze::score::STEP_TERMS),
        names(&amaze::score::ZONE_START_TERMS),
        reports.join(",")
    );
    match std::fs::write(out, PAGE.replace("\"__DATA__\"", &data)) {
        Ok(()) => {
            println!(
                "{} mazes from seeds {first} to {}, {} failed",
                reports.len(),
                next - 1,
                failed.len()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not write {out}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn report(grid: Grid, seed: u64) -> Option<String> {
    let mut growth: Vec<String> = Vec::new();
    let mut steps: Vec<String> = Vec::new();
    let made = generate_watched(grid, seed, zones_for(grid), |event| match event {
        Event::Loop(loop_cells) => growth.push(cells(loop_cells.iter().copied())),
        Event::Chosen(maze, choice) => steps.push(step_report(maze, choice)),
    });
    let maze = made.ok()?;
    Some(format!(
        r#"{{"seed":{seed},"growth":[{}],"start":{},"finish":{},"steps":[{}]}}"#,
        growth.join(","),
        cell(maze.start),
        cell(maze.finish),
        steps.join(",")
    ))
}

fn step_report(maze: &Maze, choice: &Choice) -> String {
    let count = choice.candidates.len();
    let terms = choice.terms.len();
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&a, &b| {
        total(&choice.candidates[b].1).total_cmp(&total(&choice.candidates[a].1))
    });
    let mean: Vec<f64> = (0..terms)
        .map(|t| {
            choice
                .candidates
                .iter()
                .map(|(_, terms)| terms[t])
                .sum::<f64>()
                / count as f64
        })
        .collect();
    let flips: Vec<bool> = (0..terms)
        .map(|t| {
            let without = |terms: &[f64]| total(terms) - terms[t];
            let winner = (0..count)
                .max_by(|&a, &b| {
                    without(&choice.candidates[a].1).total_cmp(&without(&choice.candidates[b].1))
                })
                .unwrap_or(choice.chosen);
            winner != choice.chosen
        })
        .collect();
    let shown: Vec<String> = std::iter::once(choice.chosen)
        .chain(order.into_iter().filter(|&i| i != choice.chosen))
        .take(SHOWN)
        .map(|i| candidate(maze, &choice.candidates[i].0, &choice.candidates[i].1))
        .collect();
    format!(
        r#"{{"kind":"{}","zone":{},"count":{count},"mean":{},"flips":{flips:?},"shown":[{}],"measures":{}}}"#,
        match choice.kind {
            Kind::ZoneStart => "zone start",
            Kind::Step => "step",
        },
        choice.step().zone,
        numbers(&mean),
        shown.join(","),
        measures(&measure(maze))
    )
}

fn candidate(maze: &Maze, step: &Step, terms: &[f64]) -> String {
    let route = std::iter::once(maze.piece(step.from).cell)
        .chain(step.cells.iter().map(|c| c.cell))
        .chain(std::iter::once(maze.piece(step.to).cell));
    let on_top: Vec<u8> = step.cells.iter().map(|c| c.on_top as u8).collect();
    format!(
        r#"{{"cells":{},"onTop":{on_top:?},"terms":{}}}"#,
        cells(route),
        numbers(terms)
    )
}

fn measures(m: &Measures) -> String {
    format!(
        r#"{{"short routes":{},"near solution":{},"crossings":{},"crossings in a row":{},"zone joins apart":{},"empty cells":{},"start to finish":{},"solution length":{},"least mixed zone":{}}}"#,
        m.short_routes_between_neighbours,
        m.maze_cells_near_solution,
        m.crossings,
        m.crossings_in_a_row,
        m.nearest_zone_joins.unwrap_or(0.0),
        m.empty_cells,
        m.start_to_finish,
        m.solution_length,
        m.least_mixed_zone
    )
}

fn cell(cell: Cell) -> String {
    format!("[{},{}]", cell.x, cell.y)
}

fn cells(route: impl Iterator<Item = Cell>) -> String {
    let all: Vec<String> = route.map(cell).collect();
    format!("[{}]", all.join(","))
}

fn numbers(values: &[f64]) -> String {
    let mut text = String::from("[");
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            text.push(',');
        }
        let value = if value.is_finite() { *value } else { 0.0 };
        write!(text, "{:.4}", value).unwrap();
    }
    text.push(']');
    text
}

fn names(terms: &[&str]) -> String {
    let quoted: Vec<String> = terms.iter().map(|name| format!("\"{name}\"")).collect();
    format!("[{}]", quoted.join(","))
}
