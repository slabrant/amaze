#!/usr/bin/env python3
"""amaze - printable mazes with bridges that weave over and under.

Rules of the printed maze:
  * Paths are thin corridors through solid ground. Start at the arrow and
    finish at the bullseye. Those two are the only dead ends.
  * A bridge is a straight stretch of path raised over whatever is below:
    solid ground, or a path crossing underneath. A path that goes under a
    bridge stops at one side and comes out the other, its hidden edges
    shown as faint dashes. Where bridges cross, they alternate over and
    under like a basket weave.

Behind the scenes the maze is split into zones: fingers that wind into
each other, bridging over and under one another without ever
connecting. Each zone has exactly one way across to the next. Loops
wander freely inside a zone, even right up against the finish, but
always lead back within their own zone.
"""

import argparse
import colorsys
import heapq
import itertools
import math
import random
import sys
from collections import deque

import cairo

PT_PER_IN = 72
PAGE_W, PAGE_H = 8.5 * PT_PER_IN, 11 * PT_PER_IN

H, V = "h", "v"  # orientation of a bridge or edge
DIRS = ((-1, 0), (1, 0), (0, -1), (0, 1))
BRIDGE_CHOICES = 30  # spots tried per bridge; the best-scoring one wins
CROSSING_CAP = 4     # crossings per bridge that still count toward its score
BESIDE_COST = 0.5    # score lost per cell running right beside a parallel bridge
ZONE_HOP_GAIN = 3.0  # score for a bridge that passes over another zone
ZONE_BLOCK = 2       # zone fingers are built from blocks this many cells wide
ZONE_WIGGLE = 3      # edge-wiggling passes over the zones (times the cell count)
ZONE_SNAKE = 0.9     # how strongly zones grow from their newest tip (winding)
BRIDGE_MEDIAN = 5    # typical bridge length; longer ones trail off (log-normal)
LENGTH_SPREAD = 0.6  # how far lengths stray from typical (log-normal sigma)
CANDIDATES = 16      # mazes built per seed; the longest solution wins
MIN_LOOP = 8         # shortest loop a repair may make: no lone stubs or open rooms
PASSAGE_SHARE = 0.4  # chance a cell under a bridge carries a passage across
SHORTCUT_LIMIT = 3  # most route cells a new loop may cut off the solution


class UnionFind:
    def __init__(self, n):
        self.p = list(range(n))

    def find(self, x):
        while self.p[x] != x:
            self.p[x] = self.p[self.p[x]]
            x = self.p[x]
        return x

    def union(self, a, b):
        ra, rb = self.find(a), self.find(b)
        if ra == rb:
            return False
        self.p[ra] = rb
        return True

    def copy(self):
        uf = UnionFind(0)
        uf.p = self.p[:]
        return uf


def axis(d):
    return H if d[0] == 0 else V


def walk_length(path):
    return sum(abs(p[0] - q[0]) + abs(p[1] - q[1]) for p, q in zip(path, path[1:]))


def add(cell, d, k=1):
    return (cell[0] + d[0] * k, cell[1] + d[1] * k)


