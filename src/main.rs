//! amaze - printable mazes with bridges that weave over and under.
//!
//! Rules of the printed maze:
//!   * Paths are thin corridors through solid ground. Start at the arrow and
//!     finish at the bullseye. Those two are the only dead ends.
//!   * A bridge is a straight run of path between two landings that passes
//!     over or under what it crosses. Over solid ground or a plain path it is
//!     on top; where bridges cross each other, each alternates over and under
//!     along its length like a basket weave. A path that goes under stops at
//!     one side and comes out the other, its hidden edges shown as faint
//!     dashes.

mod check;
mod code;
mod maze;
mod pdf;
mod render;
mod rng;

use std::fs::File;
use std::io::BufWriter;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use maze::{default_zones, walk_length, Maze};
use rng::Rng;

const USAGE: &str = "\
Generate printable mazes with bridges that weave over and under.

usage: amaze [options]
       amaze book [-o PATH] [--seed CODE] [--sign NAME]
                                              a book of mazes, Trivial to Hard,
                                              then minis, solutions on the backs
       amaze check [FIRST_SEED] [LAST_SEED]   build mazes and check every rule

options:
  -o, --out PATH      output PDF path (default maze.pdf)
  --cols N            maze width in cells (default 36, or 18 at 4 per page)
  --rows N            maze height in cells (default 45, or 22 at 4 per page)
  --zones N           zones the maze is split into, each with exactly one way
                      across to the next (default: picked at random, more on
                      bigger mazes)
  --count N           number of mazes (default 1)
  --per-page 1|4      mazes per page: 1, or 4 small ones in a 2x2 grid
  --level NAME        size by level: trivial, easy, medium, hard,
                      mini-trivial, mini-easy, mini-medium or mini-hard
                      (as in the book)
  --seed CODE         recreate a maze from the code printed beside it
  --solution          add solutions, all at the back
  --back-solution     put each page's solutions on the back of its sheet:
                      only the solution line, mirrored, to see through the
                      paper held up to the light (print double sided,
                      flip on long edge)
  --show-zones        tint each zone a pale colour on the solution pages
  --graph-paper       draw a faint blue graph-paper grid behind the maze
  --sign NAME         print NAME once on each page, under the (bottom-left) maze
  -h, --help          show this help";

struct Args {
    out: String,
    cols: Option<usize>,
    rows: Option<usize>,
    zones: Option<usize>,
    count: usize,
    per_page: usize,
    seed: Option<u64>,
    level: Option<usize>,
    solution: bool,
    back_solution: bool,
    show_zones: bool,
    graph_paper: bool,
    sign: Option<String>,
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        out: "maze.pdf".into(),
        cols: None,
        rows: None,
        zones: None,
        count: 1,
        per_page: 1,
        seed: None,
        level: None,
        solution: false,
        back_solution: false,
        show_zones: false,
        graph_paper: false,
        sign: None,
    };
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| -> Result<String, String> {
            it.next().cloned().ok_or_else(|| format!("{name} needs a value"))
        };
        let number = |v: String, name: &str| -> Result<usize, String> {
            v.parse().map_err(|_| format!("{name}: not a number: {v}"))
        };
        match flag.as_str() {
            "-o" | "--out" => a.out = value(flag)?,
            "--cols" => a.cols = Some(number(value(flag)?, flag)?),
            "--rows" => a.rows = Some(number(value(flag)?, flag)?),
            "--zones" => a.zones = Some(number(value(flag)?, flag)?),
            "--count" => a.count = number(value(flag)?, flag)?,
            "--per-page" => {
                a.per_page = number(value(flag)?, flag)?;
                if a.per_page != 1 && a.per_page != 4 {
                    return Err("--per-page must be 1 or 4".into());
                }
            }
            "--seed" => {
                let v = value(flag)?;
                a.seed = Some(code::decode(&v).ok_or_else(|| format!("--seed: not a maze code: {v}"))?);
            }
            "--level" => {
                let v = value(flag)?;
                let i = LEVELS.iter().position(|l| l.name == v.to_lowercase());
                a.level = Some(i.ok_or_else(|| format!("--level: no level called {v}"))?);
            }
            "--solution" => a.solution = true,
            "--back-solution" => a.back_solution = true,
            "--show-zones" => a.show_zones = true,
            "--graph-paper" => a.graph_paper = true,
            "--sign" => a.sign = Some(value(flag)?),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(a)
}

