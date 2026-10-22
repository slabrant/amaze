mod generate;
mod goals;
mod levels;
mod maze;
mod pdf;
mod render;
mod seed_code;
mod seeded_rng;

use std::fs::File;
use std::io::BufWriter;
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use levels::Level;
use maze::{walk_length, Maze};
use seeded_rng::Rng;

const USAGE: &str = "\
Generate printable mazes with bridges that weave over and under.

usage: amaze [options]
       amaze book [-o PATH] [--seed CODE] [--sign NAME]
                                              a book of mazes, Trivial to Hard,
                                              then minis, solutions on the backs
       amaze check [FIRST_SEED] [LAST_SEED]   build mazes and print their stats

options:
  -o, --out PATH      output PDF path (default maze.pdf)
  --cols N            maze width in cells (default: Standard's)
  --rows N            maze height in cells (default: Standard's)
  --count N           number of mazes (default 1)
  --per-page 1|4      mazes per page: 1, or 4 small ones in a 2x2 grid
  --level NAME        size by level: trivial, easy, standard, hard,
                      mini-trivial, mini-easy, mini-standard or mini-hard
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
    count: usize,
    per_page: usize,
    seed: Option<u64>,
    level: Option<&'static Level>,
    solutions_at_the_back: bool,
    solutions_on_backs_of_sheets: bool,
    zone_tints_on_answers: bool,
    graph_paper: bool,
    signature: Option<String>,
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        out: "maze.pdf".into(),
        cols: None,
        rows: None,
        count: 1,
        per_page: 1,
        seed: None,
        level: None,
        solutions_at_the_back: false,
        solutions_on_backs_of_sheets: false,
        zone_tints_on_answers: false,
        graph_paper: false,
        signature: None,
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
            "--count" => a.count = number(value(flag)?, flag)?,
            "--per-page" => {
                a.per_page = number(value(flag)?, flag)?;
                if a.per_page != 1 && a.per_page != 4 {
                    return Err("--per-page must be 1 or 4".into());
                }
            }
            "--seed" => {
                let v = value(flag)?;
                a.seed = Some(seed_code::decode(&v).ok_or_else(|| format!("--seed: not a maze code: {v}"))?);
            }
            "--level" => {
                let v = value(flag)?;
                a.level = Some(levels::named(&v).ok_or_else(|| format!("--level: no level called {v}"))?);
            }
            "--solution" => a.solutions_at_the_back = true,
            "--back-solution" => a.solutions_on_backs_of_sheets = true,
            "--show-zones" => a.zone_tints_on_answers = true,
            "--graph-paper" => a.graph_paper = true,
            "--sign" => a.signature = Some(value(flag)?),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(a)
}

fn run(a: Args) -> Result<(), String> {
    let sized_by = a.level.unwrap_or_else(|| levels::standard(a.per_page));
    let cols = a.cols.unwrap_or(sized_by.cols);
    let rows = a.rows.unwrap_or(sized_by.rows);
    let level = Some(sized_by).filter(|_| a.cols.is_none() && a.rows.is_none());
    let mut seed = a.seed.unwrap_or_else(seed_from_clock);
    let mut mazes = Vec::new();
    for i in 0..a.count {
        if i > 0 {
            seed = seed_code::next_seed_with_a_letter(seed);
        }
        let mut rng = Rng::new(seed);
        let t = Instant::now();
        let m = Maze::generate(rows, cols, &mut rng);
        let route = m.solve().ok_or_else(|| format!("seed {seed}: goal unreachable (bug)"))?;
        eprintln!(
            "maze {}: code {}, {} zones, {} bridges, {} junctions, solution walks {} cells ({:.0} ms)",
            i + 1,
            seed_code::encode(seed),
            m.zone_count,
            m.bridges.len(),
            m.junctions(),
            walk_length(&route, cols),
            t.elapsed().as_secs_f64() * 1000.0
        );
        mazes.push((m, seed, route));
    }
    let numbered = a.count > 1;
    let name = level.map_or("Maze", |l| l.label);
    let label = |i: usize, answer: bool| match (numbered, answer) {
        (true, false) => format!("{name} {}", i + 1),
        (true, true) => format!("{name} {} solution", i + 1),
        (false, false) => level.map_or(String::new(), |l| l.label.to_string()),
        (false, true) => format!("{} solution", level.map_or("Maze", |l| l.label)),
    };
    let opts = render::Options {
        per_page: a.per_page,
        graph_paper: a.graph_paper,
        zone_tints_on_answers: a.zone_tints_on_answers,
        signature_once_a_page: a.signature.clone(),
    };
    let items = |answers: bool| -> Vec<render::Item> {
        mazes
            .iter()
            .enumerate()
            .map(|(i, (m, seed, route))| render::Item {
                maze: m,
                seed_printed_as_code: Some(*seed),
                solution: Some(route),
                solution_drawn_on_maze: answers,
                label: label(i, answers),
            })
            .collect()
    };
    let puzzles = items(false);
    let mut doc = pdf::Pdf::new();
    let fronts = render::render_pages(&puzzles, &opts, false);
    let backs =
        if a.solutions_on_backs_of_sheets { render::render_pages(&puzzles, &opts, true) } else { Vec::new() };
    let mut backs = backs.into_iter();
    for page in fronts {
        doc.add_page(page);
        if let Some(back) = backs.next() {
            doc.add_page(back);
        }
    }
    if a.solutions_at_the_back {
        for page in render::render_pages(&items(true), &opts, false) {
            doc.add_page(page);
        }
    }
    let file = File::create(&a.out).map_err(|e| format!("{}: {e}", a.out))?;
    doc.write(&mut BufWriter::new(file)).map_err(|e| format!("{}: {e}", a.out))?;
    eprintln!("wrote {}", a.out);
    Ok(())
}

const BOOK_MAZES_PER_FULL_SIZE_LEVEL: usize = 2;
const BOOK_MAZES_PER_MINI_LEVEL: usize = 3;

struct BookSection {
    mazes_per_page: usize,
    levels: Vec<&'static Level>,
}

fn book_sections_in_page_order() -> Vec<BookSection> {
    let mut sections: Vec<BookSection> = levels::full_size()
        .map(|l| BookSection { mazes_per_page: l.mazes_per_page, levels: vec![l; BOOK_MAZES_PER_FULL_SIZE_LEVEL] })
        .collect();
    let minis_level_by_level = levels::minis().flat_map(|l| vec![l; BOOK_MAZES_PER_MINI_LEVEL]).collect();
    sections.push(BookSection { mazes_per_page: levels::MINI_MAZES_PER_PAGE, levels: minis_level_by_level });
    sections
}

fn seed_from_clock() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    seed_code::random_seed_with_a_letter(&mut Rng::new(now))
}

