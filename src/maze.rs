//! Maze generation.
//!
//! The maze is split into zones: tangled strands that wind through each
//! other like spaghetti, crossing over and under one another on bridges but
//! never connecting. Each zone has exactly one way across to the next, so
//! the route must pass through every zone in turn.
//!
//! Generation never repairs or relaxes anything after the fact. Every step
//! keeps promises about each zone, checked as the zones grow:
//!   * the zone is one connected piece (counting its bridges),
//!   * every cell in it has at least two walls it may open, and
//!   * no 2x2 square of it can only ever loop round a lone pillar of wall.
//!
//! The second promise is what lets every dead end be closed into a loop at
//! the end: a dead end has used one of its openings, so another is free.
//!
//! Bridges are never placed to meet a count. A bridge appears where it is
//! needed: where one strand has to cross another while the zones grow, or
//! where a corridor being carved is boxed in and has to hop over a passage
//! to reach new ground.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use crate::rng::Rng;

// Directions: north, south, west, east.
pub const DR: [isize; 4] = [-1, 1, 0, 0];
pub const DC: [isize; 4] = [0, 0, -1, 1];
pub const NONE: u8 = u8::MAX;

pub fn opp(d: usize) -> usize {
    d ^ 1
}

/// Open-wall bits for a straight passage across a bridge running along `o`.
fn across_bits(o: Orient) -> u8 {
    match o {
        Orient::H => 0b0011, // bridge runs east-west, passage north-south
        Orient::V => 0b1100,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orient {
    H,
    V,
}

impl Orient {
    /// The orientation of a step in direction d.
    pub fn of(d: usize) -> Orient {
        if d >= 2 {
            Orient::H
        } else {
            Orient::V
        }
    }
    pub fn bit(self) -> u8 {
        match self {
            Orient::H => 1,
            Orient::V => 2,
        }
    }
    pub fn other(self) -> Orient {
        match self {
            Orient::H => Orient::V,
            Orient::V => Orient::H,
        }
    }
}

/// A straight path from landing `a` to landing `b` (a above or left of b)
/// across the cells `mids`, over or under whatever it crosses there.
#[derive(Clone, Debug)]
pub struct Bridge {
    pub a: usize,
    pub b: usize,
    pub o: Orient,
    pub mids: Vec<usize>,
}

// Zone layout.
const ZONE_BLOCK: usize = 3; // strands are built from blocks this many cells wide
const ZONE_SNAKE: f64 = 0.85; // how often a zone grows from its newest block
const CROSS_WEIGHT: f64 = 3.0; // pull toward crossing over another strand
const UNDER_WEIGHT: f64 = 3.0; // pull toward slipping under another strand
const CROSS_BLOCKS: usize = 2; // most blocks one strand crossing may span
const RING_WEIGHT: f64 = 6.0; // pull toward closing a ring round another strand
const RING_BLOCKS: usize = 6; // fewest blocks round a ring
const ZONE_WIGGLE: usize = 3; // edge-wiggling passes (times the cell count)
const ZONE_FLOOR: f64 = 0.8; // wiggling never shrinks a zone below this share

// Carving.
const P_NEWEST: f64 = 0.6; // carve on from the newest cell; else any (branches)
const STRAIGHT_WEIGHT: f64 = 0.3; // carrying straight on, against turning
const POCKET_PULL: f64 = 1.0; // pull toward cells with fewer ways on
const HOP_MIDS: usize = 3; // most cells a carving hop may pass over
const MIN_LOOP: usize = 8; // loops closed at dead ends prefer at least this
const END_TRIALS: usize = 12; // first crossings tried when choosing the route

#[derive(Clone)]
pub struct Maze {
    pub rows: usize,
    pub cols: usize,
    pub n_zones: usize,
    pub zone: Vec<u8>,
    /// Open-wall bits per cell (bit d: the wall toward direction d is open).
    pub open: Vec<u8>,
    /// Orientation bits of the bridges overhead (both: a crossing).
    pub under: Vec<u8>,
    /// Solid ground under a bridge: no floor.
    pub solid: Vec<bool>,
    /// For each landing, the far ends of its bridges.
    pub partners: Vec<Vec<usize>>,
    pub bridges: Vec<Bridge>,
    /// Crossing cell -> index of the bridge on top there.
    pub on_top: HashMap<usize, usize>,
    pub cuts: Vec<(usize, usize)>,
    pub start: usize,
    pub goal: usize,
    pub stats: Stats,
}

#[derive(Clone, Default, Debug)]
pub struct Stats {
    pub layouts: usize,     // zone layouts drawn before one could be ordered
    pub crossings: usize,   // bridges where one strand crosses another
    pub hops: usize,        // bridges where a boxed-in corridor hopped over
    pub rings: usize,       // strands closed into a ring round another strand
}

/// Rows (or columns) split into blocks ZONE_BLOCK wide; the last takes
/// any left over.
fn spans(n: usize) -> Vec<(usize, usize)> {
    let b = ZONE_BLOCK;
    let k = (n / b).max(1);
    (0..k).map(|i| (b * i, if i + 1 == k { n } else { b * i + b })).collect()
}

/// The block grid zones are grown on, so every strand is at least
/// ZONE_BLOCK cells thick, with room inside for loops of its own.
struct Layout {
    rows: Vec<(usize, usize)>,
    cols: Vec<(usize, usize)>,
    owner: Vec<u8>,
    cells: Vec<Vec<usize>>,
    /// For each block, the blocks of its own strand it reaches by bridge.
    links: Vec<Vec<usize>>,
}

impl Layout {
    fn nbc(&self) -> usize {
        self.cols.len()
    }
    fn step(&self, b: usize, d: usize) -> Option<usize> {
        let i = (b / self.nbc()) as isize + DR[d];
        let j = (b % self.nbc()) as isize + DC[d];
        if i < 0 || j < 0 || i >= self.rows.len() as isize || j >= self.nbc() as isize {
            None
        } else {
            Some(i as usize * self.nbc() + j as usize)
        }
    }
    fn own_adjacent(&self, b: usize, z: u8) -> usize {
        (0..4).filter_map(|d| self.step(b, d)).filter(|&x| self.owner[x] == z).count()
    }

    /// The blocks of strand z joined to b, side by side or by bridge.
    fn strand_next(&self, b: usize, z: u8) -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter_map(|d| self.step(b, d)).filter(|&x| self.owner[x] == z).collect();
        out.extend(&self.links[b]);
        out
    }

    /// Would strand z growing into `via`, a block joining its blocks a and
    /// b (`bridged`: b across a bridge), close a ring that winds round
    /// another strand? The ring is the shortest way back from b to a along
    /// the strand. It must be at least RING_BLOCKS round, and either cross a
    /// bridge (every bridge passes over or under another strand) or have
    /// another strand's block inside it.
    fn closes_ring(&self, z: u8, a: usize, b: usize, via: usize, bridged: bool) -> bool {
        let mut prev = vec![usize::MAX; self.owner.len()];
        prev[b] = b;
        let mut q = VecDeque::from([b]);
        while let Some(x) = q.pop_front() {
            if x == a {
                break;
            }
            for y in self.strand_next(x, z) {
                if prev[y] == usize::MAX {
                    prev[y] = x;
                    q.push_back(y);
                }
            }
        }
        if prev[a] == usize::MAX {
            return false;
        }
        // Round the ring: a back along the strand to b, then via (if any).
        let mut ring = vec![a];
        let mut bridged = bridged;
        while *ring.last().unwrap() != b {
            let x = *ring.last().unwrap();
            bridged |= self.links[x].contains(&prev[x]);
            ring.push(prev[x]);
        }
        ring.push(via);
        ring.push(a);
        if ring.len() - 1 < RING_BLOCKS {
            return false;
        }
        if bridged {
            return true;
        }
        // Another strand's block inside: count the ring's upright sides to
        // its right, just below the block's middle.
        let nbc = self.nbc();
        let sides: Vec<(usize, f64, f64)> = ring
            .windows(2)
            .filter(|w| w[0] % nbc == w[1] % nbc)
            .map(|w| {
                let (ra, rb) = ((w[0] / nbc) as f64, (w[1] / nbc) as f64);
                (w[0] % nbc, ra.min(rb), ra.max(rb))
            })
            .collect();
        (0..self.owner.len()).any(|x| {
            let w = self.owner[x];
            let y = (x / nbc) as f64 + 0.01;
            w != NONE
                && w != z
                && sides.iter().filter(|&&(k, lo, hi)| k > x % nbc && lo < y && y < hi).count() % 2 == 1
        })
    }
}

enum Close {
    Wall(usize),
    Hop(usize),
}

enum Grow {
    /// Into open ground beside the strand.
    Step(usize),
    /// From a block, over other strands, to open ground beyond.
    Over(usize, usize),
    /// Into open ground between two blocks of another strand, which
    /// bridges over it.
    Under(usize, usize, usize),
}

impl Maze {
    fn blank(rows: usize, cols: usize, n_zones: usize) -> Maze {
        let n = rows * cols;
        Maze {
            rows,
            cols,
            n_zones,
            zone: vec![NONE; n],
            open: vec![0; n],
            under: vec![0; n],
            solid: vec![false; n],
            partners: vec![Vec::new(); n],
            bridges: Vec::new(),
            on_top: HashMap::new(),
            cuts: Vec::new(),
            start: 0,
            goal: 0,
            stats: Stats::default(),
        }
    }

    pub fn generate(rows: usize, cols: usize, n_zones: usize, rng: &mut Rng) -> Result<Maze, String> {
        if rows < 4 || cols < 4 {
            return Err("a maze needs at least 4 rows and 4 columns".into());
        }
        if n_zones == 0 || n_zones > max_zones(rows, cols) {
            return Err(format!("{n_zones} zones will not fit a {cols}x{rows} maze"));
        }
        // A zone layout can come out impossible to order (no chain of zones
        // each touching the next). Then draw a fresh layout with the same
        // rules; nothing else is ever redone.
        for layout in 1..=200 {
            let mut m = Maze::blank(rows, cols, n_zones);
            if !m.make_zones(rng) {
                continue;
            }
            m.carve(rng);
            m.braid(rng);
            m.break_pillars(rng);
            if !m.place_route(rng) {
                continue;
            }
            m.weave(rng);
            m.stats.layouts = layout;
            return Ok(m);
        }
        Err(format!("no workable zone layout for {n_zones} zones in a {cols}x{rows} maze"))
    }

    // -- geometry ---------------------------------------------------------
    pub fn step(&self, c: usize, d: usize) -> Option<usize> {
        let r = (c / self.cols) as isize + DR[d];
        let k = (c % self.cols) as isize + DC[d];
        if r < 0 || k < 0 || r >= self.rows as isize || k >= self.cols as isize {
            None
        } else {
            Some(r as usize * self.cols + k as usize)
        }
    }

    pub fn floorless(&self, c: usize) -> bool {
        self.solid[c] || self.under[c] == 3
    }

    /// May cell c open toward direction d? Not without a floor, and a
    /// passage under a bridge only runs straight across it.
    fn passable(&self, c: usize, d: usize) -> bool {
        !self.floorless(c)
            && match self.under[c] {
                0 => true,
                1 => d < 2,
                _ => d >= 2,
            }
    }

    /// The neighbour beyond the wall toward d, if that wall may be opened.
    pub fn can_open(&self, c: usize, d: usize, same_zone: bool) -> Option<usize> {
        let n = self.step(c, d)?;
        if !self.passable(c, d) || !self.passable(n, d) {
            return None;
        }
        if same_zone && (self.zone[c] == NONE || self.zone[c] != self.zone[n]) {
            return None;
        }
        Some(n)
    }

    pub fn is_open(&self, c: usize, d: usize) -> bool {
        self.open[c] >> d & 1 == 1
    }

    fn knock(&mut self, c: usize, d: usize) {
        let n = self.step(c, d).unwrap();
        self.open[c] |= 1 << d;
        self.open[n] |= 1 << opp(d);
    }

    pub fn degree(&self, c: usize) -> usize {
        self.open[c].count_ones() as usize + self.partners[c].len()
    }

    pub fn links(&self, c: usize) -> impl Iterator<Item = usize> + '_ {
        (0..4)
            .filter(move |&d| self.is_open(c, d))
            .map(move |d| self.step(c, d).unwrap())
            .chain(self.partners[c].iter().copied())
    }

    /// Directions in which this cell visibly opens, bridges included.
    pub fn openings(&self, c: usize) -> Vec<usize> {
        let mut out: Vec<usize> = (0..4).filter(|&d| self.is_open(c, d)).collect();
        for &p in &self.partners[c] {
            out.push(self.dir_to(c, p));
        }
        out
    }

    /// Direction from c straight toward p.
    pub fn dir_to(&self, c: usize, p: usize) -> usize {
        let (r0, c0) = (c / self.cols, c % self.cols);
        let (r1, c1) = (p / self.cols, p % self.cols);
        if r0 == r1 {
            if c1 < c0 { 2 } else { 3 }
        } else if r1 < r0 {
            0
        } else {
            1
        }
    }

    /// Lay a bridge from a to b, the cells between it marked `solid` (true)
    /// or carrying a passage across (false).
    fn add_bridge(&mut self, a: usize, b: usize, solid: &[bool]) {
        let (a, b) = (a.min(b), a.max(b));
        let d = self.dir_to(a, b);
        let o = Orient::of(d);
        let mut mids = Vec::new();
        let mut x = self.step(a, d).unwrap();
        while x != b {
            mids.push(x);
            x = self.step(x, d).unwrap();
        }
        for (i, &m) in mids.iter().enumerate() {
            self.under[m] |= o.bit();
            if solid[i] {
                self.solid[m] = true;
            }
        }
        self.partners[a].push(b);
        self.partners[b].push(a);
        self.bridges.push(Bridge { a, b, o, mids });
    }

    // -- zones ------------------------------------------------------------
    /// Is zone z one connected piece in which every cell has at least two
    /// walls it may open (or bridges), with no 2x2 square whose only
    /// possible loop is round a lone pillar of wall?
    fn zone_sound(&self, z: u8) -> bool {
        let mut first = None;
        let mut count = 0;
        for c in 0..self.zone.len() {
            if self.zone[c] != z || self.floorless(c) {
                continue;
            }
            count += 1;
            first.get_or_insert(c);
            let ways = (0..4).filter(|&d| self.can_open(c, d, true).is_some()).count()
                + self.partners[c].len();
            if ways < 2 {
                return false;
            }
        }
        let Some(first) = first else { return false };
        if self.has_knob(z) {
            return false;
        }
        let mut seen = vec![false; self.zone.len()];
        seen[first] = true;
        let mut q = VecDeque::from([first]);
        let mut reached = 1;
        while let Some(c) = q.pop_front() {
            let nbrs = (0..4).filter_map(|d| self.can_open(c, d, true));
            for n in nbrs.chain(self.partners[c].iter().copied()).collect::<Vec<_>>() {
                if !seen[n] {
                    seen[n] = true;
                    reached += 1;
                    q.push_back(n);
                }
            }
        }
        reached == count
    }

    /// Is there a 2x2 square of zone z with two diagonally opposite cells
    /// that can only open into the square? Each needs both of the other two
    /// cells, which closes the square into a loop round a lone pillar.
    fn has_knob(&self, z: u8) -> bool {
        (0..self.zone.len()).any(|c| self.forced_square(c, z, usize::MAX))
    }

    /// Is the 2x2 square with top-left cell c such a square, if cell `skip`
    /// were out of the running?
    fn forced_square(&self, c: usize, z: u8, skip: usize) -> bool {
        let cols = self.cols;
        if c % cols + 1 >= cols || c / cols + 1 >= self.rows {
            return false;
        }
        let sq = [c, c + 1, c + cols, c + cols + 1];
        if sq.iter().any(|&x| x == skip || self.zone[x] != z || self.floorless(x)) {
            return false;
        }
        let inward = |x: usize| {
            !(0..4).any(|d| self.can_open(x, d, true).is_some_and(|y| y != skip && !sq.contains(&y)))
                && self.partners[x].is_empty()
        };
        (inward(sq[0]) && inward(sq[3])) || (inward(sq[1]) && inward(sq[2]))
    }

    /// Grow the zones as strands of blocks, from seeds spread across the
    /// map. Each zone in turn adds a block touching exactly one of its own,
    /// so it stays a thin strand, mostly from its newest block, so it snakes
    /// along. Where another strand is in the way it may cross over it on a
    /// bridge, or slip under it while the other strand bridges over: strands
    /// coil over and under each other like spaghetti.
    fn make_zones(&mut self, rng: &mut Rng) -> bool {
        let n = self.n_zones;
        let rows = spans(self.rows);
        let cols = spans(self.cols);
        let width = self.cols;
        let mut cells = Vec::new();
        for &(r0, r1) in &rows {
            for &(c0, c1) in &cols {
                cells.push((r0..r1).flat_map(|r| (c0..c1).map(move |c| r * width + c)).collect());
            }
        }
        let nb = cells.len();
        let mut lay = Layout { rows, cols, owner: vec![NONE; nb], cells, links: vec![Vec::new(); nb] };

        // Seeds as far apart as they can be, so no strand starts boxed in.
        let nbc = lay.nbc();
        let gap = |a: usize, b: usize| (a / nbc).abs_diff(b / nbc) + (a % nbc).abs_diff(b % nbc);
        let mut seeds = vec![rng.below(nb)];
        while seeds.len() < n {
            let far = (0..nb)
                .filter(|b| !seeds.contains(b))
                .max_by_key(|&b| (seeds.iter().map(|&s| gap(s, b)).min().unwrap(), rng.below(1000)))
                .unwrap();
            seeds.push(far);
        }
        rng.shuffle(&mut seeds);
        let mut size = vec![1usize; n];
        let mut tip = seeds.clone();
        for (z, &b) in seeds.iter().enumerate() {
            self.claim(&mut lay, b, z as u8);
        }
        let mut active = vec![true; n];
        loop {
            let live: Vec<usize> = (0..n).filter(|&z| active[z]).collect();
            let Some(low) = live.iter().map(|&z| size[z]).min() else { break };
            let smallest: Vec<usize> = live.into_iter().filter(|&z| size[z] == low).collect();
            let z = rng.pick(&smallest);
            let mut opts = if rng.chance(ZONE_SNAKE) {
                self.grow_options(&lay, z as u8, &[tip[z]])
            } else {
                Vec::new()
            };
            if opts.is_empty() {
                let mine: Vec<usize> = (0..nb).filter(|&b| lay.owner[b] == z as u8).collect();
                opts = self.grow_options(&lay, z as u8, &mine);
            }
            let mut grown = false;
            while !opts.is_empty() {
                let w: Vec<f64> = opts.iter().map(|o| o.1).collect();
                let (o, _) = opts.swap_remove(rng.weighted(&w));
                if let Some(t) = self.try_grow(&mut lay, z as u8, o, rng) {
                    tip[z] = t;
                    size[z] += 1;
                    grown = true;
                    break;
                }
            }
            if !grown {
                active[z] = false;
            }
        }

        // Blocks no strand could reach without touching itself go to a zone
        // beside them, the smallest that stays sound.
        loop {
            let mut left: Vec<usize> = (0..nb).filter(|&b| lay.owner[b] == NONE).collect();
            if left.is_empty() {
                break;
            }
            rng.shuffle(&mut left);
            let mut progress = false;
            for b in left {
                let mut near: Vec<u8> = (0..4)
                    .filter_map(|d| lay.step(b, d))
                    .map(|x| lay.owner[x])
                    .filter(|&z| z != NONE)
                    .collect();
                near.sort_by_key(|&z| (size[z as usize], z));
                near.dedup();
                for z in near {
                    let saved = self.clone();
                    self.claim(&mut lay, b, z);
                    if self.zone_sound(z) {
                        size[z as usize] += 1;
                        progress = true;
                        break;
                    }
                    *self = saved;
                    lay.owner[b] = NONE;
                }
            }
            if !progress {
                return false;
            }
        }
        self.roughen_zones(rng);
        self.order_zones(rng)
    }

    fn claim(&mut self, lay: &mut Layout, b: usize, z: u8) {
        lay.owner[b] = z;
        for &c in &lay.cells[b] {
            self.zone[c] = z;
        }
    }

    /// Ways zone z could grow from blocks `from`, with their weights.
    fn grow_options(&self, lay: &Layout, z: u8, from: &[usize]) -> Vec<(Grow, f64)> {
        let mut out = Vec::new();
        for &s in from {
            for d in 0..4 {
                let Some(t) = lay.step(s, d) else { continue };
                if lay.owner[t] == NONE {
                    // Back beside the strand's own blocks: close a ring.
                    if lay.own_adjacent(t, z) == 2 {
                        let own: Vec<usize> =
                            (0..4).filter_map(|e| lay.step(t, e)).filter(|&x| lay.owner[x] == z).collect();
                        if lay.closes_ring(z, own[0], own[1], t, false) {
                            out.push((Grow::Step(t), RING_WEIGHT));
                        }
                        continue;
                    }
                    if lay.own_adjacent(t, z) != 1 {
                        continue;
                    }
                    // Head for open ground, so strands don't box themselves in.
                    let free = (0..4).filter_map(|e| lay.step(t, e)).filter(|&x| lay.owner[x] == NONE).count();
                    out.push((Grow::Step(t), 0.5 + free as f64));
                    // Between two blocks of one other strand: slip under it.
                    let side = if d >= 2 { 0 } else { 2 };
                    if let (Some(f1), Some(f2)) = (lay.step(t, side), lay.step(t, side + 1)) {
                        let w = lay.owner[f1];
                        if w != NONE && w != z && lay.owner[f2] == w {
                            out.push((Grow::Under(t, f1, f2), UNDER_WEIGHT));
                        }
                    }
                    continue;
                }
                // Over one or more blocks of other strands, to open ground.
                // Long bridges read as long straight runs: favour short ones.
                let mut x = t;
                for k in 1..=CROSS_BLOCKS {
                    if lay.owner[x] == z || lay.owner[x] == NONE {
                        break;
                    }
                    let Some(beyond) = lay.step(x, d) else { break };
                    if lay.owner[beyond] == NONE {
                        match lay.own_adjacent(beyond, z) {
                            0 => out.push((Grow::Over(s, beyond), CROSS_WEIGHT / (k * k) as f64)),
                            // Over another strand and back beside its own
                            // blocks: a ring through the bridge.
                            1 => {
                                let q = (0..4).filter_map(|e| lay.step(beyond, e)).find(|&x| lay.owner[x] == z).unwrap();
                                if lay.closes_ring(z, q, s, beyond, true) {
                                    out.push((Grow::Over(s, beyond), RING_WEIGHT / (k * k) as f64));
                                }
                            }
                            _ => {}
                        }
                        break;
                    }
                    x = beyond;
                }
            }
        }
        out
    }

    /// The lines a bridge from block `from` straight to block `to` could
    /// run along, as (landing, landing) pairs on the facing edges.
    fn bridge_lines(&self, lay: &Layout, from: usize, to: usize) -> Vec<(usize, usize)> {
        let nbc = lay.nbc();
        let (fb, tb) = ((from / nbc, from % nbc), (to / nbc, to % nbc));
        let w = self.cols;
        if fb.0 == tb.0 {
            let (c_from, c_to) =
                if tb.1 > fb.1 { (lay.cols[fb.1].1 - 1, lay.cols[tb.1].0) } else { (lay.cols[fb.1].0, lay.cols[tb.1].1 - 1) };
            let (r0, r1) = lay.rows[fb.0];
            (r0..r1).map(|r| (r * w + c_from, r * w + c_to)).collect()
        } else {
            let (r_from, r_to) =
                if tb.0 > fb.0 { (lay.rows[fb.0].1 - 1, lay.rows[tb.0].0) } else { (lay.rows[fb.0].0, lay.rows[tb.0].1 - 1) };
            let (c0, c1) = lay.cols[fb.1];
            (c0..c1).map(|c| (r_from * w + c, r_to * w + c)).collect()
        }
    }

    /// Lay a bridge from block `from` to block `to` along one of its possible
    /// lines, if every zone it touches stays sound. Under it each cell is
    /// solid or carries a passage across, never two passages side by side
    /// (that reads as a ladder of little rooms); most passages first, for
    /// more weave. Cells already under a crossing bridge must be solid.
    fn try_bridge(&mut self, lay: &Layout, from: usize, to: usize, rng: &mut Rng) -> bool {
        let mut lines = self.bridge_lines(lay, from, to);
        rng.shuffle(&mut lines);
        for (a, b) in lines {
            if !self.plain(a) || !self.plain(b) {
                continue;
            }
            let o = Orient::of(self.dir_to(a, b));
            let mids = self.cells_between(a.min(b), a.max(b));
            let mut forced = 0u32; // mids that must stay solid
            let mut ok = true;
            for (i, &m) in mids.iter().enumerate() {
                if !self.partners[m].is_empty() || self.under[m] & o.bit() != 0 {
                    ok = false;
                } else if self.under[m] != 0 {
                    if !self.solid[m] {
                        ok = false; // a passage running along this line
                    }
                    forced |= 1 << i;
                }
            }
            if !ok {
                continue;
            }
            let mut patterns: Vec<u32> =
                (0..1u32 << mids.len()).filter(|p| p & (p >> 1) == 0 && p & forced == 0).collect();
            rng.shuffle(&mut patterns);
            patterns.sort_by_key(|p| Reverse(p.count_ones()));
            let mut zones: Vec<u8> = mids.iter().map(|&m| self.zone[m]).collect();
            zones.push(self.zone[a]);
            zones.sort();
            zones.dedup();
            for p in patterns {
                let solid: Vec<bool> = (0..mids.len()).map(|i| p >> i & 1 == 0).collect();
                let saved = self.clone();
                self.add_bridge(a, b, &solid);
                if zones.iter().all(|&z| self.zone_sound(z)) {
                    self.stats.crossings += 1;
                    return true;
                }
                *self = saved;
            }
        }
        false
    }

    /// Carry out a growth step if every zone it touches stays sound.
    /// Returns the block grown into.
    fn try_grow(&mut self, lay: &mut Layout, z: u8, o: Grow, rng: &mut Rng) -> Option<usize> {
        let saved = self.clone();
        let t = match o {
            Grow::Step(t) | Grow::Over(_, t) | Grow::Under(t, ..) => t,
        };
        let ring = match o {
            Grow::Step(t) => lay.own_adjacent(t, z) == 2,
            Grow::Over(_, t) => lay.own_adjacent(t, z) == 1,
            Grow::Under(..) => false,
        };
        self.claim(lay, t, z);
        let ok = match o {
            Grow::Step(_) => self.zone_sound(z),
            Grow::Over(s, t) => self.try_bridge(lay, s, t, rng),
            Grow::Under(_, f1, f2) => self.zone_sound(z) && self.try_bridge(lay, f1, f2, rng),
        };
        if ok {
            match o {
                Grow::Over(s, t) => {
                    lay.links[s].push(t);
                    lay.links[t].push(s);
                }
                Grow::Under(_, f1, f2) => {
                    lay.links[f1].push(f2);
                    lay.links[f2].push(f1);
                }
                Grow::Step(_) => {}
            }
            self.stats.rings += ring as usize;
            return Some(t);
        }
        *self = saved;
        lay.owner[t] = NONE;
        None
    }

    /// Zone edges built from blocks run dead straight, and since no path
    /// crosses them they show as long straight walls. Wiggle them: move
    /// edge cells into the zone next door, one at a time or two side by side
    /// along the edge (a lone cell pushed into a straight edge would have
    /// only one neighbour in its new zone), whenever both zones stay sound
    /// and neither gets too small.
    fn roughen_zones(&mut self, rng: &mut Rng) {
        let n = self.zone.len();
        let mut size = vec![0usize; self.n_zones];
        for &z in &self.zone {
            size[z as usize] += 1;
        }
        let floor = *size.iter().min().unwrap() as f64 * ZONE_FLOOR;
        for _ in 0..ZONE_WIGGLE * n {
            let x = rng.below(n);
            let z = self.zone[x];
            let toward: Vec<usize> =
                (0..4).filter(|&d| self.step(x, d).is_some_and(|m| self.zone[m] != z)).collect();
            if toward.is_empty() || !self.plain(x) {
                continue;
            }
            let d = rng.pick(&toward);
            let w = self.zone[self.step(x, d).unwrap()];
            let mut group = vec![x];
            if rng.chance(0.5) {
                let e = if d >= 2 { rng.below(2) } else { 2 + rng.below(2) };
                let Some(y) = self.step(x, e) else { continue };
                if self.zone[y] != z || !self.plain(y) || self.step(y, d).map(|m| self.zone[m]) != Some(w) {
                    continue;
                }
                group.push(y);
            }
            if ((size[z as usize] - group.len()) as f64) < floor {
                continue;
            }
            for &c in &group {
                self.zone[c] = w;
            }
            if self.zone_sound(z) && self.zone_sound(w) {
                size[z as usize] -= group.len();
                size[w as usize] += group.len();
            } else {
                for &c in &group {
                    self.zone[c] = z;
                }
            }
        }
    }

    /// Plain cells (no bridge over or landing on them): where zones may meet.
    fn plain(&self, c: usize) -> bool {
        self.under[c] == 0 && !self.solid[c] && self.partners[c].is_empty()
    }

    /// Number the zones so each touches the next.
    fn order_zones(&mut self, rng: &mut Rng) -> bool {
        let n = self.n_zones;
        let mut touch = vec![vec![false; n]; n];
        for c in 0..self.zone.len() {
            for d in [1, 3] {
                if let Some(m) = self.can_open(c, d, false) {
                    let (a, b) = (self.zone[c] as usize, self.zone[m] as usize);
                    if a != b && self.plain(c) && self.plain(m) {
                        touch[a][b] = true;
                        touch[b][a] = true;
                    }
                }
            }
        }
        // A random chain through every zone, found by depth-first search.
        let mut starts: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut starts);
        let mut perm = Vec::new();
        let mut budget = 100_000;
        if !starts.into_iter().any(|z| {
            perm = vec![z];
            chain(&touch, &mut perm, &mut budget, rng)
        }) {
            return false;
        }
        let mut rank = vec![0u8; n];
        for (i, &z) in perm.iter().enumerate() {
            rank[z] = i as u8;
        }
        for z in &mut self.zone {
            *z = rank[*z as usize];
        }
        true
    }

    // -- carving ----------------------------------------------------------
    /// Carve every zone at once with a growing tree: carry on from the
    /// newest cell or branch off from any earlier one, preferring turns to
    /// straight runs. A corridor boxed in by others hops over one on a new
    /// bridge, if there is open ground of its zone beyond.
    fn carve(&mut self, rng: &mut Rng) {
        let n = self.zone.len();
        let mut visited: Vec<bool> = (0..n).map(|c| self.floorless(c)).collect();
        let mut entry = vec![usize::MAX; n]; // direction each cell was entered by
        let mut lists: Vec<Vec<usize>> = vec![Vec::new(); self.n_zones];
        for (z, list) in lists.iter_mut().enumerate() {
            let cells: Vec<usize> = (0..n).filter(|&c| self.zone[c] == z as u8 && !visited[c]).collect();
            self.visit(rng.pick(&cells), &mut visited, list, rng);
        }
        loop {
            let live: Vec<usize> = (0..self.n_zones).filter(|&z| !lists[z].is_empty()).collect();
            if live.is_empty() {
                break;
            }
            let z = rng.pick(&live);
            let len = lists[z].len();
            let i = if rng.chance(P_NEWEST) { len - 1 } else { rng.below(len) };
            let cur = lists[z][i];
            let mut dirs = Vec::new();
            let mut w = Vec::new();
            for d in 0..4 {
                if let Some(m) = self.can_open(cur, d, true) {
                    if !visited[m] {
                        // Sweep up pockets that would otherwise be left as
                        // one-cell stubs: favour cells with fewer ways on.
                        let free = (0..4)
                            .filter(|&e| self.can_open(m, e, true).is_some_and(|x| !visited[x]))
                            .count();
                        let turn = if entry[cur] == d { STRAIGHT_WEIGHT } else { 1.0 };
                        dirs.push(d);
                        w.push(turn / (1.0 + free as f64).powf(POCKET_PULL));
                    }
                }
            }
            if !dirs.is_empty() {
                let d = dirs[rng.weighted(&w)];
                let m = self.step(cur, d).unwrap();
                self.knock(cur, d);
                entry[m] = d;
                self.visit(m, &mut visited, &mut lists[z], rng);
                continue;
            }
            let hops = self.hop_options(cur, |y| !visited[y]);
            if !hops.is_empty() {
                let (d, y) = rng.pick(&hops);
                self.hop(cur, y);
                entry[y] = d;
                self.visit(y, &mut visited, &mut lists[z], rng);
                continue;
            }
            lists[z].remove(i);
        }
        assert!(visited.iter().all(|&v| v), "zone promise broken: carving left cells unreached");
    }

    /// Bridge from a to b over cells whose floors are already settled.
    fn hop(&mut self, a: usize, b: usize) {
        let (lo, hi) = (a.min(b), a.max(b));
        let solid: Vec<bool> = self.cells_between(lo, hi).iter().map(|&m| self.solid[m]).collect();
        self.add_bridge(lo, hi, &solid);
        self.stats.hops += 1;
    }

    fn cells_between(&self, a: usize, b: usize) -> Vec<usize> {
        let d = self.dir_to(a, b);
        let mut out = Vec::new();
        let mut x = self.step(a, d).unwrap();
        while x != b {
            out.push(x);
            x = self.step(x, d).unwrap();
        }
        out
    }

    /// Mark c visited, with everything joined to it by bridges, and add
    /// them to the carving list with c last (newest).
    fn visit(&self, c: usize, visited: &mut [bool], list: &mut Vec<usize>, rng: &mut Rng) {
        let mut group = vec![c];
        visited[c] = true;
        let mut i = 0;
        while i < group.len() {
            for &p in &self.partners[group[i]] {
                if !visited[p] {
                    visited[p] = true;
                    group.push(p);
                }
            }
            i += 1;
        }
        let mut rest = group.split_off(1);
        rng.shuffle(&mut rest);
        list.extend(rest);
        list.push(c);
    }

    /// Straight hops from cur over finished passages (or solid ground under
    /// a crossing bridge) to the first cell of its own zone that `lands`
    /// accepts, as (direction, landing).
    fn hop_options(&self, cur: usize, lands: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if self.under[cur] != 0 || self.solid[cur] {
            return out;
        }
        for d in 0..4 {
            let o = Orient::of(d);
            let mut x = cur;
            let mut prev_passage = false;
            for k in 1..=HOP_MIDS + 1 {
                let Some(nx) = self.step(x, d) else { break };
                x = nx;
                if k >= 2 && self.zone[x] == self.zone[cur] && self.under[x] == 0 && !self.solid[x] && lands(x) {
                    out.push((d, x));
                    break;
                }
                if k > HOP_MIDS {
                    break;
                }
                // A finished straight passage across the hop's line.
                let passage = self.plain(x) && self.open[x] == across_bits(o);
                let rock = self.solid[x] && self.under[x] == o.other().bit();
                if passage && !prev_passage {
                    prev_passage = true;
                } else if rock {
                    prev_passage = false;
                } else {
                    break;
                }
            }
        }
        out
    }

    // -- route ------------------------------------------------------------
    /// Distance along the paths from src to every cell, a bridge counting
    /// the cells it spans.
    fn distances(&self, src: usize) -> Vec<usize> {
        let mut dist = vec![usize::MAX; self.zone.len()];
        dist[src] = 0;
        let mut heap = BinaryHeap::from([Reverse((0, src))]);
        while let Some(Reverse((d, c))) = heap.pop() {
            if d > dist[c] {
                continue;
            }
            for m in self.links(c) {
                let (r0, c0) = (c / self.cols, c % self.cols);
                let (r1, c1) = (m / self.cols, m % self.cols);
                let nd = d + r0.abs_diff(r1) + c0.abs_diff(c1);
                if nd < dist[m] {
                    dist[m] = nd;
                    heap.push(Reverse((nd, m)));
                }
            }
        }
        dist
    }

    /// Could c become the start or finish? A plain cell with two ways out,
    /// one of them a step of a loop whose far cell has a third: putting
    /// that wall back makes c a dead end and cuts nothing off. Returns the
    /// wall to put back, keeping the way out that is farther along `dist`.
    fn end_wall(&self, c: usize, z: u8, dist: &[usize], taken: &[usize]) -> Option<usize> {
        if self.zone[c] != z || !self.plain(c) || self.degree(c) != 2 || taken.contains(&c) {
            return None;
        }
        let ways: Vec<usize> = (0..4).filter(|&d| self.is_open(c, d)).collect();
        if ways.len() != 2 {
            return None;
        }
        let mut best = None;
        for (i, &d) in ways.iter().enumerate() {
            let x = self.step(c, d).unwrap();
            let keep = self.step(c, ways[1 - i]).unwrap();
            if self.degree(x) < 3 || taken.contains(&x) || dist[keep] < dist[x] {
                continue;
            }
            // c-x lies on a loop if x can still be reached from c the other way.
            let mut seen = vec![c, keep];
            let mut q = VecDeque::from([keep]);
            let mut looped = false;
            while let Some(y) = q.pop_front() {
                if y == x {
                    looped = true;
                    break;
                }
                for m in self.links(y) {
                    if self.zone[m] == z && !seen.contains(&m) {
                        seen.push(m);
                        q.push_back(m);
                    }
                }
            }
            if looped {
                best = Some(d);
            }
        }
        best
    }

    /// Pick the one way across between each zone and the next, and the start
    /// and finish, once every loop is in place, so each zone's stretch of the
    /// route is as long as the shortest way through it can be.
    fn place_route(&mut self, rng: &mut Rng) -> bool {
        let last = self.n_zones - 1;
        let n = self.zone.len();
        let mut options: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); self.n_zones];
        for c in 0..n {
            for d in 0..4 {
                if let Some(m) = self.can_open(c, d, false) {
                    if self.zone[m] == self.zone[c] + 1 && self.plain(c) && self.plain(m) {
                        options[self.zone[c] as usize].push((c, m, d));
                    }
                }
            }
        }
        if options[..last].iter().any(|o| o.is_empty()) {
            return false;
        }
        // The farthest cell of zone z from src that can become an end.
        let farthest_end = |src: usize, z: usize, taken: &[usize]| -> Option<(usize, usize, usize)> {
            let dist = self.distances(src);
            let mut far: Vec<(usize, usize)> =
                (0..n).filter(|&c| dist[c] != usize::MAX && self.zone[c] == z as u8).map(|c| (dist[c], c)).collect();
            far.sort_unstable_by(|a, b| b.cmp(a));
            far.into_iter().find_map(|(dc, c)| self.end_wall(c, z as u8, &dist, taken).map(|d| (dc, c, d)))
        };
        type Plan = (usize, Vec<(usize, usize, usize)>, (usize, usize), (usize, usize));
        let mut best: Option<Plan> = None;
        for _ in 0..END_TRIALS {
            let (start_src, mut cuts) = if self.n_zones == 1 {
                (rng.below(n), Vec::new())
            } else {
                let first = rng.pick(&options[0]);
                (first.0, vec![first])
            };
            if self.floorless(start_src) {
                continue;
            }
            let Some((d0, start, sw)) = farthest_end(start_src, 0, &[]) else { continue };
            let mut total = d0;
            let mut entry = if self.n_zones == 1 { start } else { cuts[0].1 };
            for z in 1..last {
                let dist = self.distances(entry);
                let next = options[z].iter().copied().filter(|o| o.0 != entry).max_by_key(|o| (dist[o.0], o.0));
                let Some(next) = next else { break };
                total += dist[next.0];
                cuts.push(next);
                entry = next.1;
            }
            if cuts.len() != last {
                continue;
            }
            let taken: Vec<usize> = cuts.iter().flat_map(|&(a, b, _)| [a, b]).chain([start]).collect();
            let Some((dl, goal, gw)) = farthest_end(entry, last, &taken) else { continue };
            total += dl;
            if best.as_ref().is_none_or(|b| total > b.0) {
                best = Some((total, cuts, (start, sw), (goal, gw)));
            }
        }
        let Some((_, cuts, (start, sw), (goal, gw))) = best else { return false };
        for &(a, b, d) in &cuts {
            self.knock(a, d);
            self.cuts.push((a.min(b), a.max(b)));
        }
        self.shut(start, sw);
        if self.degree(goal) != 2 || !self.is_open(goal, gw) {
            return false; // the start's wall was one of the goal's: try another layout
        }
        self.shut(goal, gw);
        self.start = start;
        self.goal = goal;
        true
    }

    // -- dead ends --------------------------------------------------------
    /// Steps around the loop that opening a-b would make (at most MIN_LOOP).
    fn loop_length(&self, a: usize, b: usize) -> usize {
        let mut dist = HashMap::from([(a, 0usize)]);
        let mut q = VecDeque::from([a]);
        while let Some(c) = q.pop_front() {
            let dc = dist[&c];
            if dc + 1 >= MIN_LOOP {
                break;
            }
            for m in self.links(c) {
                if let std::collections::hash_map::Entry::Vacant(e) = dist.entry(m) {
                    e.insert(dc + 1);
                    if m == b {
                        return dc + 2;
                    }
                    q.push_back(m);
                }
            }
        }
        MIN_LOOP
    }

    /// Close every dead end but the start and finish into a loop within its
    /// zone: open one more of its walls (the zone promise means one is always
    /// free), or hop on a bridge over a neighbouring passage to its zone
    /// beyond. Strands are thin, so the wall next door often only makes a
    /// tiny loop around a lone stub of wall; then a hop that makes a real
    /// loop is the better way out. Prefer joining other dead ends, so one
    /// opening fixes two.
    fn braid(&mut self, rng: &mut Rng) {
        let n = self.zone.len();
        let mut todo: Vec<usize> = (0..n)
            .filter(|&c| !self.floorless(c) && self.degree(c) == 1)
            .collect();
        rng.shuffle(&mut todo);
        for x in todo {
            if self.degree(x) != 1 {
                continue;
            }
            let (_, act) = self.best_close(x, rng).expect("zone promise broken: a dead end has no wall to open");
            self.close(x, act);
        }
    }

    /// The best new way out of x that loops back within its zone: the
    /// biggest loop (counted up to MIN_LOOP), a wall before a hop, joining a
    /// dead end before not. Returns the loop size with it.
    fn best_close(&self, x: usize, rng: &mut Rng) -> Option<(usize, Close)> {
        let mut best: Option<((usize, bool, bool, usize), usize, Close)> = None;
        let mut consider = |len: usize, q: usize, wall: bool, act: Close, tie: usize| {
            let key = (len, wall, self.degree(q) == 1, tie);
            if best.as_ref().is_none_or(|b| key > b.0) {
                best = Some((key, len, act));
            }
        };
        for d in 0..4 {
            if self.is_open(x, d) {
                continue;
            }
            let Some(q) = self.can_open(x, d, true) else { continue };
            let len = self.loop_length(x, q).min(MIN_LOOP);
            consider(len, q, true, Close::Wall(d), rng.below(1000));
        }
        for (_, y) in self.hop_options(x, |y| !self.partners[x].contains(&y)) {
            let len = self.loop_length(x, y).min(MIN_LOOP);
            consider(len, y, false, Close::Hop(y), rng.below(1000));
        }
        best.map(|(_, len, act)| (len, act))
    }

    fn close(&mut self, x: usize, act: Close) {
        match act {
            Close::Wall(d) => self.knock(x, d),
            Close::Hop(y) => self.hop(x, y),
        }
    }

    fn shut(&mut self, x: usize, d: usize) {
        let y = self.step(x, d).unwrap();
        self.open[x] &= !(1 << d);
        self.open[y] &= !(1 << opp(d));
    }

    /// A loop of four cells leaves a lone pillar of wall in a little open
    /// room. Put back one of its four walls. Taking a step out of a loop
    /// can't cut anything off; if both its cells still have two ways out,
    /// that's all. If one would be left a dead end, it gets a new way out
    /// that makes a bigger loop instead (at least MIN_LOOP around if it can,
    /// else six), or the wall stays open.
    fn break_pillars(&mut self, rng: &mut Rng) {
        let cols = self.cols;
        let mut squares: Vec<usize> = (0..self.zone.len())
            .filter(|&c| c % cols + 1 < cols && c / cols + 1 < self.rows)
            .collect();
        rng.shuffle(&mut squares);
        for c in squares {
            // The square's four walls, as (cell, direction).
            let mut sides = [(c, 3), (c, 1), (c + 1, 1), (c + cols, 3)];
            if !sides.iter().all(|&(x, d)| self.is_open(x, d)) {
                continue;
            }
            rng.shuffle(&mut sides);
            // Walls whose cells both keep two ways out first.
            sides.sort_by_key(|&(x, d)| {
                let y = self.step(x, d).unwrap();
                (self.degree(x) < 3) as u8 + (self.degree(y) < 3) as u8
            });
            for (x, d) in sides {
                let y = self.step(x, d).unwrap();
                let stranded: Vec<usize> = [x, y].into_iter().filter(|&v| self.degree(v) < 3).collect();
                if stranded.len() == 2 {
                    break; // sorted: no wall left with a cell to spare
                }
                self.shut(x, d);
                let Some(&v) = stranded.first() else { break };
                match self.best_close(v, rng) {
                    Some((len, act)) if len >= 6 => {
                        self.close(v, act);
                        break;
                    }
                    _ => self.knock(x, d),
                }
            }
        }
    }

    /// Choose which bridge is on top at each crossing, flipping between
    /// neighbouring crossings along every bridge like a basket weave.
    fn weave(&mut self, rng: &mut Rng) {
        let mut over: HashMap<usize, Vec<usize>> = HashMap::new();
        for (i, b) in self.bridges.iter().enumerate() {
            for &m in &b.mids {
                if self.under[m] == 3 {
                    over.entry(m).or_default().push(i);
                }
            }
        }
        let mut nexts: HashMap<usize, Vec<usize>> = over.keys().map(|&m| (m, Vec::new())).collect();
        for b in &self.bridges {
            let row: Vec<usize> = b.mids.iter().copied().filter(|m| over.contains_key(m)).collect();
            for w in row.windows(2) {
                nexts.get_mut(&w[0]).unwrap().push(w[1]);
                nexts.get_mut(&w[1]).unwrap().push(w[0]);
            }
        }
        let mut h_on_top: HashMap<usize, bool> = HashMap::new();
        let mut cells: Vec<usize> = over.keys().copied().collect();
        cells.sort();
        for seed in cells {
            if h_on_top.contains_key(&seed) {
                continue;
            }
            h_on_top.insert(seed, rng.chance(0.5));
            let mut q = VecDeque::from([seed]);
            while let Some(c) = q.pop_front() {
                let flip = !h_on_top[&c];
                for &m in &nexts[&c] {
                    if !h_on_top.contains_key(&m) {
                        h_on_top.insert(m, flip);
                        q.push_back(m);
                    }
                }
            }
        }
        for (m, pair) in over {
            let want = if h_on_top[&m] { Orient::H } else { Orient::V };
            let top = pair.into_iter().find(|&i| self.bridges[i].o == want).unwrap();
            self.on_top.insert(m, top);
        }
    }

    // -- solving ----------------------------------------------------------
    pub fn solve(&self) -> Option<Vec<usize>> {
        let mut prev = vec![usize::MAX; self.zone.len()];
        prev[self.start] = self.start;
        let mut q = VecDeque::from([self.start]);
        while let Some(c) = q.pop_front() {
            if c == self.goal {
                break;
            }
            for m in self.links(c) {
                if prev[m] == usize::MAX {
                    prev[m] = c;
                    q.push_back(m);
                }
            }
        }
        if prev[self.goal] == usize::MAX {
            return None;
        }
        let mut path = vec![self.goal];
        let mut c = self.goal;
        while c != self.start {
            c = prev[c];
            path.push(c);
        }
        path.reverse();
        Some(path)
    }

    /// Cells where the path splits three or four ways.
    pub fn junctions(&self) -> usize {
        (0..self.zone.len()).filter(|&c| self.degree(c) >= 3).count()
    }
}