fn run(a: Args) -> Result<(), String> {
    let small = a.per_page == 4;
    let level = a.level.map(|i| &LEVELS[i]);
    let cols = a.cols.or(level.map(|l| l.cols)).unwrap_or(if small { 18 } else { 36 });
    let rows = a.rows.or(level.map(|l| l.rows)).unwrap_or(if small { 22 } else { 45 });
    // A level's own zone counts, unless its size was changed.
    let level_zones = level.filter(|_| a.cols.is_none() && a.rows.is_none()).map(|l| l.zones);
    let mut seed = a.seed.unwrap_or_else(random_seed);
    let mut mazes = Vec::new();
    for i in 0..a.count {
        if i > 0 {
            seed = code::next(seed);
        }
        let mut rng = Rng::new(seed);
        let zones = a.zones.unwrap_or_else(|| match level_zones {
            Some((lo, hi)) => rng.range(lo, hi),
            None => default_zones(rows, cols, &mut rng),
        });
        let t = Instant::now();
        let m = Maze::generate(rows, cols, zones, &mut rng).map_err(|e| format!("seed {seed}: {e}"))?;
        let sol = m.solve().ok_or_else(|| format!("seed {seed}: goal unreachable (bug)"))?;
        eprintln!(
            "maze {}: code {}, {zones} zones, {} bridges, {} junctions, solution walks {} cells ({:.0} ms)",
            i + 1,
            code::encode(seed),
            m.bridges.len(),
            m.junctions(),
            walk_length(&sol, cols),
            t.elapsed().as_secs_f64() * 1000.0
        );
        mazes.push((m, seed, sol));
    }
    let numbered = a.count > 1;
    let name = level.map_or("Maze", |l| l.label);
    let label = |i: usize, sol: bool| match (numbered, sol) {
        (true, false) => format!("{name} {}", i + 1),
        (true, true) => format!("{name} {} solution", i + 1),
        (false, false) => level.map_or(String::new(), |l| l.label.to_string()),
        (false, true) => format!("{} solution", level.map_or("Maze", |l| l.label)),
    };
    let opts = render::Options {
        per_page: a.per_page,
        graph_paper: a.graph_paper,
        show_zones: a.show_zones,
        sign: a.sign.clone(),
    };
    let puzzles: Vec<render::Item> = mazes
        .iter()
        .enumerate()
        .map(|(i, (m, seed, sol))| render::Item {
            maze: m,
            seed: Some(*seed),
            // Only drawn on the back of the sheet, if asked for.
            solution: Some(sol),
            answer: false,
            label: label(i, false),
        })
        .collect();
    let mut doc = pdf::Pdf::new();
    let fronts = render::render_pages(&puzzles, &opts, false);
    let backs = if a.back_solution { render::render_pages(&puzzles, &opts, true) } else { Vec::new() };
    let mut backs = backs.into_iter();
    for page in fronts {
        doc.add_page(page);
        if let Some(back) = backs.next() {
            doc.add_page(back);
        }
    }
    // All the solutions go at the back, in the same order as the mazes,
    // starting on a fresh page.
    if a.solution {
        let answers: Vec<render::Item> = mazes
            .iter()
            .enumerate()
            .map(|(i, (m, seed, sol))| render::Item {
                maze: m,
                seed: Some(*seed),
                solution: Some(sol),
                answer: true,
                label: label(i, true),
            })
            .collect();
        for page in render::render_pages(&answers, &opts, false) {
            doc.add_page(page);
        }
    }
    let file = File::create(&a.out).map_err(|e| format!("{}: {e}", a.out))?;
    doc.write(&mut BufWriter::new(file)).map_err(|e| format!("{}: {e}", a.out))?;
    eprintln!("wrote {}", a.out);
    Ok(())
}

/// The book's sections: (name, mazes per page, its mazes' levels), in
/// page order.
type Section = (&'static str, usize, Vec<&'static Level>);

/// A size of maze, as in the book.
struct Level {
    /// Name for --level.
    name: &'static str,
    label: &'static str,
    cols: usize,
    rows: usize,
    /// Fewest and most zones, picked at random between.
    zones: (usize, usize),
}

const fn level(name: &'static str, label: &'static str, cols: usize, rows: usize, zones: (usize, usize)) -> Level {
    Level { name, label, cols, rows, zones }
}

/// Each full-size level is eight cells wider than the last, rows filling
/// the page, and each mini is sized so its cells print the same size as its
/// full-size twin's. A mini has as many zones as its twin.
const LEVELS: [Level; 8] = [
    level("trivial", "Trivial", 16, 20, (2, 2)),
    level("easy", "Easy", 24, 31, (3, 4)),
    level("medium", "Medium", 32, 41, (3, 5)),
    level("hard", "Hard", 40, 52, (4, 7)),
    level("mini-trivial", "Mini Trivial", 8, 10, (2, 2)),
    level("mini-easy", "Mini Easy", 12, 15, (3, 4)),
    level("mini-medium", "Mini Medium", 15, 19, (3, 5)),
    level("mini-hard", "Mini Hard", 19, 24, (4, 7)),
];
/// Where the minis start in LEVELS.
const MINIS: usize = 4;

fn book_sections() -> Vec<Section> {
    let full = |l: &'static Level| -> Section { (l.label, 1, vec![l; 3]) };
    // Minis, three of each level in turn, four to a page.
    let minis = LEVELS[MINIS..].iter().flat_map(|l| vec![l; 3]).collect();
    let mut out: Vec<Section> = LEVELS[..MINIS].iter().map(full).collect();
    out.push(("Minis", 4, minis));
    out
}

/// A fresh seed when none is given.
fn random_seed() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    code::fresh(&mut Rng::new(now))
}

