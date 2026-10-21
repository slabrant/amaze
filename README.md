# amaze

Printable mazes with bridges that weave over and under each other.

- Paths are broad corridors separated by thin, lightly hatched walls.
- Bridges are straight runs of path between two landings that pass over or
  under what they cross. Over solid ground or a plain path, a bridge is on
  top; where bridges cross each other, each one alternates over and under
  along its length like a basket weave. A path that goes under stops at one
  side and comes out the other, with its hidden edges shown as faint dashes.
- Start at the arrow, finish at the bullseye. Those are the only dead ends.
- Behind the scenes the maze is split into zones: strands that wind through
  each other like spaghetti, crossing over and under one another on bridges
  without ever connecting. Each zone has exactly one way across to the next,
  so loops never shortcut the route.

## How it's built

Generation never repairs or relaxes anything after the fact:

1. **Strands.** Zones grow as strands three cells thick from seeds spread
   across the map, snaking from their newest end. A strand blocked by another
   can cross over it on a bridge, or slip under it while the other strand
   bridges over: that over-and-under is what winds strands round each other.
   Every growth step must keep each zone it touches in one piece, with at
   least two walls each cell may open and no 2x2 square that could only ever
   loop round a lone pillar of wall. Steps that would break that are not
   taken.
2. **Carving.** Each zone is carved with a growing tree that branches often
   and prefers turns to straight runs. A corridor boxed in by others hops
   over a passage on a new bridge if its zone carries on beyond.
3. **Route.** The one way across between each zone and the next, and the
   start and finish, are chosen so each zone's stretch of the route is long.
4. **Loops.** Every other dead end opens one more wall (or hops a bridge) back
   into its own zone. The promise from step 1 means one is always free. Any
   loop of four cells then gets a wall back.

Bridges are never counted toward a target: a bridge is there because a strand
had to cross another or a corridor had to hop over one.

## Building

Needs Rust (`cargo`).

```
cargo build --release
```

## Usage

```
./target/release/amaze                                          # one maze -> maze.pdf
./target/release/amaze --count 3 --solution -o mazes.pdf        # 3 mazes, solutions at the back
./target/release/amaze --count 8 --per-page 4 --solution -o small.pdf   # 4 small mazes per page
./target/release/amaze --level hard --seed CYVF                  # recreate a maze from its code
```

Each maze shows a four-character code (no look-alike letters: no I, L, O, 0
or 1). Give it back with `--seed`, and the same size or `--level`, to make
that maze again. Levels are `easy`, `medium`, `hard`, and `mini-trivial`,
`mini-easy`, `mini-medium`, `mini-hard`: the sizes used in the book.

`amaze book -o book.pdf` makes a whole book for double-sided printing: three
each of Easy, Medium and Hard, then eight minis four to a page (two each of
Trivial, Easy, Medium and Hard, a level to a row), every page with its solutions on the back.
Each level's route runs roughly two to three times the last (minis about 1.6
times). Every maze shows its code; the book's own code is printed in the
terminal, and `--seed` with it makes the same book again.

`--sign NAME` (on either command) prints NAME once on each front page, in the
caption line under the maze (the bottom-left one, four to a page).

`--back-solution` puts each page's solutions on the back of its sheet: just
the solution line, mirrored, so it shows through when the maze is held up to
the light. Print double sided, flipping on the long edge.

Other options: `--rows`, `--cols`, `--zones`, `--sign`, `--show-zones` (tint each
zone's paths on solution pages, to see the strands), `--graph-paper`. Run
`amaze --help` for details.

## Checking the rules

```
./target/release/amaze check          # build 12 mazes of mixed sizes and check them
./target/release/amaze check 0 300    # seeds 0 to 299
```

It rebuilds everything from the finished walls and bridges, independently of
the generator, and fails if any rule is broken: no dead ends besides start and
finish, every cell reachable, at most one passage at a time under a bridge, no
lone pillars of wall, and zones joined only at their single crossings. Run it
after changing the generator.
