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
//! The second promise is what lets every corridor carry on or join up
//! again: a cell entered by one opening always has another.
//!
//! Zones are carved in ears, corridors that set out from what is carved and
//! join back up, so no dead end is ever made. An ear joins up where its loop
//! winds round or across another zone; see `carve`.
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
const ZONE_SNAKE: f64 = 0.2; // how often a zone grows from its newest block
const CROSS_WEIGHT: f64 = 20.0; // pull toward crossing over another strand (a step: 1)
const UNDER_WEIGHT: f64 = 20.0; // pull toward slipping under another strand
const CROSS_BLOCKS: usize = 3; // most blocks one strand crossing may span
const RING_WEIGHT: f64 = 40.0; // pull toward closing a ring round another strand
const RING_BLOCKS: usize = 4; // fewest blocks round a ring
const ZONE_WIGGLE: usize = 3; // edge-wiggling passes (times the cell count)
const ZONE_FLOOR: f64 = 0.8; // wiggling never shrinks a zone below this share

// Carving.
const STRAIGHT_WEIGHT: f64 = 0.3; // carrying straight on, against turning
const POCKET_PULL: f64 = 1.0; // pull toward cells with fewer ways on
const HOP_MIDS: usize = 3; // most cells a carving hop may pass over
const EAR_TRIES: usize = 30; // walks at an ear before the layout is redrawn
const EAR_BACKTRACKS: usize = 500; // steps an ear may back up before it is walked again
const LOOP_MIN: usize = 12; // fewest steps round a loop that winds round nothing
const SEED_TRIES: usize = 10; // cells a zone may begin from before the layout is redrawn

// Cells while carving.
const UNCARVED: u8 = 0;
const IN_EAR: u8 = 1;
const CARVED: u8 = 2;
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

