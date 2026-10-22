# amaze

Printable mazes where paths go over and under each other.

## Words

- Cell: one square of the grid.
- Neighbour: a cell that shares a side with another cell.
- Joined: two neighbouring cells are joined if a path goes from one to the other.
- Path: a string of cells, each joined to the next. It can turn but does not fork.
- Maze: all the paths. A cell with no path in it is empty.
- Loop: a path whose last cell is joined to its first.
- Crossing: a cell with two paths in it. The cell records which path is on top.
- Route: the cells gone through, from joined cell to joined cell, to get from one cell to another.
- Start, finish: the cells where the solver begins and ends.
- Solution: the route from start to finish.
- Zone: a part of the maze spread among the other zones, not kept in one area. Zones are in order, first to last. Two zones join where a cell of one is joined to a cell of the other.
- Step: adding a path whose two ends are joined to the maze. Its cells are empty cells or cells with one path in them; those become crossings.

## Building

Start with one loop. Then make steps, each picked from the steps the rules
allow, favouring the goals. Steps are never undone or changed. If no step is
allowed, generation fails.

## Rules

1. A cell has at most two paths in it.
2. Every maze cell has a route to the start.
3. Each zone joins only the zone before it and the zone after it, in one place each.
4. Start is in the first zone. Finish is in the last zone.
5. No dead ends.

## Goals

1. For two paths in the same or neighbouring cells, the shortest route between them is long.
2. Most maze cells are a few cells from the solution, as the crow flies.
3. Many crossings, including one path having several crossings one after another.
4. The places where zones join are far apart.
5. A small number of zones.
6. Few empty cells.
7. Start and finish are near each other, as the crow flies.

## Look

- Paths are white with black edges. No walls.
- At a crossing, the lower path has a gap on each side of the top path.
- A crossing where both paths run the same way: undecided (dashed edges, or a narrower top path).
- No text.

## Output

- A book: Trivial, Easy, Standard, Hard, and small mazes four to a page.
- Each solution is printed mirrored on the back of its page.
