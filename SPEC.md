# amaze spec

A book of printable mazes where paths string all over each other, passing over
and under, so the path right next to you is a long way to follow.

The rules and goals here are the only things that are rigid about the maze.
Nothing else about its shape is decided by the program on its own.

## Words

- **Cell**: one square of the grid.
- **Path**: what you follow. It floats in space; anything that isn't path is
  empty.
- **Pass**: where one path goes over or under another. It doesn't have to make
  sense in 3D: each cell only records which path is on top.
- **Zone**: one part of the maze, strung all through the others. Zones form a
  chain, each meeting the next at a single choke point.
- **Step**: the builder adds a detour loop: a new stretch of path that leaves
  the maze at one point and rejoins it at another.

## Building

The maze starts as one loop. Each step adds a detour loop, chosen from the
steps the rules allow, leaning toward the goals.

Every step is final. Nothing is backed up, repaired or relaxed afterwards. If
no step is allowed, the builder stops: "I'm dead - I give up."

## Rules

These hold after every step.

1. Every cell holds 0, 1 or 2 paths.
2. Everything built connects to the start.
3. Zones form a chain: each connects only to the one before and the one
   after, at one choke point each.
4. The start is in the first zone and the finish is in the last.
5. No dead ends. Because every step is a loop, there are never any loose
   ends.

## Goals

Most important first.

1. **Close on the page, far to follow.** Paths next to or over each other
   should be a long way apart to follow. This covers zones winding through
   each other, long loops, and no open areas.
2. **Coverage.** The solution passes through or near most of the maze. Where
   the finish ends up follows from this.
3. **Lots of passes, and long ones.**
4. **Choke points spread apart.**
5. **A few zones**, roughly. Close is good enough.
6. **Little empty space.**
7. **Start and finish close together on the page.** A light tiebreaker.

Forks aren't a goal: every detour loop adds two.

The maze should be confusing and fun, not a slog. Length for its own sake is
not a goal.

## Look

- Paths are white ribbons with black edges. There are no walls.
- At a crossing, the path underneath has a small gap on each side of the one
  on top.
- Two paths stacked running the same way: to be decided. The options are the
  hidden edges shown as short, sparse dashes, or the top path drawn narrower
  so the lower one shows on both sides.
- Start and finish are ordinary path cells, marked.
- No text on the page.

## Output

- A book of levels: Trivial, Easy, Standard and Hard, plus minis four to a
  page.
- Each puzzle's solution is printed mirrored on the back of its page, so it
  shows through when held up to the light. Print double sided.
- Simple tooling.

## Open

- How to draw two paths stacked the same way.
- How many zones counts as "a few", per level.