pub fn walk_length(path: &[usize], cols: usize) -> usize {
    path.windows(2)
        .map(|w| (w[0] / cols).abs_diff(w[1] / cols) + (w[0] % cols).abs_diff(w[1] % cols))
        .sum()
}

/// Extend `path` to visit every zone, each touching the next.
fn chain(touch: &[Vec<bool>], path: &mut Vec<usize>, budget: &mut usize, rng: &mut Rng) -> bool {
    if path.len() == touch.len() {
        return true;
    }
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let last = *path.last().unwrap();
    let mut next: Vec<usize> = (0..touch.len()).filter(|&z| touch[last][z] && !path.contains(&z)).collect();
    rng.shuffle(&mut next);
    for z in next {
        path.push(z);
        if chain(touch, path, budget, rng) {
            return true;
        }
        path.pop();
    }
    false
}

/// Zones for a maze of this size when none are asked for: more strands to
/// tangle on a bigger map.
pub fn default_zones(rows: usize, cols: usize, rng: &mut Rng) -> usize {
    let area = rows * cols;
    let most = max_zones(rows, cols);
    rng.range((2 + area / 1000).min(most), (3 + area / 500).min(most))
}

/// The most zones a maze this size has room for: two blocks each.
pub fn max_zones(rows: usize, cols: usize) -> usize {
    let blocks = (rows / ZONE_BLOCK).max(1) * (cols / ZONE_BLOCK).max(1);
    (blocks / 2).clamp(1, 64)
}