/// How an ear's newest cell joins the carved ground to close its loop.
#[derive(Clone, Copy)]
enum Join {
    Wall(usize),
    Hop(usize),
    /// A bridge already landing there.
    Bridge,
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
            if !m.carve(rng) || !m.place_route(rng) {
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
    /// so it stays a thin strand, now and then from its newest block so it
    /// snakes along, more often from anywhere along it so it branches. Where
    /// another strand is in the way it would much rather cross over it on a
    /// bridge, or slip under it while the other strand bridges over, than
    /// step aside: strands coil over and under each other like spaghetti. A
    /// strand that comes back beside itself round another closes a ring.
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
                    out.push((Grow::Step(t), 1.0));
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
    /// Something always goes under: a passage, or another bridge. A bridge
    /// over nothing but solid ground would just be a straight corridor.
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
                (0..1u32 << mids.len()).filter(|p| p & (p >> 1) == 0 && p & forced == 0 && (p | forced) != 0).collect();
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
    /// Carve each zone in ears. An ear sets out from the carved part of its
    /// zone through uncarved ground and keeps going until it can join the
    /// carved part again, or itself, in a loop that winds round or across
    /// another zone. Boxed in, it may instead come back as a detour, or close
    /// a long loop; otherwise it backs up and tries another way, and in the
    /// end gives up. The whole space need not be filled: ground no ear could
    /// take in stays solid, rather than being forced into little loops. So no
    /// corridor is ever left as a dead end, and nothing is closed up after
    /// the fact. Zones take turns an ear at a time, so an ear can hop over
    /// another zone's finished passages. Returns false if some zone got no
    /// ear at all.
    fn carve(&mut self, rng: &mut Rng) -> bool {
        let n = self.zone.len();
        let mut state: Vec<u8> = (0..n).map(|c| if self.floorless(c) { CARVED } else { UNCARVED }).collect();
        let mut entry = vec![usize::MAX; n]; // direction each cell was entered by
        // Carved cells of each zone that may still set out on an ear, and
        // the cells each zone has tried to begin from.
        let mut from: Vec<Vec<usize>> = vec![Vec::new(); self.n_zones];
        let mut seeds: Vec<Vec<usize>> = vec![Vec::new(); self.n_zones];
        loop {
            // A zone with nothing carved yet begins again somewhere new.
            for z in 0..self.n_zones {
                if from[z].is_empty() && !(0..n).any(|c| self.zone[c] == z as u8 && state[c] == CARVED && !self.floorless(c)) {
                    let cells: Vec<usize> = (0..n)
                        .filter(|&c| self.zone[c] == z as u8 && state[c] == UNCARVED && self.plain(c) && !seeds[z].contains(&c))
                        .collect();
                    if cells.is_empty() || seeds[z].len() >= SEED_TRIES {
                        return false;
                    }
                    let c = rng.pick(&cells);
                    seeds[z].push(c);
                    state[c] = CARVED;
                    from[z].push(c);
                }
            }
            for list in from.iter_mut() {
                list.retain(|&c| !self.ways_on(c, &state).is_empty());
            }
            let live: Vec<usize> = (0..self.n_zones).filter(|&z| !from[z].is_empty()).collect();
            if live.is_empty() {
                break;
            }
            let z = rng.pick(&live);
            let u = rng.pick(&from[z]);
            let mut cells = None;
            for _ in 0..EAR_TRIES {
                let saved = (self.clone(), state.clone(), entry.clone());
                cells = self.ear(z as u8, u, &mut state, &mut entry, rng);
                if cells.is_some() {
                    break;
                }
                (*self, state, entry) = saved;
            }
            match cells {
                Some(cells) => {
                    from[z].retain(|&c| state[c] == CARVED);
                    from[z].extend(cells);
                }
                None => {
                    // Nothing worth carving from here.
                    from[z].retain(|&c| c != u);
                    if self.degree(u) == 0 {
                        state[u] = UNCARVED;
                    }
                }
            }
        }
        if (0..self.n_zones).any(|z| !(0..n).any(|c| self.zone[c] == z as u8 && state[c] == CARVED && self.degree(c) > 0)) {
            return false;
        }
        self.leave_ground(&state);
        true
    }

    /// Ground no ear took in stays solid, and a bridge that lands on it
    /// goes, with whatever ran under it left as it was. A bridge left with nothing under it is just a straight corridor,
    /// so it becomes one: the ground it spanned becomes path of its zone.
    fn leave_ground(&mut self, state: &[u8]) {
        let left: Vec<usize> = (0..self.zone.len()).filter(|&c| state[c] == UNCARVED).collect();
        for &c in &left {
            self.solid[c] = true;
        }
        let gone: Vec<usize> =
            (0..self.bridges.len()).filter(|&i| self.solid[self.bridges[i].a] || self.solid[self.bridges[i].b]).collect();
        for &i in gone.iter().rev() {
            self.drop_bridge(i);
        }
        let bare: Vec<usize> = (0..self.bridges.len())
            .filter(|&i| self.bridges[i].mids.iter().all(|&m| self.solid[m] && self.under[m] != 3))
            .collect();
        for &i in bare.iter().rev() {
            let b = self.drop_bridge(i);
            let d = self.dir_to(b.a, b.b);
            let z = self.zone[b.a];
            for &m in &b.mids {
                self.solid[m] = false;
                self.zone[m] = z;
            }
            let mut x = b.a;
            while x != b.b {
                self.knock(x, d);
                x = self.step(x, d).unwrap();
            }
        }
    }

    /// Remove bridge i, and its marks on the cells it spans.
    fn drop_bridge(&mut self, i: usize) -> Bridge {
        let b = self.bridges.remove(i);
        self.partners[b.a].retain(|&x| x != b.b);
        self.partners[b.b].retain(|&x| x != b.a);
        for &m in &b.mids {
            self.under[m] &= !b.o.bit();
        }
        b
    }

    /// Ways on from c into uncarved ground of its zone: through a wall, or
    /// else hopping over a finished passage. As (direction, cell, hop).
    fn ways_on(&self, c: usize, state: &[u8]) -> Vec<(usize, usize, bool)> {
        self.ways_on_by(c, state, true)
    }

    fn ways_on_by(&self, c: usize, state: &[u8], hops: bool) -> Vec<(usize, usize, bool)> {
        let walls: Vec<(usize, usize, bool)> = (0..4)
            .filter_map(|d| self.can_open(c, d, true).map(|m| (d, m, false)))
            .filter(|&(_, m, _)| state[m] == UNCARVED)
            .collect();
        if !walls.is_empty() || !hops {
            return walls;
        }
        self.hop_options(c, |y| state[y] == UNCARVED && self.partners[y].is_empty())
            .into_iter()
            .map(|(d, y)| (d, y, true))
            .collect()
    }

    /// Walk one ear of zone z from the carved cell u. Where it is boxed in
    /// with no loop it may close, it backs up and tries another way, up to
    /// EAR_BACKTRACKS times. Returns its new cells, or None if it found no
    /// loop to close.
    fn ear(&mut self, z: u8, u: usize, state: &mut [u8], entry: &mut [usize], rng: &mut Rng) -> Option<Vec<usize>> {
        let n = self.zone.len();
        // The zone's first ear: it has nothing carved to come back to but u.
        let bare = self.degree(u) == 0;
        // The shortest ways back to u through the carved part of the zone,
        // which stays as it is while the ear grows.
        let mut back = vec![usize::MAX; n];
        back[u] = u;
        let mut q = VecDeque::from([u]);
        while let Some(x) = q.pop_front() {
            for y in self.links(x).collect::<Vec<_>>() {
                if back[y] == usize::MAX && state[y] == CARVED {
                    back[y] = x;
                    q.push_back(y);
                }
            }
        }
        let mut ear = vec![u];
        // For each step taken, where from, which way, whether it hopped, and
        // how many cells it added; for each place along the ear, the cells
        // already tried from there.
        let mut steps: Vec<(usize, usize, bool, usize)> = Vec::new();
        let mut tried: Vec<Vec<usize>> = vec![Vec::new()];
        let mut backtracks = 0;
        loop {
            let cur = *ear.last().unwrap();
            let prev = if ear.len() > 1 { ear[ear.len() - 2] } else { usize::MAX };
            // The loops it could close from here, by a wall or a hop.
            let mut closes: Vec<(Vec<usize>, Join)> = Vec::new();
            if ear.len() > 1 {
                for d in 0..4 {
                    if self.is_open(cur, d) {
                        continue;
                    }
                    let Some(v) = self.can_open(cur, d, true) else { continue };
                    if v != prev && state[v] != UNCARVED {
                        if let Some(ring) = self.ring(&ear, v, &back, state) {
                            closes.push((ring, Join::Wall(d)));
                        }
                    }
                }
                let hops = self.hop_options(cur, |y| y != prev && state[y] != UNCARVED && self.partners[y].is_empty());
                for (_, y) in hops {
                    if let Some(ring) = self.ring(&ear, y, &back, state) {
                        closes.push((ring, Join::Hop(y)));
                    }
                }
            }
            // A first ear doesn't hop: its lead-in may be taken back.
            let here = steps.len();
            let ways: Vec<(usize, usize, bool)> =
                self.ways_on_by(cur, state, !bare).into_iter().filter(|w| !tried[here].contains(&w.1)).collect();
            // Close a loop round or across another zone as soon as there is
            // one. Boxed in, rather than close a loop round nothing, come back
            // beside the start where a wall can go back up between them: the
            // ear becomes a detour on the corridor it set out from, and makes
            // no loop at all. Failing that, the longest loop that isn't round
            // a lone pillar.
            let wrapping = closes.iter().filter(|c| self.ring_wraps(z, &c.0)).max_by_key(|c| c.0.len());
            if let Some((ring, join)) = wrapping {
                let (ring, join) = (ring.clone(), *join);
                return Some(self.close_ear(&ear, &ring, join, bare, state));
            }
            if ways.is_empty() {
                let detour = closes.iter().find(|c| {
                    let v = *c.0.last().unwrap();
                    c.0.len() == ear.len() + 1 && self.adjacent(u, v) && self.is_open(u, self.dir_to(u, v))
                });
                if let Some((ring, join)) = detour {
                    let (v, join) = (*ring.last().unwrap(), *join);
                    let ring = ring.clone();
                    self.shut(u, self.dir_to(u, v));
                    return Some(self.close_ear(&ear, &ring, join, bare, state));
                }
                let longest = closes.iter().filter(|c| c.0.len() >= LOOP_MIN).max_by_key(|c| (c.0.len(), rng.below(1000)));
                if let Some((ring, join)) = longest {
                    let (ring, join) = (ring.clone(), *join);
                    return Some(self.close_ear(&ear, &ring, join, bare, state));
                }
            }
            if ways.is_empty() {
                // Back up a step and try another way from there.
                let (from, d, hop, added) = steps.pop()?;
                backtracks += 1;
                if backtracks > EAR_BACKTRACKS {
                    return None;
                }
                if hop {
                    self.unhop();
                } else {
                    self.shut(from, d);
                }
                for c in ear.drain(ear.len() - added..) {
                    state[c] = UNCARVED;
                }
                tried.pop();
                continue;
            }
            // Carry on into new ground: favour turns, and cells with fewer
            // ways on, so the ear hugs what is there and leaves no pockets.
            let w: Vec<f64> = ways
                .iter()
                .map(|&(d, m, _)| {
                    let free = self.ways_on(m, state).len();
                    let turn = if entry[cur] == d { STRAIGHT_WEIGHT } else { 1.0 };
                    turn / (1.0 + free as f64).powf(POCKET_PULL)
                })
                .collect();
            let (d, m, hop) = ways[rng.weighted(&w)];
            tried[here].push(m);
            steps.push((cur, d, hop, 1));
            tried.push(Vec::new());
            if hop {
                self.hop(cur, m);
            } else {
                self.knock(cur, d);
            }
            entry[m] = d;
            state[m] = IN_EAR;
            ear.push(m);
            // A bridge already landing here carries the ear across with it.
            let Some(&p) = self.partners[m].first() else { continue };
            if self.partners[m].len() != 1 || (hop && p == cur) {
                continue;
            }
            if state[p] == UNCARVED {
                entry[p] = self.dir_to(m, p);
                state[p] = IN_EAR;
                ear.push(p);
                steps.last_mut().unwrap().3 = 2;
            } else if let Some(ring) = self.ring(&ear, p, &back, state) {
                // Its far end is carved: the bridge closes the loop.
                return Some(self.close_ear(&ear, &ring, Join::Bridge, bare, state));
            }
        }
    }

    /// Close the ear's loop `ring` by `join`, and mark its cells carved.
    /// Returns the new ones. A first ear that came back to itself rather
    /// than its start drops its lead-in, so the zone begins as one loop.
    fn close_ear(&mut self, ear: &[usize], ring: &[usize], join: Join, bare: bool, state: &mut [u8]) -> Vec<usize> {
        let cur = *ear.last().unwrap();
        match join {
            Join::Wall(d) => self.knock(cur, d),
            Join::Hop(y) => self.hop(cur, y),
            Join::Bridge => {}
        }
        let mut ear = ear;
        if bare && ring[0] != ear[0] {
            let j = ear.iter().position(|&c| c == ring[0]).unwrap();
            for i in 0..j {
                self.shut(ear[i], self.dir_to(ear[i], ear[i + 1]));
                state[ear[i]] = UNCARVED;
            }
            ear = &ear[j - 1..];
        }
        for &c in &ear[1..] {
            state[c] = CARVED;
        }
        ear[1..].to_vec()
    }

    /// The loop that joining the ear's newest cell to v would close, in
    /// order round it: back along the ear to v if v is on it, or else the
    /// whole ear and the shortest carved way from v back to its start.
    fn ring(&self, ear: &[usize], v: usize, back: &[usize], state: &[u8]) -> Option<Vec<usize>> {
        if state[v] == IN_EAR {
            let j = ear.iter().position(|&c| c == v)?;
            return Some(ear[j..].to_vec());
        }
        if back[v] == usize::MAX {
            return None;
        }
        let mut ring = ear.to_vec();
        let mut x = v;
        while x != ear[0] {
            ring.push(x);
            x = back[x];
        }
        Some(ring)
    }

    fn adjacent(&self, a: usize, b: usize) -> bool {
        (a / self.cols).abs_diff(b / self.cols) + (a % self.cols).abs_diff(b % self.cols) == 1
    }

    /// Does the loop through these cells, in order, wind round or across a
    /// zone other than z? Across: it bridges over another zone's passage or
    /// bridge, or passes under another zone's bridge. Round: another zone's
    /// cell lies inside it.
    fn ring_wraps(&self, z: u8, ring: &[usize]) -> bool {
        let k = ring.len();
        for i in 0..k {
            let (a, b) = (ring[i], ring[(i + 1) % k]);
            let over = |m: usize| self.zone[m] != z && (!self.solid[m] || self.under[m] == 3);
            if !self.adjacent(a, b) && self.cells_between(a.min(b), a.max(b)).iter().any(|&m| over(m)) {
                return true;
            }
        }
        for &c in ring {
            if self.under[c] != 0 && self.bridges.iter().any(|b| self.zone[b.a] != z && b.mids.contains(&c)) {
                return true;
            }
        }
        // Another zone's cell inside: on each row, the loop's upright sides
        // crossing just below the cells' middles, and the cells between.
        let cols = self.cols;
        let (mut r0, mut r1) = (self.rows, 0);
        for &c in ring {
            r0 = r0.min(c / cols);
            r1 = r1.max(c / cols);
        }
        let mut sides: Vec<Vec<usize>> = vec![Vec::new(); r1 + 1];
        for i in 0..k {
            let (a, b) = (ring[i], ring[(i + 1) % k]);
            if a % cols == b % cols {
                for row in sides.iter_mut().take((a / cols).max(b / cols)).skip((a / cols).min(b / cols)) {
                    row.push(a % cols);
                }
            }
        }
        for (r, xs) in sides.iter_mut().enumerate().skip(r0) {
            xs.sort_unstable();
            for pair in xs.chunks(2) {
                if let [lo, hi] = *pair {
                    if (lo..hi).any(|c| self.zone[r * cols + c] != z && !self.solid[r * cols + c]) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Bridge from a to b over cells whose floors are already settled.
    fn hop(&mut self, a: usize, b: usize) {
        let (lo, hi) = (a.min(b), a.max(b));
        let solid: Vec<bool> = self.cells_between(lo, hi).iter().map(|&m| self.solid[m]).collect();
        self.add_bridge(lo, hi, &solid);
        self.stats.hops += 1;
    }

    /// Take back the last bridge laid, a hop: nothing else has used it.
    fn unhop(&mut self) {
        let b = self.bridges.pop().unwrap();
        self.partners[b.a].pop();
        self.partners[b.b].pop();
        for &m in &b.mids {
            self.under[m] &= !b.o.bit();
        }
        self.stats.hops -= 1;
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
        let Some((_, cuts, (start, sw), (goal, _))) = best else { return false };
        for &(a, b, d) in &cuts {
            self.knock(a, d);
            self.cuts.push((a.min(b), a.max(b)));
        }
        self.shut(start, sw);
        // The goal's wall is chosen again now the start's is back up: with
        // both on one loop, the planned one could cut the maze in two.
        let entry = if self.n_zones == 1 { start } else { self.cuts.last().map(|&(a, b)| if self.zone[a] as usize == last { a } else { b }).unwrap() };
        let taken: Vec<usize> = self.cuts.iter().flat_map(|&(a, b)| [a, b]).chain([start]).collect();
        let Some(gw) = self.end_wall(goal, last as u8, &self.distances(entry), &taken) else {
            return false; // no wall left to put back there: try another layout
        };
        self.shut(goal, gw);
        self.start = start;
        self.goal = goal;
        true
    }

    fn shut(&mut self, x: usize, d: usize) {
        let y = self.step(x, d).unwrap();
        self.open[x] &= !(1 << d);
        self.open[y] &= !(1 << opp(d));
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
