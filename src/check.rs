//! Check finished mazes against amaze's rules.
//!
//! Everything is re-derived from the finished walls, bridges and zones
//! alone, independently of how the generator reasons about them:
//!
//!   * Walls open symmetrically and never off the board.
//!   * Bridges run straight between their landings over the cells listed.
//!   * Under a single bridge, a cell is either solid (no openings) or carries
//!     one straight passage across the bridge. Never two passages side by
//!     side under the same bridge. A cell where two bridges cross has no floor.
//!   * Bridges never run along on top of each other, and never land under one.
//!   * No opening runs into a floorless cell.
//!   * The start and finish each have exactly one way out; no other cell is a
//!     dead end, and none is sealed off.
//!   * No loop of four cells: a lone pillar of wall in a little open room.
//!   * Every cell with a floor can be reached from the start.
//!   * The only links between zones are the designated crossings, and each
//!     crossing is the only way through.

use std::collections::{HashSet, VecDeque};

use crate::maze::{opp, Maze, Orient};

pub fn check(m: &Maze) -> Vec<String> {
    let (rows, cols) = (m.rows, m.cols);
    let n = rows * cols;
    let rc = |c: usize| (c / cols, c % cols);
    let mut errs = Vec::new();

    for c in 0..n {
        for d in 0..4 {
            if !m.is_open(c, d) {
                continue;
            }
            match m.step(c, d) {
                None => errs.push(format!("opening off the board at {:?}", rc(c))),
                Some(x) if !m.is_open(x, opp(d)) => errs.push(format!("one-sided opening at {:?}", rc(c))),
                _ => {}
            }
        }
    }

    // Bridges: straight, mids exactly the cells between the landings.
    let mut land_dirs: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut under: Vec<u8> = vec![0; n];
    let mut ends = HashSet::new();
    for b in &m.bridges {
        let ((ra, ca), (rb, cb)) = (rc(b.a), rc(b.b));
        let (d, back, o) = if ra == rb && ca < cb {
            (3, 2, Orient::H)
        } else if ca == cb && ra < rb {
            (1, 0, Orient::V)
        } else {
            errs.push(format!("bridge {:?}-{:?} is not straight", rc(b.a), rc(b.b)));
            continue;
        };
        if o != b.o {
            errs.push(format!("bridge {:?} has the wrong orientation", rc(b.a)));
        }
        let mut between = Vec::new();
        let mut x = m.step(b.a, d).unwrap();
        while x != b.b {
            between.push(x);
            x = m.step(x, d).unwrap();
        }
        if between.is_empty() || between != b.mids {
            errs.push(format!("bridge {:?} spans the wrong cells", rc(b.a)));
        }
        land_dirs[b.a].push(d);
        land_dirs[b.b].push(back);
        ends.insert(b.a);
        ends.insert(b.b);
        for &x in &between {
            if under[x] & o.bit() != 0 {
                errs.push(format!("parallel bridges over {:?}", rc(x)));
            }
            under[x] |= o.bit();
        }
    }
    let ops = |c: usize| -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter(|&d| m.is_open(c, d)).collect();
        out.extend(&land_dirs[c]);
        out
    };

    let mut floorless = vec![false; n];
    for x in 0..n {
        if under[x] == 0 {
            continue;
        }
        if ends.contains(&x) {
            errs.push(format!("bridge landing {:?} is under another bridge", rc(x)));
        }
        if under[x] == 3 || ops(x).is_empty() {
            if !ops(x).is_empty() {
                errs.push(format!("bridge crossing {:?} has floor openings", rc(x)));
            }
            floorless[x] = true;
        } else {
            // Under an east-west bridge the passage runs north-south.
            let across: Vec<usize> = if under[x] == 1 { vec![0, 1] } else { vec![2, 3] };
            let mut got = ops(x);
            got.sort();
            if got != across {
                errs.push(format!("cell under bridge {:?} has openings {:?}", rc(x), got));
            }
        }
    }
    for b in &m.bridges {
        for w in b.mids.windows(2) {
            if !floorless[w[0]] && !floorless[w[1]] {
                errs.push(format!("two passages side by side under a bridge at {:?}", rc(w[0])));
            }
        }
    }
    for c in 0..n {
        if floorless[c] && m.open[c] != 0 {
            errs.push(format!("opening into floorless cell {:?}", rc(c)));
        }
    }
    for x in [m.start, m.goal] {
        if ops(x).len() != 1 {
            errs.push(format!("start/finish {:?} should have one way out, has {:?}", rc(x), ops(x)));
        }
    }
    for x in 0..n {
        if under[x] != 0 || x == m.start || x == m.goal {
            continue;
        }
        match ops(x).len() {
            0 => errs.push(format!("sealed cell {:?}", rc(x))),
            1 => errs.push(format!("dead end at {:?}", rc(x))),
            _ => {}
        }
    }

    for c in 0..n {
        let (r, k) = rc(c);
        if r + 1 < rows
            && k + 1 < cols
            && m.is_open(c, 3)
            && m.is_open(c, 1)
            && m.is_open(c + 1, 1)
            && m.is_open(c + cols, 3)
        {
            errs.push(format!("lone pillar of wall inside {:?}", rc(c)));
        }
    }

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut links = Vec::new();
    for c in 0..n {
        for d in [1, 3] {
            if m.is_open(c, d) {
                let x = m.step(c, d).unwrap();
                links.push((c, x));
            }
        }
    }
    links.extend(m.bridges.iter().map(|b| (b.a, b.b)));
    for &(a, b) in &links {
        adj[a].push(b);
        adj[b].push(a);
    }
    let reach = |skip: Option<(usize, usize)>| -> Vec<bool> {
        let mut seen = vec![false; n];
        seen[m.start] = true;
        let mut q = VecDeque::from([m.start]);
        while let Some(x) = q.pop_front() {
            for &y in &adj[x] {
                if skip == Some((x.min(y), x.max(y))) || seen[y] {
                    continue;
                }
                seen[y] = true;
                q.push_back(y);
            }
        }
        seen
    };
    let total = (0..n).filter(|&c| !floorless[c]).count();
    let reached = reach(None).iter().filter(|&&v| v).count();
    if reached != total {
        errs.push(format!("only {reached} of {total} cells reachable"));
    }
    let cuts: HashSet<(usize, usize)> = m.cuts.iter().copied().collect();
    let mut extra: Vec<(usize, usize)> = links
        .iter()
        .map(|&(a, b)| (a.min(b), a.max(b)))
        .filter(|&(a, b)| m.zone[a] != m.zone[b] && !cuts.contains(&(a, b)))
        .collect();
    extra.sort();
    if !extra.is_empty() {
        let shown: Vec<_> = extra.iter().take(3).map(|&(a, b)| (rc(a), rc(b))).collect();
        errs.push(format!("links between zones beyond the {} crossings: {:?}", m.cuts.len(), shown));
    }
    if m.cuts.len() + 1 != m.n_zones {
        errs.push(format!("{} zones but {} crossings", m.n_zones, m.cuts.len()));
    }
    for &e in &m.cuts {
        if reach(Some(e))[m.goal] {
            errs.push(format!("zone crossing {:?} is not the only way through", (rc(e.0), rc(e.1))));
        }
    }
    errs
}
