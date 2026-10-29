# amaze

Printable mazes where paths go over and under each other.

## Words

- Cell: one square of the grid.
- Neighbour: a cell that shares a side with another cell.
- Joined: two neighbouring cells are joined if a path goes from one to the other.
- Junction: a cell joined to three or four neighbours. A junction is not part of any path.
- Path: a string of cells between junctions, each joined to the next.
- Maze: all the paths and junctions. A cell with no path or junction in it is empty.
- Loop: a path whose last cell is joined to its first.
- Crossing: a cell with two paths in it. The cell records which path is on top.
- Route: the cells gone through, from joined cell to joined cell, to get from one cell to another.
- Start, finish: the cells where the solver begins and ends.
- Solution: the route from start to finish.
- Zone: a part of the maze spread among the other zones, not kept in one area. Zones are in order, first to last. Two zones join at a junction of one that is joined to cells of the other.
- Circuit: a step's path together with the shortest route between its two ends, before the step.
- Step: adding a path whose two ends are joined to the maze. The cells its ends join become junctions, splitting the paths they were on. Its cells are empty cells or cells with one path in them; those become crossings.

## Building

Start with one loop: a small rectangle somewhere in the grid, grown by pushing
out two cells at a time, each time where the loop is least crowded, so it
reaches across the grid. Then make steps, each picked from the steps the rules
allow, favouring the goals. Each step is in one zone. A new zone starts with a
step whose two ends join the same cell of the last zone. Steps are never undone
or changed. Steps are made until no step is allowed, when the area is nearly
filled. If a zone cannot be started, generation fails.

## Rules

1. A cell has at most two paths in it.
2. Every maze cell has a route to the start.
3. Each zone joins only the zone before it and the zone after it, at one junction each.
4. Start is in the first zone. Finish is in the last zone.
5. No dead ends.
6. Both paths go straight through a crossing.
7. Every step's circuit is at least 8 cells.

## Goals

1. Long way round: for two paths in the same or neighbouring cells, the shortest route between them is long.
2. Solution coverage: most maze cells are a few cells from the solution, as the crow flies.
3. Crossings, and crossings in a row: many crossings, including one path having several crossings one after another.
4. Joins apart: the junctions where zones join are far apart, both as the crow flies and by route.
5. A small number of zones.
6. Filling: few empty cells.
7. Finish near start: start and finish are near each other, as the crow flies.
8. Zones mixed: each zone is spread among the other zones' paths.
9. Zone balance: zones are about the same size.
10. Solution length: the solution is long enough (a fifth of the grid's cells), but no longer is better; long solutions are tiresome.

For a step that starts a zone, joins apart also counts how far its path reaches
from the joins already made, since the next zone's join will be on that path.
Steps that start a zone are also scored on next zone room: empty cells beside
the new path, so the next zone has somewhere to start from.

## Measures

A finished maze is measured against the goals. For every measure, bigger is better,
and every measure is a share or a rate, so mazes of different sizes compare.

Distances the goals use (near the solution, a long way round, how far zones mix,
how near the finish must be) are shares of the grid's side, the square root of its
cell count, so difficulty scales with size. Near the solution is never less than
2 cells, and a long way round never shorter than the shortest circuit the rules allow.

- Long way round: share of neighbouring paths whose shortest route between them is long.
- Solution coverage: share of maze cells a few cells from the solution.
- Crossings, crossings in a row: per cell of the grid.
- Joins apart: crow-flies distance between the two nearest zone joins, as a share of the grid's side; 1 when there is only one join.
- Filling: share of cells that are not empty.
- Finish near start: 1 minus the start-to-finish distance as a share of the grid's diagonal.
- Zones mixed: how much the least mixed zone is spread among the others.
- Solution length: the solution's cells as a share of long enough, at most 1.
- Away from border: share of the solution not next to the edge.
- Solver effort: how long simulated solvers take, per maze cell.

## Look

- Paths are white with thin black edges, about three quarters of a cell wide. No walls.
- The space between paths is plain white, and a black line with rounded corners runs round the outside.
- Light comes from the top left. Every path casts a soft shadow onto the background.
- A crossing's top path is a bridge: plain white and straight. The lower path is shaded where it dips under, on both sides, more on the side away from the light.
- Turns are rounded, inside and outside.
- At a crossing, the top path is whole and hides the lower path.
- A crossing where both paths run the same way: undecided (dashed edges, or a narrower top path).
- Start: a red square with a white dot. It has no direction; the solver can set off either way.
- Finish: a purple square with a white ring and dot.
- No text.

## Output

- A book: Trivial, Easy, Standard and Hard one to a page, then mini Trivial, Easy, Standard and Hard four to a page.
- Each solution is printed mirrored on the back of its page.
- With no seed given, a random one is used and printed.
- With `--two-up`, a book of two mazes to a page instead: four wide sizes, two mazes each, easiest first.
- The back shows only the solution: a thick line whose colour runs from red at the start to purple at the finish, with a gap where it goes under another path. Being mirrored, it lines up when the page is held to the light.

## Exceptions

Places where generation treats something differently from the rest.

- A step that starts a zone is picked from more tries than other steps. It is found by walking out a short way from a path of the previous zone, then taking the shortest way back to the other side of the cell it left.
- The step that starts the last zone is pulled towards the start, so the finish can be near it.
- The finish is any cell of the last zone nearly as near the start as the nearest one; of those, the one with the longest solution.
- Start and finish cells are never made into crossings.
- The book retries a maze that fails with another seed, and every maze has at least two zones.
- Grids smaller than 4 by 4 are refused.

## Step report

`cargo run --release --bin steps -- report.html [seeds] [first seed] [width height]`

Builds several mazes (8 by default) and writes a page showing every step of each,
side by side, with why each step was chosen. Before the steps, it shows the first
loop growing one bump at a time.

- Term: one part of a step's score. A step's score is the sum of its terms.
- Priority: how much say a term has. At each choice, every term is rescaled by how much it varies among the steps on offer, then multiplied by its priority, so a term's pull is its priority whatever its units. Priorities are in `src/score.rs`.
- Lift: how far the chosen step's term is above the average step it was picked from.
- Decides: how often leaving the term out would have changed which step was chosen.

Lifts are averaged per maze, then given a 95% interval across mazes. A term whose
interval crosses zero is not steering the choice.
