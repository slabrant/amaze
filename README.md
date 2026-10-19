# amaze

Printable mazes with bridges that weave over and under each other.

- Paths are broad corridors separated by thin, lightly hatched walls.
- Bridges are straight stretches of path raised over solid ground or over a
  path crossing underneath. A path that goes under a bridge stops at one side
  and comes out the other, with its hidden edges shown as faint dashes. Where
  bridges cross, they alternate over and under like a basket weave.
- Start at the arrow, finish at the bullseye. Those are the only dead ends.
- Behind the scenes the maze is split into 2–4 zones that wind through each
  other, bridging over and under one another without connecting. Each zone
  has exactly one way across to the next, so loops never shortcut the route.

## Requirements

Python 3 and [pycairo](https://pycairo.readthedocs.io/).

## Usage

```
python3 amaze.py                                          # one maze -> maze.pdf
python3 amaze.py --count 3 --solution -o mazes.pdf        # 3 mazes, solutions at the back
python3 amaze.py --count 8 --per-page 4 --solution -o small.pdf   # 4 small mazes per page
python3 amaze.py --seed 689202                            # recreate a specific maze
```

Each page shows its maze's seed, so any maze can be made again.

Other options: `--rows`, `--cols`, `--zones`, `--bridges`, `--bridge-span`,
`--show-zones` (tint zones on solution pages), `--graph-paper`. Run
`python3 amaze.py --help` for details.

## Checking the rules

```
python3 check_rules.py          # build 12 mazes of mixed sizes and check them
python3 check_rules.py 0 50     # seeds 0 to 49
```

It rebuilds everything from the finished walls and bridges, independently of
the generator, and fails if any rule is broken: no dead ends besides start and
finish, every cell reachable, at most one passage at a time under a bridge,
and zones joined only at their single crossings. Run it after changing
`amaze.py`.