/// A book of mazes for double-sided printing: each section in turn, every
/// page with its solutions mirrored on the back.
fn run_book(argv: &[String]) -> Result<(), String> {
    let mut out = "book.pdf".to_string();
    let mut base = None;
    let mut sign = None;
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "-o" | "--out" => out = value.clone(),
            "--seed" => base = Some(code::decode(value).ok_or_else(|| format!("--seed: not a maze code: {value}"))?),
            "--sign" => sign = Some(value.clone()),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    let base = base.unwrap_or_else(random_seed);
    let sections = book_sections();
    // Every maze gets its own code, drawn from the book's.
    let mut draw = Rng::new(base);
    let jobs: Vec<(&Level, u64)> =
        sections.iter().flat_map(|(_, _, mazes)| mazes.iter().copied()).map(|l| (l, code::fresh(&mut draw))).collect();

    // Each maze is independent: build them all at once.
    let t = Instant::now();
    let built: Vec<Result<(Maze, Vec<usize>), String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(l, seed)| {
                scope.spawn(move || {
                    let mut rng = Rng::new(seed);
                    let zones = rng.range(l.zones.0, l.zones.1);
                    let m = Maze::generate(l.rows, l.cols, zones, &mut rng).map_err(|e| format!("seed {seed}: {e}"))?;
                    let sol = m.solve().ok_or_else(|| format!("seed {seed}: goal unreachable (bug)"))?;
                    Ok((m, sol))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("maze thread panicked")).collect()
    });
    let built: Vec<(Maze, Vec<usize>)> = built.into_iter().collect::<Result<_, _>>()?;
    eprintln!("built {} mazes in {:.1} s (book code {})", built.len(), t.elapsed().as_secs_f64(), code::encode(base));

    let mut doc = pdf::Pdf::new();
    let mut k = 0;
    for (_, per, mazes) in &sections {
        let items: Vec<render::Item> = mazes
            .iter()
            .map(|l| {
                let (m, sol) = &built[k];
                let it = render::Item {
                    maze: m,
                    seed: Some(jobs[k].1),
                    solution: Some(sol),
                    answer: false,
                    label: l.label.to_string(),
                };
                k += 1;
                it
            })
            .collect();
        let opts = render::Options { per_page: *per, graph_paper: false, show_zones: false, sign: sign.clone() };
        let fronts = render::render_pages(&items, &opts, false);
        let backs = render::render_pages(&items, &opts, true);
        for (front, back) in fronts.into_iter().zip(backs) {
            doc.add_page(front);
            doc.add_page(back);
        }
    }
    let file = File::create(&out).map_err(|e| format!("{out}: {e}"))?;
    doc.write(&mut BufWriter::new(file)).map_err(|e| format!("{out}: {e}"))?;
    eprintln!("wrote {out}: print double sided, flip on long edge");
    Ok(())
}

/// Build mazes for a range of seeds in the book's sizes, check every rule,
/// and measure how many loops wind around or across another zone.
fn run_checks(first: u64, last: u64) -> bool {
    let mut bad = 0;
    let mut all = check::LoopStats::default();
    for seed in first..last {
        let l = &LEVELS[seed as usize % LEVELS.len()];
        let (cols, rows) = (l.cols, l.rows);
        let mut rng = Rng::new(seed);
        let zones = rng.range(l.zones.0, l.zones.1);
        let what = format!("seed {seed} ({cols}x{rows}, {zones} zones)");
        let t = Instant::now();
        let m = match Maze::generate(rows, cols, zones, &mut rng) {
            Ok(m) => m,
            Err(e) => {
                bad += 1;
                println!("{what}: could not generate: {e}");
                continue;
            }
        };
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let errs = check::check(&m);
        let s = &m.stats;
        let l = check::loop_stats(&m);
        all.loops += l.loops;
        all.wrapping += l.wrapping;
        all.around += l.around;
        all.links += l.links;
        all.wrapping_links += l.wrapping_links;
        if errs.is_empty() {
            println!(
                "{what}: ok  {:>3.0} ms  {} strand crossings, {} hops, {} rings, {} junctions, loops {} ({} around, {} across), layouts tried {}",
                ms,
                s.crossings,
                s.hops,
                s.rings,
                m.junctions(),
                l.loops,
                l.around,
                l.wrapping - l.around,
                s.layouts
            );
        } else {
            bad += 1;
            println!("{what}: {} problems", errs.len());
            for e in errs.iter().take(5) {
                println!("    {e}");
            }
        }
    }
    println!("{} of {} mazes passed", (last - first) as usize - bad, last - first);
    let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
    println!(
        "loops: {}; around another zone {:.0}%, only across one {:.0}%; links whose shortest loop wraps {:.0}%",
        all.loops,
        pct(all.around, all.loops),
        pct(all.wrapping - all.around, all.loops),
        pct(all.wrapping_links, all.links)
    );
    bad == 0
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if argv.first().map(String::as_str) == Some("book") {
        return match run_book(&argv[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("amaze: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if argv.first().map(String::as_str) == Some("check") {
        let n = |i: usize, d: u64| argv.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
        return if run_checks(n(1, 0), n(2, 12)) { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    match parse(&argv).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("amaze: {e}");
            ExitCode::FAILURE
        }
    }
}
