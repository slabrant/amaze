#!/usr/bin/env python3
"""Check generated mazes against amaze's rules.

Builds mazes across a range of seeds and sizes, then re-derives everything
from the finished walls and bridges alone, independently of how the
generator reasons about them, and reports any rule that is broken:

  * Under a single bridge, a cell is either solid (no openings) or carries
    one straight passage across the bridge. Never two passages side by side
    under the same bridge. A cell where two bridges cross has no floor.
  * Bridges never run along on top of each other, and never land under one.
  * No opening runs off the board or into a floorless cell.
  * The start and finish each have exactly one way out; no other cell is a
    dead end, and none is sealed off.
  * Every cell with a floor can be reached from the start.
  * The only links between zones are the designated crossings, and each
    crossing is the only way through.

Usage:  python3 check_rules.py [FIRST_SEED] [LAST_SEED]   (default 0 12)
Exits non-zero if any maze breaks a rule.
"""

import random
import sys
from collections import deque

import amaze

SIZES = [(45, 36), (22, 18), (30, 24)]  # (rows, cols): full page, 4 per page, other


def check(m):
    """Return a list of rule violations in maze m (empty if it is fine)."""
    R, C = m.rows, m.cols
    errs = []

    def ops(cell):
        """Directions a cell visibly opens: walls knocked down, bridges landing."""
        out = []
        for d in amaze.DIRS:
            n = amaze.add(cell, d)
            if m.inside(n) and m.edge(cell, n) in m.ground:
                out.append(d)
        for a, b, _, _ in m.bridges:
            if cell in (a, b):
                t = b if cell == a else a
                out.append(((t[0] > cell[0]) - (t[0] < cell[0]),
                            (t[1] > cell[1]) - (t[1] < cell[1])))
        return out

    under = {}
    for a, b, o, mids in m.bridges:
        for x in mids:
            if o in under.get(x, ()):
                errs.append(f"parallel bridges over {x}")
            under.setdefault(x, set()).add(o)
    ends = {x for a, b, _, _ in m.bridges for x in (a, b)}
    floorless = set()
    for x, dirs in under.items():
        if x in ends:
            errs.append(f"bridge landing {x} is under another bridge")
        if len(dirs) == 2 or not ops(x):
            if ops(x):
                errs.append(f"bridge crossing {x} has floor openings")
            floorless.add(x)
        else:
            o = next(iter(dirs))
            across = {(0, 1), (0, -1)} if o == amaze.V else {(1, 0), (-1, 0)}
            if set(ops(x)) != across:
                errs.append(f"cell under bridge {x} has openings {ops(x)}")
    for a, b, o, mids in m.bridges:
        for p, q in zip(mids, mids[1:]):
            if p not in floorless and q not in floorless:
                errs.append(f"two passages side by side under a bridge at {p}, {q}")
    for e in m.ground:
        for x in e:
            if not m.inside(x):
                errs.append(f"opening off the board at {x}")
            elif x in floorless:
                errs.append(f"opening into floorless cell {x}")
    for x in (m.start, m.goal):
        if len(ops(x)) != 1:
            errs.append(f"start/finish {x} should have one way out, has {ops(x)}")
    for r in range(R):
        for c in range(C):
            x = (r, c)
            if x in under or x in (m.start, m.goal):
                continue
            if not ops(x):
                errs.append(f"sealed cell {x}")
            elif len(ops(x)) == 1:
                errs.append(f"dead end at {x}")

    adj, links = {}, []
    for a, b in list(m.ground) + [(a, b) for a, b, _, _ in m.bridges]:
        adj.setdefault(a, set()).add(b)
        adj.setdefault(b, set()).add(a)
        links.append((a, b))

    def reach(skip=None):
        seen, q = {m.start}, deque([m.start])
        while q:
            x = q.popleft()
            for y in adj.get(x, ()):
                if skip and m.edge(x, y) == skip:
                    continue
                if y not in seen:
                    seen.add(y)
                    q.append(y)
        return seen

    total = R * C - len(floorless)
    if len(reach()) != total:
        errs.append(f"only {len(reach())} of {total} cells reachable")
    between = {m.edge(a, b) for a, b in links if m.zone.get(a) != m.zone.get(b)}
    extra = sorted(between - set(m.cuts))
    if extra:
        errs.append(f"links between zones beyond the {len(m.cuts)} crossings: {extra[:3]}")
    for e in m.cuts:
        if m.goal in reach(skip=e):
            errs.append(f"zone crossing {e} is not the only way through")
    return errs


def main():
    first = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    last = int(sys.argv[2]) if len(sys.argv) > 2 else 12
    bad = 0
    for seed in range(first, last):
        rng = random.Random(seed)
        rows, cols = SIZES[seed % len(SIZES)]
        zones = rng.randint(2, 4)
        try:
            m = amaze.Maze(rows, cols, zones, rows * cols // 13, (3, 0), rng, candidates=2)
        except RuntimeError as e:
            bad += 1
            print(f"seed {seed} ({cols}x{rows}, {zones} zones): could not generate: {e}")
            continue
        errs = check(m)
        if errs:
            bad += 1
            print(f"seed {seed} ({cols}x{rows}, {zones} zones): {len(errs)} problems")
            for e in errs[:5]:
                print("   ", e)
        else:
            print(f"seed {seed} ({cols}x{rows}, {zones} zones): ok")
    print(f"{last - first - bad} of {last - first} mazes passed")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