class Maze:
    def __init__(self, rows, cols, n_zones, n_bridges, bridge_span, rng,
                 candidates=CANDIDATES):
        self.rows, self.cols = rows, cols
        self._nbrs = {(r, c): [n for n in (add((r, c), d) for d in DIRS) if self.inside(n)]
                      for r in range(rows) for c in range(cols)}
        self.rng = rng
        self.bridge_span = bridge_span
        # Laying the bridges is the slow part, so lay them once and carve
        # several mazes around them, keeping the one with the longest solution.
        # If carvings keep failing, start over with a new bridge layout.
        best, best_len = None, -1
        for _ in range(10):
            if not self._lay_bridges(n_zones, n_bridges):
                continue
            failures = 0
            while candidates > 0 and failures < 3 * CANDIDATES:
                if not self._carve():
                    failures += 1
                    continue
                length = walk_length(self.solve())
                if length > best_len:
                    best, best_len = dict(self.__dict__), length
                candidates -= 1
            if candidates == 0:
                break
        if best is None:
            raise RuntimeError("could not generate a valid maze; try fewer bridges")
        self.__dict__.update(best)
        self._weave()

    # -- helpers ---------------------------------------------------------
    def idx(self, cell):
        return cell[0] * self.cols + cell[1]

    def inside(self, cell):
        return 0 <= cell[0] < self.rows and 0 <= cell[1] < self.cols

    def neighbors(self, cell):
        return self._nbrs[cell]

    @staticmethod
    def orient(a, b):
        return H if a[0] == b[0] else V

    @staticmethod
    def edge(a, b):
        return (a, b) if a < b else (b, a)

    def _span(self, limits, room, median):
        """Random length in cells, end to end, on a log-normal curve: rises
        quickly to a peak near `median`, then trails off slowly, so long
        ones happen now and then, up to the room available."""
        lo, hi = limits
        hi = min(hi or room, room)
        if hi < lo:
            return lo  # too big to fit; the caller's bounds check rejects it
        for _ in range(20):
            n = round(self.rng.lognormvariate(math.log(median), LENGTH_SPREAD))
            if lo <= n <= hi:
                return n
        return lo

    def can_open(self, a, b):
        """May the wall between adjacent cells a and b be knocked down?"""
        if a in self.locked or b in self.locked or self.zone[a] != self.zone[b]:
            return False
        if a in self.floorless or b in self.floorless:
            return False
        o = self.orient(a, b)
        return o not in self.under.get(a, ()) and o not in self.under.get(b, ())

    def ground_degree(self, cell):
        return sum(self.edge(cell, n) in self.ground for n in self.neighbors(cell))

    # -- generation ------------------------------------------------------
    def _lay_bridges(self, n_zones, n_bridges):
        self.under = {}         # cell -> orientations of the bridges above it
        self.buried = set()     # cells where two bridges cross: no floor
        self.solid = set()      # solid ground under a bridge: no floor either
        self.floorless = set()  # both of the above: not part of the maze
        self.bridge_ends = set()
        self.bridges = []       # (end_a, end_b, orientation, [cells under])
        self.locked = set()     # cells that keep exactly one opening
        self.reserved = set()
        self.fixed = set()
        self.ground = set()     # open walls between adjacent cells
        if not self._make_zones(n_zones):
            return False
        uf = UnionFind(self.rows * self.cols)

        # Each bridge is centred in the part of the map with the fewest bridges
        # so far, then placed there where it crosses the most bridges: an even
        # spread, with crossings everywhere for the basket weave.
        for _ in range(n_bridges):
            region = self._emptiest_region()
            options = []
            for _ in range(400):
                opt = self._bridge_option(region)
                if opt:
                    options.append(opt)
                    if len(options) >= BRIDGE_CHOICES:
                        break
            if not options:
                break
            _, _, bridges, solid, layout = max(options, key=lambda t: t[:2])
            self.bridges = bridges
            self.under, self.ground, uf = layout
            self.bridge_ends = {x for a, b, _, _ in bridges for x in (a, b)}
            self.buried = {m for m, s in self.under.items() if len(s) == 2}
            self.solid = solid - self.buried
            self.floorless = self.buried | self.solid
        # Where each bridge lands, and which way it leaves from there.
        self.landing, self.partner = {}, {}
        for a, b, _, _ in self.bridges:
            for here, there in ((a, b), (b, a)):
                d = ((there[0] > here[0]) - (there[0] < here[0]),
                     (there[1] > here[1]) - (there[1] < here[1]))
                self.landing.setdefault(here, []).append(d)
                self.partner.setdefault(here, []).append(there)
        self._bridged = (set(self.ground), uf, dict(self.zone))
        return True

    def _carve(self):
        """Carve one maze around the bridges already laid."""
        ground, uf, zone = self._bridged
        self.ground, uf, self.zone = set(ground), uf.copy(), dict(zone)
        self.locked, self.reserved = set(), set()
        self.start = self.goal = None
        # The one way across from each zone to the next.
        self.cuts = []
        for z in range(self.n_zones - 1):
            options = [(a, b) for a in self.zone if self.zone[a] == z
                       for b in self.neighbors(a) if self.zone[b] == z + 1
                       and not {a, b} & (set(self.under) | set(self.landing))]
            if not options:
                return False
            a, b = self.rng.choice(options)
            self.cuts.append(self.edge(a, b))
            self.ground.add(self.edge(a, b))
            uf.union(self.idx(a), self.idx(b))
            self.reserved.update((a, b))

        # Carve the rest with a recursive backtracker: long winding corridors
        # and few dead ends. Cells already joined by bridges move
        # as one group, so carving never makes a loop. Zones never connect
        # except at their one way across.
        groups = {}
        for r in range(self.rows):
            for c in range(self.cols):
                groups.setdefault(uf.find(self.idx((r, c))), []).append((r, c))
        visited = set()

        def visit(cell):
            members = groups[uf.find(self.idx(cell))]
            visited.update(members)
            others = [m for m in members if m != cell]
            self.rng.shuffle(others)
            return others + [cell]

        stack = visit(self.rng.choice([x for x in self.zone if x not in self.floorless]))
        while True:
            while stack:
                cur = stack[-1]
                opts = [n for n in self.neighbors(cur)
                        if n not in visited and self.can_open(cur, n)]
                if not opts:
                    stack.pop()
                    continue
                n = self.rng.choice(opts)
                self.ground.add(self.edge(cur, n))
                stack.extend(visit(n))
            # A bridge can cut a piece off its zone. Hand that piece to the
            # zone next door and keep carving from there.
            left = [x for x in self.zone if x not in visited and x not in self.floorless]
            if not left:
                break
            joins = [(x, n) for x in left for n in self.neighbors(x) if n in visited
                     and self._can_join(groups[uf.find(self.idx(x))], x, n)]
            if not joins:
                return False
            x, n = self.rng.choice(joins)
            for m in groups[uf.find(self.idx(x))]:
                self.zone[m] = self.zone[n]
            self.ground.add(self.edge(x, n))
            stack = visit(x)

        # A landing reached only across its bridge has no other way out.
        for cell in self.landing:
            if any(self.edge(cell, n) in self.ground for n in self.neighbors(cell)):
                continue
            opts = [n for n in self.neighbors(cell) if self.can_open(cell, n)]
            if not opts:
                continue  # left for the dead-end repairs
            if not self._rewire(cell, opts):
                self.ground.add(self.edge(cell, self.rng.choice(opts)))
        self.moved = {}  # cell -> zones it has belonged to while being repaired
        if not self._pick_ends():
            return False
        return self._resolve_dead_ends()

    def _can_join(self, group, x, n):
        """Could the cut-off group holding x move into n's zone through x-n?"""
        z = self.zone[n]
        saved = {m: self.zone[m] for m in group}
        for m in group:
            self.zone[m] = z
        ok = self.can_open(x, n)
        # Its bridges' cells keep their side passages only within one zone.
        for m in group:
            for k in self.neighbors(m):
                if self.edge(m, k) in self.ground and self.zone[k] != z:
                    ok = False
        self.zone.update(saved)
        return ok

    def _farthest_leaf(self, src, zone):
        """The dead end in `zone` farthest from src along the paths, counting
        each step by the cells it covers (a long bridge counts its length)."""
        dist = {src: 0}
        heap = [(0, src)]
        while heap:
            d, cur = heapq.heappop(heap)
            if d > dist[cur]:
                continue
            for n in self.links(cur):
                nd = d + abs(n[0] - cur[0]) + abs(n[1] - cur[1])
                if nd < dist.get(n, nd + 1):
                    dist[n] = nd
                    heapq.heappush(heap, (nd, n))
        leaves = [x for x in dist if self.zone[x] == zone and x not in self.locked
                  and x not in self.landing and x not in self.under
                  and x not in self.reserved and len(self.openings(x)) == 1]
        return max(leaves, key=dist.get) if leaves else None

    def _pick_ends(self):
        """Start and finish are the dead ends farthest from the way across,
        in the first and last zones: both real dead ends, and each zone's
        stretch of the solution starts out as long as it can be."""
        last = self.n_zones - 1
        a = self._farthest_leaf(self.cuts[0][0] if self.zone[self.cuts[0][0]] == 0
                                else self.cuts[0][1], 0)
        end = self.cuts[-1][0] if self.zone[self.cuts[-1][0]] == last else self.cuts[-1][1]
        b = self._farthest_leaf(end, last)
        if not a or not b:
            return False
        self.start, self.goal = a, b
        self.locked.update((a, b))
        return True

    def _make_zones(self, n):
        """Zones as tangled fingers, grown like coral: each zone in turn adds
        a block a couple of cells wide, but only one that touches exactly one
        of its own blocks, so every zone stays thin and branches out between
        the others. Holes left at the end go to the smallest zone beside them.
        Then order the zones so each touches the next."""
        bs = ZONE_BLOCK
        br, bc = -(-self.rows // bs), -(-self.cols // bs)
        blocks = [(i, j) for i in range(br) for j in range(bc)]
        if n > len(blocks):
            return False

        def around(b):
            return [x for x in ((b[0] + d[0], b[1] + d[1]) for d in DIRS)
                    if 0 <= x[0] < br and 0 <= x[1] < bc]

        owner, size = {}, [0] * n
        for z, b in enumerate(self.rng.sample(blocks, n)):
            owner[b] = z
            size[z] = 1
        active = set(range(n))
        while active:
            z = min(active, key=lambda z: (size[z], self.rng.random()))
            mine = [b for b, o in owner.items() if o == z]
            grow = {x for b in mine for x in around(b) if x not in owner
                    and sum(owner.get(y) == z for y in around(x)) == 1}
            if not grow:
                active.discard(z)
                continue
            # Prefer blocks reaching out from a tip, for long thin fingers.
            tips = [x for x in grow
                    if sum(owner.get(y) == z for y in around(x) for y2 in [y]
                           if sum(owner.get(w) == z for w in around(y)) <= 1)]
            b = self.rng.choice(tips if tips and self.rng.random() < ZONE_SNAKE else sorted(grow))
            owner[b] = z
            size[z] += 1
        while len(owner) < len(blocks):
            for b in blocks:
                if b in owner:
                    continue
                near = [owner[x] for x in around(b) if x in owner]
                if near:
                    owner[b] = min(near, key=lambda z: (size[z], self.rng.random()))
                    size[owner[b]] += 1
        touching = {(owner[a], owner[b]) for a in blocks for b in around(a)}
        for perm in itertools.permutations(range(n)):
            if all((p, q) in touching for p, q in zip(perm, perm[1:])):
                break
        else:
            return False
        rank = {z: i for i, z in enumerate(perm)}
        self.n_zones = n
        self.zone = {(r, c): rank[owner[r // bs, c // bs]]
                     for r in range(self.rows) for c in range(self.cols)}
        self._roughen_zones()
        return self._zones_whole()

    RING = ((-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1))

    def _roughen_zones(self):
        """Zone edges built from blocks run dead straight, and since no path
        crosses them they show as long straight walls. Wiggle them: move edge
        cells one at a time into the zone next door, whenever the cell's own
        zone stays in one piece around it (and neither zone gets too small)."""
        size = {}
        for z in self.zone.values():
            size[z] = size.get(z, 0) + 1
        floor = min(size.values()) * 0.8
        cells = list(self.zone)
        for _ in range(ZONE_WIGGLE * len(cells)):
            x = self.rng.choice(cells)
            z = self.zone[x]
            others = [self.zone[n] for n in self.neighbors(x) if self.zone[n] != z]
            if not others or size[z] - 1 < floor:
                continue
            ring = [self.zone.get(add(x, d)) == z for d in self.RING]
            # Same-zone sides (N, E, S, W sit at even ring places) must all
            # join up through one unbroken run of same-zone ring cells.
            runs, run, first = [], [], None
            for k in range(16):
                if ring[k % 8]:
                    run.append(k % 8)
                elif run:
                    runs.append(run)
                    run = []
            if run:
                runs.append(run)
            sides = {k for k in (0, 2, 4, 6) if ring[k]}
            if not sides or not any(sides <= set(r) for r in runs):
                continue
            new = self.rng.choice(others)
            # No one-cell bumps: a cell with a single neighbour in its own
            # zone could only ever be a dead end. Everyone keeps two.
            self.zone[x] = new
            touched = [x] + [n for n in self.neighbors(x) if self.zone[n] == z]
            if any(sum(self.zone[m] == self.zone[t] for m in self.neighbors(t)) < 2
                   for t in touched):
                self.zone[x] = z
                continue
            size[z] -= 1
            size[new] += 1

    def _zones_whole(self):
        """Is every zone one connected piece?"""
        for z in set(self.zone.values()):
            mine = [x for x, zz in self.zone.items() if zz == z]
            seen, q = {mine[0]}, deque([mine[0]])
            while q:
                for n in self.neighbors(q.popleft()):
                    if n not in seen and self.zone[n] == z:
                        seen.add(n)
                        q.append(n)
            if len(seen) != len(mine):
                return False
        return True

    def _regions(self):
        nr, nc = max(1, self.rows // 10), max(1, self.cols // 8)
        return nr, nc

    def _emptiest_region(self):
        nr, nc = self._regions()
        count = {(i, j): 0 for i in range(nr) for j in range(nc)}
        for r, c in set(self.under) | self.bridge_ends:
            count[r * nr // self.rows, c * nc // self.cols] += 1
        low = min(count.values())
        return self.rng.choice([k for k, v in count.items() if v == low])

    def _bridge_option(self, region):
        """A random valid place for one more bridge centred in `region`, as
        (score, tie-breaker, new bridge list, layout), or None."""
        rng = self.rng
        nr, nc = self._regions()
        i, j = region
        cr = rng.randrange(i * self.rows // nr, (i + 1) * self.rows // nr)
        cc = rng.randrange(j * self.cols // nc, (j + 1) * self.cols // nc)
        o = rng.choice((H, V))
        span = self._span(self.bridge_span, (self.cols if o == H else self.rows) - 1,
                          BRIDGE_MEDIAN)
        if o == H:
            if self.cols <= span:
                return None
            r, c = cr, min(max(0, cc - span // 2), self.cols - 1 - span)
            a, b = (r, c), (r, c + span)
            mids = [(r, c + k) for k in range(1, span)]
        else:
            if self.rows <= span:
                return None
            r, c = min(max(0, cr - span // 2), self.rows - 1 - span), cc
            a, b = (r, c), (r + span, c)
            mids = [(r + k, c) for k in range(1, span)]
        # Under the bridge, some cells carry a passage across and the rest are
        # solid ground. Never two passages side by side: that reads as a
        # ladder of little rooms rather than paths ducking under.
        solid, prev_open = set(self.solid), False
        for m in mids:
            if m in self.under:
                prev_open = False  # a crossing with another bridge
                continue
            if not prev_open and rng.random() < PASSAGE_SHARE:
                prev_open = True
            else:
                solid.add(m)
                prev_open = False
        bridges = self.bridges + [(a, b, o, mids)]
        layout = self._layout(bridges, solid)
        if layout is None or not self._room_to_move(bridges, solid, layout[0], [a, b] + mids):
            return None
        # Crossings make the weave, but only a few count: past that a bridge
        # just piles into a clump. Running right alongside a parallel bridge
        # counts against a spot.
        crossings = sum(m in self.under for m in mids)
        side = ((1, 0), (-1, 0)) if o == H else ((0, 1), (0, -1))
        parallel = {x for x, s in self.under.items() if o in s} | self.bridge_ends
        beside = sum(add(x, d) in parallel for x in [a, b] + mids for d in side)
        hops = any(self.zone[m] != self.zone[a] for m in mids)
        score = min(crossings, CROSSING_CAP) - BESIDE_COST * beside + ZONE_HOP_GAIN * hops
        return score, rng.random(), bridges, solid, layout

    def _room_to_move(self, bridges, solid, under, cells):
        """Would every cell near these keep at least two possible ways out?
        One with fewer, walled in by solid ground, could only be a dead end."""
        floorless = solid | {m for m, s in under.items() if len(s) == 2}
        landings = {}
        for a, b, _, _ in bridges:
            landings[a] = landings.get(a, 0) + 1
            landings[b] = landings.get(b, 0) + 1
        near = {add(x, (dr, dc)) for x in cells for dr in (-1, 0, 1) for dc in (-1, 0, 1)}
        for x in near:
            if not self.inside(x) or x in floorless or x in under:
                continue
            ways = landings.get(x, 0)
            for n in self.neighbors(x):
                o = self.orient(x, n)
                if (n not in floorless and self.zone[n] == self.zone[x]
                        and o not in under.get(n, ())):
                    ways += 1
            if ways < 2:
                return False
        return True

    def _layout(self, bridges, solid):
        """Work out what lies under a set of bridges, or None if it can't work.

        A cell under one bridge is either solid ground or carries a straight
        passage across it. A cell where two bridges cross has no floor at all:
        one bridge passes over the other. Bridges never run along on top of
        each other.
        """
        under, ends = {}, set()
        for a, b, o, mids in bridges:
            ends.update((a, b))
            for m in mids:
                if o in under.setdefault(m, set()):
                    return None
                under[m].add(o)
        if any(x in under for x in ends) or any(x in self.locked for x in under):
            return None
        if any(x in self.reserved for x in ends) or any(x in self.reserved for x in under):
            return None
        uf = UnionFind(self.rows * self.cols)
        for a, b in self.fixed:
            uf.union(self.idx(a), self.idx(b))
        for a, b, _, _ in bridges:
            if self.zone[a] != self.zone[b] or not uf.union(self.idx(a), self.idx(b)):
                return None
        forced = set()
        for m, s in under.items():
            if len(s) == 2 or m in solid:
                continue
            o = next(iter(s))
            for d in (((-1, 0), (1, 0)) if o == H else ((0, -1), (0, 1))):
                n = add(m, d)
                if not self.inside(n) or n in self.locked or self.zone[n] != self.zone[m]:
                    return None
                if axis(d) in under.get(n, ()) or n in solid:
                    return None  # the passage would run under n's bridge, or into rock
                e = self.edge(m, n)
                if e not in forced:
                    if not uf.union(self.idx(m), self.idx(n)):
                        return None
                    forced.add(e)
        return under, forced, uf

    def _weave(self):
        """Choose which bridge is on top at each crossing, flipping between
        neighbouring crossings along every bridge like a basket weave."""
        over = {}  # crossing cell -> indices of the bridges through it
        for i, (_, _, _, mids) in enumerate(self.bridges):
            for m in mids:
                if m in self.buried:
                    over.setdefault(m, []).append(i)
        nexts = {m: [] for m in over}  # crossings next to each other on a bridge
        for _, _, _, mids in self.bridges:
            row = [m for m in mids if m in over]
            for p, q in zip(row, row[1:]):
                nexts[p].append(q)
                nexts[q].append(p)
        h_on_top = {}
        for seed_cell in sorted(over):
            if seed_cell in h_on_top:
                continue
            h_on_top[seed_cell] = self.rng.random() < 0.5
            q = deque([seed_cell])
            while q:
                cur = q.popleft()
                for n in nexts[cur]:
                    if n not in h_on_top:
                        h_on_top[n] = not h_on_top[cur]
                        q.append(n)
        self.on_top = {}
        for m, pair in over.items():
            want = H if h_on_top[m] else V
            self.on_top[m] = next(i for i in pair if self.bridges[i][2] == want)

    # -- dead ends -------------------------------------------------------
    def openings(self, cell):
        """Directions in which this cell visibly opens."""
        out = []
        for d in DIRS:
            n = add(cell, d)
            if self.inside(n) and self.edge(cell, n) in self.ground:
                out.append(d)
        return out + self.landing.get(cell, [])

    def dead_ends(self):
        """Cells with a single way out, other than the start and finish."""
        return [x for x in self.zone if x not in self.under and x not in (self.start, self.goal)
                and len(self.openings(x)) == 1]

    def _attach_points(self):
        """For each cell, the index of the point on the start-goal route that
        its branch hangs off (the maze is still loop-free when this runs)."""
        jumps = {}
        for a, b, _, _ in self.bridges:
            jumps.setdefault(a, []).append(b)
            jumps.setdefault(b, []).append(a)

        def links(cur):
            return [n for n in self.neighbors(cur)
                    if self.edge(cur, n) in self.ground] + jumps.get(cur, [])

        prev = {self.start: None}
        q = deque([self.start])
        while q:
            cur = q.popleft()
            for n in links(cur):
                if n not in prev:
                    prev[n] = cur
                    q.append(n)
        if self.goal not in prev:
            self._route = set()
            return {}  # not joined up yet; every join counts as local
        route, cur = [], self.goal
        while cur is not None:
            route.append(cur)
            cur = prev[cur]
        self._route = set(route)
        attach = {cell: i for i, cell in enumerate(reversed(route))}
        q = deque(attach)
        while q:
            cur = q.popleft()
            for n in links(cur):
                if n not in attach:
                    attach[n] = attach[cur]
                    q.append(n)
        return attach

    def _rewire(self, cell, opts):
        """Fix dead end `cell` without adding a loop: open one of its walls,
        then close another wall on the loop that just formed, choosing one
        whose two cells keep at least two openings each."""
        for n in self.rng.sample(opts, len(opts)):
            prev = {cell: None}
            q = deque([cell])
            while q and n not in prev:
                cur = q.popleft()
                for m in self.links(cur):
                    if m not in prev:
                        prev[m] = cur
                        q.append(m)
            if n not in prev:
                continue
            self.ground.add(self.edge(cell, n))
            path, cur = [], n
            while prev[cur] is not None:
                path.append((prev[cur], cur))
                cur = prev[cur]
            closable = [(u, v) for u, v in path
                        if self.edge(u, v) in self.ground and (u, v) != (cell, n)
                        and len(self.openings(u)) >= 3 and len(self.openings(v)) >= 3]
            if closable:
                u, v = self.rng.choice(closable)
                self.ground.discard(self.edge(u, v))
                return u, v
            self.ground.discard(self.edge(cell, n))
        return False

    def _loop_length(self, a, b):
        """Steps around the loop that opening the wall a-b would make: the
        current walk from a to b plus the new step (capped at MIN_LOOP)."""
        dist, q = {a: 0}, deque([a])
        while q:
            cur = q.popleft()
            if dist[cur] + 1 >= MIN_LOOP:
                break
            for n in self.links(cur):
                if n not in dist:
                    dist[n] = dist[cur] + 1
                    if n == b:
                        return dist[n] + 1
                    q.append(n)
        return MIN_LOOP

    def _change_zone(self, cell):
        """A dead end walled in by other zones' cells: unhook it from its own
        zone and join a neighbouring zone instead, opening onto two of that
        zone's cells at once so it is no longer a dead end. It was a leaf, so
        nothing is cut off; its old neighbour may become a dead end in turn."""
        if cell in self.bridge_ends or cell in self.reserved or cell in self.locked:
            return False
        (d,) = self.openings(cell)
        old = add(cell, d)
        if self.edge(cell, old) in self.cuts:
            return False
        if old in self.landing and len(self.openings(old)) <= 2:
            return False  # would leave that landing with only its bridge
        if old in self.under:
            return False  # the passage under a bridge must keep both ends
        z = self.zone[cell]
        been = self.moved.setdefault(cell, {z})
        by_zone = {}
        for n in self.neighbors(cell):
            nz = self.zone.get(n, -1)
            if n == old or nz in been or nz == -1:
                continue
            self.zone[cell] = nz
            if self.can_open(cell, n):
                by_zone.setdefault(nz, []).append(n)
            self.zone[cell] = z
        choices = [ns for ns in by_zone.values() if len(ns) >= 2]
        if choices:
            ns = self.rng.sample(self.rng.choice(choices), 2)
        elif by_zone and len(been) <= 3:
            # Only one way into the other zone: join it as a dead end there and
            # let its old neighbour, now perhaps a dead end, peel off next.
            ns = [self.rng.choice(self.rng.choice(list(by_zone.values())))]
        else:
            return False
        been.add(self.zone[ns[0]])
        self.ground.discard(self.edge(cell, old))
        self.zone[cell] = self.zone[ns[0]]
        for n in ns:
            self.ground.add(self.edge(cell, n))
        return True

    def _resolve_dead_ends(self):
        attach = self._attach_points()
        for _ in range(self.rows * self.cols):
            todo = self.dead_ends()
            if not todo:
                return self._all_reachable()
            dead = set(todo)
            self.rng.shuffle(todo)
            progress = False
            for cell in todo:
                if len(self.openings(cell)) != 1:
                    continue  # already fixed by a neighbour this round
                opts = [n for n in self.neighbors(cell)
                        if self.edge(cell, n) not in self.ground and self.can_open(cell, n)]
                if not opts:
                    progress |= self._change_zone(cell)
                    continue
                # Every loop surrounds an island of wall. A tiny loop leaves a
                # lone stub or an open 2x2 room, which looks wrong, so only
                # make loops at least MIN_LOOP steps around. Among those, join
                # cells hanging off nearby points of the true route, so the
                # loop cannot cut much off the solution.
                jump = lambda n: abs(attach.get(cell, 0) - attach.get(n, attach.get(cell, 0)))
                loop = {n: self._loop_length(cell, n) for n in opts}
                good = [n for n in opts if loop[n] >= MIN_LOOP]
                if good:
                    n = min(good, key=lambda n: (jump(n), n not in dead, self.rng.random()))
                if not good or jump(n) > SHORTCUT_LIMIT:
                    # Rewiring fixes the dead end without any new loop.
                    closed = self._rewire(cell, opts)
                    if closed:
                        # Route points only move if the route itself changed.
                        u, v = closed
                        if (u in self._route and v in self._route) or \
                                attach.get(cell) != attach.get(u) or attach.get(u) != attach.get(v):
                            attach = self._attach_points()
                        progress = True
                        continue
                    if not good:  # last resort: the roomiest loop on offer
                        n = max(opts, key=lambda n: (loop[n], self.rng.random()))
                self.ground.add(self.edge(cell, n))
                progress = True
            if not progress:
                return False
        return False

    # -- solving ---------------------------------------------------------
    def _all_reachable(self):
        seen = {self.start}
        q = deque([self.start])
        while q:
            for n in self.links(q.popleft()):
                if n not in seen:
                    seen.add(n)
                    q.append(n)
        return len(seen) == self.rows * self.cols - len(self.floorless)

    def links(self, cell):
        for n in self.neighbors(cell):
            if self.edge(cell, n) in self.ground:
                yield n
        yield from self.partner.get(cell, ())

    def solve(self):
        prev = {self.start: None}
        q = deque([self.start])
        while q:
            cur = q.popleft()
            if cur == self.goal:
                break
            for n in self.links(cur):
                if n not in prev:
                    prev[n] = cur
                    q.append(n)
        if self.goal not in prev:
            return None
        path, cur = [], self.goal
        while cur is not None:
            path.append(cur)
            cur = prev[cur]
        return path[::-1]


# -- drawing ---------------------------------------------------------------
def path_colour(t):
    """Solution colour at fraction t of the way: red, orange, green, blue, violet."""
    return colorsys.hsv_to_rgb(0.78 * t, 0.9, 0.85 - 0.15 * math.sin(math.pi * min(t, 0.4) / 0.4))


PATH_WIDTH = 0.75          # paths are this fraction of a cell; thin walls take the rest
HATCH = (0.7, 0.7, 0.7)    # faint hatching for the solid ground (easy on ink)
ROCK = (0.86, 0.86, 0.86)  # zone tints fall back to this for floorless cells
HIDDEN = (0.6, 0.6, 0.6)   # dashes for path edges hidden under a bridge
ZONE_TINTS = [(0.98, 0.8, 0.8), (0.78, 0.87, 0.98), (0.8, 0.95, 0.8), (0.98, 0.92, 0.72)]


def draw_maze(ctx, maze, x0, y0, s, solution=None, graph_paper=False, show_zones=False):
    """Draw the maze as thin paths through solid ground. A bridge is just a
    path drawn on top: whatever runs under it stops at one side and comes
    out the other, its hidden edges shown as faint dashes."""
    R, C = maze.rows, maze.cols
    start, goal = maze.start, maze.goal
    p0 = s * (1 - PATH_WIDTH) / 2  # a path's edges, measured into its cell
    p1 = s - p0
    wall_w = max(0.6, s * 0.06)
    sol_w = max(1.0, s * 0.2)

    def line(x1, y1, x2, y2):
        ctx.move_to(x0 + x1, y0 + y1)
        ctx.line_to(x0 + x2, y0 + y2)

    def rect(x1, y1, x2, y2):
        ctx.rectangle(x0 + min(x1, x2), y0 + min(y1, y2), abs(x2 - x1), abs(y2 - y1))

    def center(cell):
        return x0 + (cell[1] + 0.5) * s, y0 + (cell[0] + 0.5) * s

    def ink(colour=(0, 0, 0), width=wall_w):
        ctx.set_source_rgb(*colour)
        ctx.set_line_width(width)
        ctx.set_line_cap(cairo.LINE_CAP_SQUARE)

    # Solid ground: faint diagonal hatching (each zone tinted, if asked),
    # then the white path floor goes over it.
    if show_zones:
        for (r, c), z in maze.zone.items():
            ctx.set_source_rgb(*(ZONE_TINTS[z % len(ZONE_TINTS)] if z >= 0 else ROCK))
            rect(c * s, r * s, (c + 1) * s, (r + 1) * s)
            ctx.fill()
    ctx.save()
    rect(0, 0, C * s, R * s)
    ctx.clip()
    ink(HATCH, max(0.25, s * 0.02))
    step = max(1.5, s * 0.14)
    k = -R * s
    while k < C * s:
        line(k, 0, k + R * s, R * s)
        k += step
    ctx.stroke()
    ctx.restore()
    if graph_paper:
        ink((0.72, 0.82, 0.93), 0.3)
        for r in range(R + 1):
            line(0, r * s, C * s, r * s)
        for c in range(C + 1):
            line(c * s, 0, c * s, R * s)
        ctx.stroke()

    ctx.set_source_rgb(1, 1, 1)
    for r in range(R):
        for c in range(C):
            dirs = maze.under.get((r, c))
            if not dirs:
                rect(c * s + p0, r * s + p0, c * s + p1, r * s + p1)
            elif len(dirs) == 1 and (r, c) not in maze.solid:  # passage under a bridge
                if H in dirs:
                    rect(c * s + p0, r * s, c * s + p1, (r + 1) * s)
                else:
                    rect(c * s, r * s + p0, (c + 1) * s, r * s + p1)
    for (r1, c1), (r2, c2) in maze.ground:
        rect(c1 * s + p0, r1 * s + p0, c2 * s + p1, r2 * s + p1)
    ctx.fill()

    # Start and finish fill their whole path square.
    for cell, t in ((start, 0), (goal, 1)):
        ctx.set_source_rgb(*path_colour(t))
        rect(cell[1] * s + p0, cell[0] * s + p0, cell[1] * s + p1, cell[0] * s + p1)
        ctx.fill()

    # How far along the solution each step starts and ends, from 0 to 1,
    # so the line can shift colour from entrance to exit.
    progress = {}
    bridge_pairs = {maze.edge(a, b) for a, b, _, _ in maze.bridges}
    bridge_segs = []
    if solution:
        total = walk_length(solution) or 1
        done = 0
        for p, q in zip(solution, solution[1:]):
            step = abs(p[0] - q[0]) + abs(p[1] - q[1])
            progress[p, q] = (done / total, (done + step) / total)
            done += step
        progress[start, start] = (0, 0)

    def stroke_segments(segs):
        ctx.set_line_width(sol_w)
        ctx.set_line_cap(cairo.LINE_CAP_ROUND)
        for p, q in segs:
            t0, t1 = progress[p, q]
            (px, py), (qx, qy) = center(p), center(q)
            if p == q:
                ctx.set_source_rgb(*path_colour(t0))
            else:
                grad = cairo.LinearGradient(px, py, qx, qy)
                for k in range(5):  # a few stops keep long bridges smooth
                    f = k / 4
                    grad.add_color_stop_rgb(f, *path_colour(t0 + (t1 - t0) * f))
                ctx.set_source(grad)
            ctx.move_to(px, py)
            ctx.line_to(qx, qy)
            ctx.stroke()

    # The solution on the ground goes down first: bridges cover it.
    if solution:
        segs = list(zip(solution, solution[1:]))
        bridge_segs = [(p, q) for p, q in segs if maze.edge(p, q) in bridge_pairs]
        stroke_segments([(p, q) for p, q in segs if maze.edge(p, q) not in bridge_pairs]
                        + [(start, start)])

    # Path edges on the ground. Each side of a path square is either open
    # (two short edges run out to the cell's side) or closed (one edge).
    ink()
    for r in range(R):
        for c in range(C):
            cell = (r, c)
            x, y = c * s, r * s
            dirs = maze.under.get(cell)
            if cell in maze.floorless:
                continue  # solid, or two bridges cross here: no ground at all
            if dirs:  # passage under a bridge: its edges stop at the bridge
                if H in dirs:  # bridge runs across, passage runs up and down
                    for ex in (x + p0, x + p1):
                        line(ex, y, ex, y + p0)
                        line(ex, y + p1, ex, y + s)
                else:
                    for ey in (y + p0, y + p1):
                        line(x, ey, x + p0, ey)
                        line(x + p1, ey, x + s, ey)
                continue
            opens = set(maze.openings(cell))
            for (dr, dc) in DIRS:
                if (dr, dc) in opens:  # two edges out to the side
                    if dr:
                        ey0, ey1 = (y, y + p0) if dr < 0 else (y + p1, y + s)
                        line(x + p0, ey0, x + p0, ey1)
                        line(x + p1, ey0, x + p1, ey1)
                    else:
                        ex0, ex1 = (x, x + p0) if dc < 0 else (x + p1, x + s)
                        line(ex0, y + p0, ex1, y + p0)
                        line(ex0, y + p1, ex1, y + p1)
                else:  # closed side
                    if dr:
                        ey = y + (p0 if dr < 0 else p1)
                        line(x + p0, ey, x + p1, ey)
                    else:
                        ex = x + (p0 if dc < 0 else p1)
                        line(ex, y + p0, ex, y + p1)
    ctx.stroke()

    # Bridges: white deck from landing to landing, then its two edges.
    on_bridge = {maze.edge(p, q): (p, q) for p, q in bridge_segs}

    def draw_bridge(a, b, o):
        (ra, ca), (rb, cb) = a, b
        if o == H:
            xs, xe, y = ca * s + p1, cb * s + p0, ra * s
            ctx.set_source_rgb(1, 1, 1)
            rect(xs, y + p0, xe, y + p1)
            ctx.fill()
            ink()
            line(xs, y + p0, xe, y + p0)
            line(xs, y + p1, xe, y + p1)
        else:
            ys, ye, x = ra * s + p1, rb * s + p0, ca * s
            ctx.set_source_rgb(1, 1, 1)
            rect(x + p0, ys, x + p1, ye)
            ctx.fill()
            ink()
            line(x + p0, ys, x + p0, ye)
            line(x + p1, ys, x + p1, ye)
        ctx.stroke()
        if maze.edge(a, b) in on_bridge:
            stroke_segments([on_bridge[maze.edge(a, b)]])

    for a, b, o, _ in maze.bridges:
        draw_bridge(a, b, o)
    # Where two bridges cross, redraw just that cell of whichever one is on
    # top there. Alternating tops along each bridge gives the basket weave.
    for (r, c), i in maze.on_top.items():
        a, b, o, _ = maze.bridges[i]
        ctx.save()
        rect(c * s, r * s, (c + 1) * s, (r + 1) * s)
        ctx.clip()
        draw_bridge(a, b, o)
        ctx.restore()

    # Hidden edges: where something passes under a bridge, its edges show
    # through as faint dashes, the way drawings show hidden lines.
    ink(HIDDEN, max(0.5, wall_w * 0.7))
    ctx.set_dash([s * 0.08, s * 0.07])
    for (r, c), dirs in maze.under.items():
        x, y = c * s, r * s
        if (r, c) in maze.solid:
            continue
        if len(dirs) == 2:  # the lower bridge, under the top one
            top = maze.bridges[maze.on_top[r, c]][2]
            hidden = V if top == H else H
        else:  # the passage under the bridge
            hidden = V if H in dirs else H
        if hidden == V:
            for ex in (x + p0, x + p1):
                line(ex, y + p0, ex, y + p1)
        else:
            for ey in (y + p0, y + p1):
                line(x + p0, ey, x + p1, ey)
    ctx.stroke()
    ctx.set_dash([])

    # Start arrow and finish bullseye, white, sized to the path, on top.
    w = s * 0.6  # symbols a little narrower than the path
    for cell in (start, goal):
        cx, cy = center(cell)
        if cell == goal:
            for radius, colour in ((0.42, (1, 1, 1)), (0.27, path_colour(1)), (0.12, (1, 1, 1))):
                ctx.arc(cx, cy, radius * w, 0, 2 * math.pi)
                ctx.set_source_rgb(*colour)
                ctx.fill()
            continue
        (dr, dc), = maze.openings(cell)  # point out through the one way out
        tip = (cx + dc * 0.4 * w, cy + dr * 0.4 * w)
        back = (cx - dc * 0.32 * w, cy - dr * 0.32 * w)
        ctx.move_to(*tip)
        ctx.line_to(back[0] - dr * 0.36 * w, back[1] + dc * 0.36 * w)
        ctx.line_to(back[0] + dr * 0.36 * w, back[1] - dc * 0.36 * w)
        ctx.close_path()
        ctx.set_source_rgb(1, 1, 1)
        ctx.fill()

    # A frame around it all.
    ink()
    rect(0, 0, C * s, R * s)
    ctx.stroke()


def render_in_box(ctx, maze, seed, solution, args, label, box, font):
    """Draw one maze centred in box (x, y, w, h), with its label below on
    the left and its seed, small and faint, below on the right."""
    bx, by, bw, bh = box
    gap = font * 1.6
    s = min(bw / maze.cols, (bh - gap) / maze.rows)
    x0 = bx + (bw - s * maze.cols) / 2
    y0 = by + (bh - gap - s * maze.rows) / 2
    draw_maze(ctx, maze, x0, y0, s, solution, args.graph_paper,
              show_zones=args.show_zones and solution is not None)
    base = y0 + s * maze.rows + gap * 0.8
    ctx.select_font_face("Sans", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
    ctx.set_font_size(font)
    ctx.set_source_rgb(0.3, 0.3, 0.3)
    ctx.move_to(x0, base)
    ctx.show_text(label)
    # The seed, small and faint, so a maze can be made again.
    ctx.set_font_size(font * 0.75)
    ctx.set_source_rgb(0.6, 0.6, 0.6)
    text = f"seed {seed}"
    ctx.move_to(x0 + s * maze.cols - ctx.text_extents(text).x_advance, base)
    ctx.show_text(text)


def render_pages(ctx, items, args):
    """Lay items (maze, seed, solution, label) out one or four to a page."""
    margin = 0.5 * PT_PER_IN
    per = args.per_page
    cols = 1 if per == 1 else 2
    rows = per // cols
    gutter = 0.3 * PT_PER_IN
    cw = (PAGE_W - 2 * margin - (cols - 1) * gutter) / cols
    ch = (PAGE_H - 2 * margin - (rows - 1) * gutter) / rows
    font = 9 if per == 1 else 7.5
    for start in range(0, len(items), per):
        ctx.set_source_rgb(1, 1, 1)
        ctx.paint()
        for k, (maze, seed, solution, label) in enumerate(items[start:start + per]):
            r, c = divmod(k, cols)
            box = (margin + c * (cw + gutter), margin + r * (ch + gutter), cw, ch)
            render_in_box(ctx, maze, seed, solution, args, label, box, font)
        ctx.show_page()


def main(argv=None):
    p = argparse.ArgumentParser(
        description="Generate printable mazes with bridges that weave over and under.")
    p.add_argument("-o", "--out", default="maze.pdf", help="output PDF path")
    p.add_argument("--cols", type=int, default=None,
                   help="maze width in cells (default 36, or 18 at 4 per page)")
    p.add_argument("--rows", type=int, default=None,
                   help="maze height in cells (default 45, or 22 at 4 per page)")
    p.add_argument("--zones", type=int, default=None,
                   help="zones the maze is split into, each with exactly one way "
                        "across to the next (default: 2 to 4, picked at random)")
    p.add_argument("--bridges", type=int, default=None,
                   help="how many bridges (default: scales with maze size)")
    p.add_argument("--bridge-span", type=int, nargs=2, default=(3, 0), metavar=("MIN", "MAX"),
                   help="bridge length in cells, landing to landing; MAX 0 means "
                        "up to the full map; lengths follow a log-normal curve "
                        "around 5 (default 3 0)")
    p.add_argument("--count", type=int, default=1, help="number of mazes")
    p.add_argument("--per-page", type=int, choices=(1, 4), default=1,
                   help="mazes per page: 1, or 4 small ones in a 2x2 grid (default 1)")
    p.add_argument("--seed", type=int, default=None, help="seed for reproducible mazes")
    p.add_argument("--solution", action="store_true",
                   help="add solutions, all at the back")
    p.add_argument("--show-zones", action="store_true",
                   help="tint each zone a pale colour on the solution pages")
    p.add_argument("--graph-paper", action="store_true",
                   help="draw a faint blue graph-paper grid behind the maze")
    args = p.parse_args(argv)

    small = args.per_page == 4
    args.cols = args.cols or (18 if small else 36)
    args.rows = args.rows or (22 if small else 45)
    lo, hi = args.bridge_span
    if lo < 2 or (hi and hi < lo):
        p.error("need 2 <= MIN <= MAX (or MAX 0) for --bridge-span")
    area = args.rows * args.cols
    n_bridges = args.bridges if args.bridges is not None else max(1, area // 13)

    base_seed = args.seed if args.seed is not None else random.randrange(10**6)
    surface = cairo.PDFSurface(args.out, PAGE_W, PAGE_H)
    ctx = cairo.Context(surface)
    puzzles, answers = [], []
    for i in range(args.count):
        seed = base_seed + i
        rng = random.Random(seed)
        zones = args.zones or rng.randint(2, 4)
        maze = Maze(args.rows, args.cols, zones, n_bridges, args.bridge_span, rng)
        solution = maze.solve()
        if solution is None:
            raise RuntimeError(f"seed {seed}: goal unreachable (bug)")
        number = f"Maze {i + 1}" if args.count > 1 else ""
        puzzles.append((maze, seed, None, number))
        answers.append((maze, seed, solution, f"{number} solution" if number else "Solution"))
        print(f"maze {i + 1}: seed {seed}, {zones} zones, {len(maze.bridges)} bridges, "
              f"solution walks {walk_length(solution)} cells", file=sys.stderr)
    # All the solutions go at the back, in the same order as the mazes,
    # starting on a fresh page.
    render_pages(ctx, puzzles, args)
    if args.solution:
        render_pages(ctx, answers, args)
    surface.finish()
    print(f"wrote {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
