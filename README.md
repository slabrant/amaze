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
- Shape: a letter or figure drawn as one unbroken line through cells. A shaped maze's solution is its shape.
- Step: adding a path whose two ends are joined to the maze. The cells its ends join become junctions, splitting the paths they were on. Its cells are empty cells or cells with one path in them; those become crossings.

## Building

Start with one loop. Then make steps, each picked from the steps the rules
allow, favouring the goals. Each step is in one zone. A new zone starts with a
step whose two ends join the same cell of the last zone. Steps are never undone
or changed. Steps are made until no step is allowed, when the area is nearly
filled. If a zone cannot be started, generation fails.

A shaped maze starts with a loop made of its shape and a route back from the
finish to the start, longer than the shape. Its steps are only those that leave
the solution as it is.

## Rules

1. A cell has at most two paths in it.
2. Every maze cell has a route to the start.
3. Each zone joins only the zone before it and the zone after it, at one junction each.
4. Start is in the first zone. Finish is in the last zone.
5. No dead ends.
6. Both paths go straight through a crossing.
7. Every step's circuit is at least 8 cells.

## Goals

1. For two paths in the same or neighbouring cells, the shortest route between them is long.
2. Most maze cells are a few cells from the solution, as the crow flies.
3. Many crossings, including one path having several crossings one after another.
4. The junctions where zones join are far apart.
5. A small number of zones.
6. Few empty cells.
7. Start and finish are near each other, as the crow flies.

## Look

- Paths are white with thin black edges, about three quarters of a cell wide. No walls.
- The space between paths is lightly hatched, and a black line with rounded corners runs round the outside.
- Turns are rounded, inside and outside.
- At a crossing, the top path is whole and the lower path's edges carry on across it as grey dashes.
- A crossing where both paths run the same way: undecided (dashed edges, or a narrower top path).
- Start: a red square with a white dot. It has no direction; the solver can set off either way.
- Finish: a purple square with a white ring and dot.
- No text.

## Output

- A book: Trivial, Easy, Standard and Hard one to a page, then mini Trivial, Easy, Standard and Hard four to a page.
- Each solution is printed mirrored on the back of its page.
- Given a shape (`amaze <maze.pdf> <seed> <shape>`), one Standard shaped maze instead. Shapes: C G J L M N S U V W Z 2 7 spiral.
- The back shows only the solution: a thick line whose colour runs from red at the start to purple at the finish, with a gap where it goes under another path. Being mirrored, it lines up when the page is held to the light.

## Exceptions

Places where generation treats something differently from the rest.

- A step that starts a zone is picked from more tries than other steps, and its search always runs the full length.
- The step that starts the last zone is pulled towards the start, so the finish can be near it.
- The finish is any cell of the last zone nearly as near the start as the nearest one; of those, the one with the longest solution.
- Start and finish cells are never made into crossings.
- The book retries a maze that fails with another seed, and every maze has at least two zones.
- Grids smaller than 4 by 4 are refused.
- A shaped maze has one zone, its start and finish are the ends of its shape, and the solution is always the top path at a crossing.