fn build_all_at_once(jobs: &[(&Level, u64)]) -> Result<Vec<(Maze, Vec<usize>)>, String> {
    let built: Vec<Result<(Maze, Vec<usize>), String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(l, seed)| {
                scope.spawn(move || {
                    let mut rng = Rng::new(seed);
                    let m = Maze::generate(l.rows, l.cols, &mut rng);
                    let route = m.solve().ok_or_else(|| format!("seed {seed}: goal unreachable (bug)"))?;
                    Ok((m, route))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("maze thread panicked")).collect()
    });
    built.into_iter().collect()
}

fn run_book(argv: &[String]) -> Result<(), String> {
    let mut out = "book.pdf".to_string();
    let mut book_seed = None;
    let mut signature = None;
    let mut it = argv.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "-o" | "--out" => out = value.clone(),
            "--seed" => {
                book_seed = Some(seed_code::decode(value).ok_or_else(|| format!("--seed: not a maze code: {value}"))?)
            }
            "--sign" => signature = Some(value.clone()),
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    let book_seed = book_seed.unwrap_or_else(seed_from_clock);
    let sections = book_sections_in_page_order();
    let mut maze_seeds_drawn_from_book_seed = Rng::new(book_seed);
    let jobs: Vec<(&Level, u64)> = sections
        .iter()
        .flat_map(|section| section.levels.iter().copied())
        .map(|l| (l, seed_code::random_seed_with_a_letter(&mut maze_seeds_drawn_from_book_seed)))
        .collect();

    let t = Instant::now();
    let built = build_all_at_once(&jobs)?;
    eprintln!(
        "built {} mazes in {:.1} s (book code {})",
        built.len(),
        t.elapsed().as_secs_f64(),
        seed_code::encode(book_seed)
    );

    let mut doc = pdf::Pdf::new();
    let mut k = 0;
    for section in &sections {
        let items: Vec<render::Item> = section
            .levels
            .iter()
            .map(|l| {
                let (m, route) = &built[k];
                let item = render::Item {
                    maze: m,
                    seed_printed_as_code: Some(jobs[k].1),
                    solution: Some(route),
                    solution_drawn_on_maze: false,
                    label: l.label.to_string(),
                };
                k += 1;
                item
            })
            .collect();
        let opts = render::Options {
            per_page: section.mazes_per_page,
            graph_paper: false,
            zone_tints_on_answers: false,
            signature_once_a_page: signature.clone(),
        };
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

fn build_book_sizes_and_print_stats(first: u64, last: u64) {
    let mut all = goals::Scores::default();
    for seed in first..last {
        let l = &levels::LEVELS[seed as usize % levels::LEVELS.len()];
        let (cols, rows) = (l.cols, l.rows);
        let mut rng = Rng::new(seed);
        let t = Instant::now();
        let m = Maze::generate(rows, cols, &mut rng);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let scores = goals::score(&m);
        all.add(&scores);
        let cells = rows * cols;
        let solid = (0..cells).filter(|&c| m.is_solid(c)).count();
        let dead_ends = (0..cells).filter(|&c| m.degree(c) == 1 && c != m.start && c != m.goal).count();
        let cut_off = cells - solid - m.reachable_from_start().iter().filter(|&&r| r).count();
        println!(
            "seed {seed} ({cols}x{rows}): {ms:>4.0} ms  score {:.2}  {} zones, {solid} solid, {dead_ends} dead ends, {cut_off} cut off, {} bridges{}",
            scores.total(),
            m.zone_count,
            m.bridges.len(),
            if m.breaks_a_rule() { "  BREAKS A RULE" } else { "" },
        );
    }
    let mazes = (last - first).max(1) as f64;
    println!("average over {} mazes:", last - first);
    for (name, weight, score) in all.weighted() {
        println!("  {name:<22} {:>3.0}%  score {:.2}  adds {:.3}", weight * 100.0, score / mazes, weight * score / mazes);
    }
    println!("  {:<22}        total {:.3}", "", all.total() / mazes);
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
        let seed_arg = |i: usize, default: u64| argv.get(i).and_then(|v| v.parse().ok()).unwrap_or(default);
        build_book_sizes_and_print_stats(seed_arg(1, 0), seed_arg(2, 12));
        return ExitCode::SUCCESS;
    }
    match parse(&argv).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("amaze: {e}");
            ExitCode::FAILURE
        }
    }
}
