"""Pure schematic layout: positions boxes and routes relationship edges in
world (character-cell) coordinates. No terminal code lives here.

This is a faithful port of ``viewer/rust/src/layout.rs``; given the same
inputs both implementations produce the same geometry (see
``tests/test_parity.py``).

* Collapsed items are fixed-size boxes (size depends on :class:`Zoom`).
* An expanded item is a container whose children are arranged in layered
  columns: longest-path layering over the enabled links projected onto the
  siblings, so dependencies flow left to right like a schematic.
* Edges are "lifted" to the nearest visible ancestor of each endpoint and
  merged when several links collapse onto the same pair of boxes.
"""

from __future__ import annotations

from collections.abc import Collection
from dataclasses import dataclass, field
from enum import Enum
from itertools import accumulate

from .model import Graph, NodeId

Point = tuple[int, int]


class Zoom(Enum):
    """Discrete detail levels; terminals cannot scale glyphs, so "zoom"
    changes how much each box shows and therefore how big it is."""

    DETAIL = 0
    COMPACT = 1
    OVERVIEW = 2

    def zoom_in(self) -> Zoom:
        return Zoom.COMPACT if self is Zoom.OVERVIEW else Zoom.DETAIL

    def zoom_out(self) -> Zoom:
        return Zoom.COMPACT if self is Zoom.DETAIL else Zoom.OVERVIEW

    @property
    def label(self) -> str:
        return self.name.lower()

    @property
    def collapsed_size(self) -> tuple[int, int]:
        return {Zoom.DETAIL: (26, 5), Zoom.COMPACT: (20, 3), Zoom.OVERVIEW: (8, 3)}[self]

    @property
    def col_gap(self) -> int:
        return {Zoom.DETAIL: 8, Zoom.COMPACT: 6, Zoom.OVERVIEW: 4}[self]

    @property
    def pad(self) -> tuple[int, int]:
        return {Zoom.DETAIL: (2, 1), Zoom.COMPACT: (1, 1), Zoom.OVERVIEW: (1, 0)}[self]


ROW_GAP = 1
MARGIN = 2
MAX_HEADER_CHARS = 40

# Router cost model (identical to the Rust implementation).
CROSS_BOX = 60  # per cell of wire through an unrelated leaf box
CROSS_CONTAINER = 6  # per cell of wire through an unrelated container
OVERLAP = 4  # per cell shared with an already-routed wire
BEND = 2
DETOUR = 6  # extra cost of a 5-segment detour over a 3-segment route


def tdiv(a: int, b: int) -> int:
    """Integer division truncating toward zero (Rust semantics)."""
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b > 0) else -q


@dataclass(frozen=True)
class Bounds:
    x: int
    y: int
    w: int
    h: int

    @property
    def right(self) -> int:
        """First column to the right of the box."""
        return self.x + self.w

    @property
    def bottom(self) -> int:
        """First row below the box."""
        return self.y + self.h

    @property
    def center(self) -> Point:
        return (self.x + self.w // 2, self.y + self.h // 2)

    def intersects(self, o: Bounds) -> bool:
        return self.x < o.right and o.x < self.right and self.y < o.bottom and o.y < self.bottom

    def strictly_contains(self, o: Bounds) -> bool:
        return o.x > self.x and o.y > self.y and o.right < self.right and o.bottom < self.bottom

    def contains_point(self, x: int, y: int) -> bool:
        return self.x <= x < self.right and self.y <= y < self.bottom


@dataclass
class PlacedBox:
    node: NodeId
    bounds: Bounds
    depth: int  # nesting depth relative to the current view root
    expanded: bool
    parent: int | None  # index into Layout.boxes of the containing box


@dataclass
class Edge:
    source: NodeId
    target: NodeId
    type: str
    count: int  # number of underlying links merged into this edge
    points: list[Point]  # orthogonal polyline, outside both boxes
    from_port: Point  # border cell on the source box
    to_port: Point  # border cell on the target box


@dataclass
class Layout:
    boxes: list[PlacedBox] = field(default_factory=list)  # parents precede children
    edges: list[Edge] = field(default_factory=list)
    width: int = 0
    height: int = 0
    hidden_links: int = 0  # enabled links leaving the current (drilled-in) view
    by_node: dict[NodeId, int] = field(default_factory=dict)

    def box_of(self, n: NodeId) -> PlacedBox | None:
        i = self.by_node.get(n)
        return None if i is None else self.boxes[i]

    def visible(self, n: NodeId) -> bool:
        return n in self.by_node


def header_text(graph: Graph, n: NodeId, zoom: Zoom) -> str:
    """Text shown in an expanded container's top border."""
    item = graph.item(n)
    s = item.id if zoom is Zoom.OVERVIEW else f"{item.id} {item.title}"
    return s[:MAX_HEADER_CHARS]


def compute(
    graph: Graph,
    root: NodeId | None,
    expanded: Collection[NodeId],
    zoom: Zoom,
    enabled: Collection[str],
) -> Layout:
    ctx = _Ctx(graph, expanded, zoom, _sibling_edges(graph, enabled))
    _, _, placed = ctx.arrange(graph.top_children(root), root)
    boxes: list[PlacedBox] = []
    for sub, x, y in placed:
        _flatten(sub, x + MARGIN, y + MARGIN, 0, None, boxes)
    by_node = {b.node: i for i, b in enumerate(boxes)}
    edges, hidden = _route_edges(graph, enabled, boxes, by_node)

    width = height = 0
    for b in boxes:
        width = max(width, b.bounds.right)
        height = max(height, b.bounds.bottom)
    for e in edges:
        for x, y in e.points:
            width = max(width, x + 1)
            height = max(height, y + 1)
    return Layout(boxes, edges, width + MARGIN, height + MARGIN, hidden, by_node)


@dataclass
class _Sub:
    node: NodeId
    w: int
    h: int
    expanded: bool
    children: list[tuple[_Sub, int, int]]


class _Ctx:
    def __init__(self, graph: Graph, expanded: Collection[NodeId], zoom: Zoom, sib: dict) -> None:
        self.graph = graph
        self.expanded = expanded
        self.zoom = zoom
        self.sib = sib

    def sub(self, n: NodeId) -> _Sub:
        kids = self.graph.children(n)
        min_w, min_h = self.zoom.collapsed_size
        if n not in self.expanded or not kids:
            return _Sub(n, min_w, min_h, False, [])
        cw, ch, placed = self.arrange(kids, n)
        px, py = self.zoom.pad
        header_w = len(header_text(self.graph, n, self.zoom)) + 9
        return _Sub(
            n,
            max(cw + 2 * px + 2, header_w, min_w),
            ch + 2 * py + 2,
            True,
            [(s, x + 1 + px, y + 1 + py) for s, x, y in placed],
        )

    def arrange(self, nodes: list[NodeId], container: NodeId | None) -> tuple[int, int, list[tuple[_Sub, int, int]]]:
        if not nodes:
            return 0, 0, []
        subs = [self.sub(c) for c in nodes]
        pos = {n: i for i, n in enumerate(nodes)}
        edges = [(pos[a], pos[b]) for a, b in sorted(self.sib.get(container, ())) if a in pos and b in pos]
        layers = _layer_assignment(len(nodes), edges)
        cols: list[list[int]] = [[] for _ in range(max(layers) + 1)]
        for i, l in enumerate(layers):
            cols[l].append(i)
        col_h = [sum(subs[i].h for i in c) + ROW_GAP * (len(c) - 1) for c in cols]
        total_h = max(col_h)
        gap = self.zoom.col_gap
        offsets = [(0, 0)] * len(nodes)
        x = 0
        for ci, col in enumerate(cols):
            cw = max(subs[i].w for i in col)
            y = (total_h - col_h[ci]) // 2
            for i in col:
                offsets[i] = (x, y)
                y += subs[i].h + ROW_GAP
            x += cw + gap
        return x - gap, total_h, [(s, ox, oy) for s, (ox, oy) in zip(subs, offsets)]


def _layer_assignment(n: int, edges: list[tuple[int, int]]) -> list[int]:
    """Longest-path layering; cycles are broken by ignoring DFS back edges."""
    adj: list[list[int]] = [[] for _ in range(n)]
    for a, b in edges:
        if a != b:
            adj[a].append(b)
    state = [0] * n
    dag: list[list[int]] = [[] for _ in range(n)]
    order: list[int] = []

    def dfs(u: int) -> None:
        state[u] = 1
        for v in adj[u]:
            if state[v] == 0:
                dag[u].append(v)
                dfs(v)
            elif state[v] == 2:
                dag[u].append(v)
            # state 1: back edge (cycle) – ignored for layering
        state[u] = 2
        order.append(u)

    for s in range(n):
        if state[s] == 0:
            dfs(s)
    order.reverse()
    layer = [0] * n
    for u in order:
        for v in dag[u]:
            layer[v] = max(layer[v], layer[u] + 1)
    return layer


def _flatten(sub: _Sub, x: int, y: int, depth: int, parent: int | None, out: list[PlacedBox]) -> None:
    idx = len(out)
    out.append(PlacedBox(sub.node, Bounds(x, y, sub.w, sub.h), depth, sub.expanded, parent))
    for c, cx, cy in sub.children:
        _flatten(c, x + cx, y + cy, depth + 1, idx, out)


def _sibling_edges(g: Graph, enabled: Collection[str]) -> dict[NodeId | None, set[tuple[NodeId, NodeId]]]:
    """Projects every enabled link onto the pair of siblings directly below
    the endpoints' lowest common ancestor."""
    out: dict[NodeId | None, set[tuple[NodeId, NodeId]]] = {}
    for l in g.links:
        if l.type not in enabled:
            continue
        pa, pb = g.path_from_root(l.source), g.path_from_root(l.target)
        common = 0
        while common < len(pa) and common < len(pb) and pa[common] == pb[common]:
            common += 1
        if common == len(pa) or common == len(pb):
            continue  # same item, or one contains the other
        container = pa[common - 1] if common > 0 else None
        out.setdefault(container, set()).add((pa[common], pb[common]))
    return out


def _route_edges(
    g: Graph, enabled: Collection[str], boxes: list[PlacedBox], by_node: dict[NodeId, int]
) -> tuple[list[Edge], int]:
    def rep(n: NodeId) -> NodeId | None:
        cur: NodeId | None = n
        while cur is not None:
            if cur in by_node:
                return cur
            cur = g.parent(cur)
        return None

    agg: dict[tuple[NodeId, NodeId, str], int] = {}
    hidden = 0
    for l in g.links:
        if l.type not in enabled:
            continue
        a, b = rep(l.source), rep(l.target)
        if a is not None and b is not None:
            if a != b and not g.is_ancestor(a, b) and not g.is_ancestor(b, a):
                agg[(a, b, l.type)] = agg.get((a, b, l.type), 0) + 1
        elif a is not None or b is not None:
            hidden += 1
    keys = sorted(agg.items())

    def bx(n: NodeId) -> Bounds:
        return boxes[by_node[n]].bounds

    # Spread ports along the box sides so parallel edges stay distinguishable.
    outgoing: dict[NodeId, list[int]] = {}
    incoming: dict[NodeId, list[int]] = {}
    for i, ((a, b, _), _) in enumerate(keys):
        outgoing.setdefault(a, []).append(i)
        incoming.setdefault(b, []).append(i)
    from_y = [0] * len(keys)
    to_y = [0] * len(keys)
    for n, lst in outgoing.items():
        lst.sort(key=lambda i: (bx(keys[i][0][1]).center[1], i))
        _assign_ports(bx(n), lst, from_y)
    for n, lst in incoming.items():
        lst.sort(key=lambda i: (bx(keys[i][0][0]).center[1], i))
        _assign_ports(bx(n), lst, to_y)

    # Vertical "lanes": sources sharing a right edge get distinct channels.
    lanes: dict[int, list[NodeId]] = {}
    for (a, _, _), _ in keys:
        lane = lanes.setdefault(bx(a).right, [])
        if a not in lane:
            lane.append(a)

    router = _Router(boxes)
    edges = []
    for i, ((a, b, kind), count) in enumerate(keys):
        ab, bb = bx(a), bx(b)
        s = (ab.right, from_y[i])
        e = (bb.x - 1, to_y[i])
        lane = lanes[s[0]]
        li = lane.index(a)
        preferred = s[0] + tdiv((e[0] - s[0]) * (li + 1), len(lane) + 1)
        excluded = [
            (o.bounds, CROSS_CONTAINER if o.expanded else CROSS_BOX)
            for o in boxes
            if g.is_ancestor(o.node, a) or g.is_ancestor(o.node, b)
        ]
        points = router.route(s, e, preferred, excluded)
        edges.append(Edge(a, b, kind, count, _simplify(points), (ab.right - 1, s[1]), (bb.x, e[1])))
    return edges, hidden


def _assign_ports(b: Bounds, lst: list[int], ys: list[int]) -> None:
    rows = max(b.h - 2, 1)
    k = len(lst)
    for j, i in enumerate(lst):
        ys[i] = b.y + 1 + (rows - k) // 2 + j if k <= rows else b.y + 1 + j * rows // k


def _overlap(x0: int, y0: int, x1: int, y1: int, b: Bounds) -> int:
    """Cells of the inclusive rectangle (x0,y0)-(x1,y1) inside ``b``."""
    w = min(x1 + 1, b.right) - max(x0, b.x)
    h = min(y1 + 1, b.bottom) - max(y0, b.y)
    return w * h if w > 0 and h > 0 else 0


class _Router:
    """Greedy orthogonal router.

    For each edge it scores a family of 3-segment (one vertical channel) and
    5-segment (detour along a free row) routes and keeps the cheapest. Costs
    are: length + per-cell penalties for crossing unrelated boxes + per-cell
    penalties for overlapping earlier wires + bends. Box and wire costs are
    kept as per-row / per-column prefix sums so each segment is O(1); the
    boxes that contain an edge's endpoints are subtracted analytically.
    """

    def __init__(self, boxes: list[PlacedBox]) -> None:
        self.boxes = boxes
        self.min_y = (min((b.bounds.y for b in boxes), default=0)) - 1
        self.max_y = max((b.bounds.bottom for b in boxes), default=0)
        self.gx0 = min((b.bounds.x for b in boxes), default=0) - 8
        self.gx1 = max((b.bounds.right for b in boxes), default=0) + 8
        width = self.gx1 - self.gx0 + 1
        height = self.max_y - self.min_y + 1
        base = [[0] * width for _ in range(height)]
        for b in boxes:
            w = CROSS_CONTAINER if b.expanded else CROSS_BOX
            r = b.bounds
            xa, xb = r.x - self.gx0, r.right - self.gx0
            for y in range(max(r.y, self.min_y), min(r.bottom, self.max_y + 1)):
                row = base[y - self.min_y]
                row[xa:xb] = [v + w for v in row[xa:xb]]
        self.used: set[Point] = set()
        self.base_rows = [[0, *accumulate(row)] for row in base]
        self.base_cols = [[0, *accumulate(base[y][x] for y in range(height))] for x in range(width)]
        self.used_grid = [[0] * width for _ in range(height)]
        self.used_rows = [[0] * (width + 1) for _ in range(height)]
        self.used_cols = [[0] * (height + 1) for _ in range(width)]

    def _inside(self, x: int, y: int) -> bool:
        return self.gx0 <= x <= self.gx1 and self.min_y <= y <= self.max_y

    def seg_cost(self, p: Point, q: Point, excluded: list[tuple[Bounds, int]]) -> int:
        x0, x1 = min(p[0], q[0]), max(p[0], q[0])
        y0, y1 = min(p[1], q[1]), max(p[1], q[1])
        cost = (x1 - x0) + (y1 - y0)
        if self._inside(x0, y0) and self._inside(x1, y1):
            if y0 == y1:
                ry = y0 - self.min_y
                a, b = x0 - self.gx0, x1 - self.gx0 + 1
                cost += self.base_rows[ry][b] - self.base_rows[ry][a]
                cost += self.used_rows[ry][b] - self.used_rows[ry][a]
            else:
                cx = x0 - self.gx0
                a, b = y0 - self.min_y, y1 - self.min_y + 1
                cost += self.base_cols[cx][b] - self.base_cols[cx][a]
                cost += self.used_cols[cx][b] - self.used_cols[cx][a]
        else:  # outside the precomputed grid: slow but exact
            for o in self.boxes:
                cost += _overlap(x0, y0, x1, y1, o.bounds) * (CROSS_CONTAINER if o.expanded else CROSS_BOX)
            cost += OVERLAP * sum(
                1 for x in range(x0, x1 + 1) for y in range(y0, y1 + 1) if (x, y) in self.used
            )
        for b, w in excluded:
            cost -= _overlap(x0, y0, x1, y1, b) * w
        return cost

    def path_cost(self, pts: list[Point], excluded: list[tuple[Bounds, int]]) -> int:
        segs = sum(self.seg_cost(pts[i], pts[i + 1], excluded) for i in range(len(pts) - 1))
        return segs + BEND * (len(pts) - 2)

    def route(self, s: Point, e: Point, preferred_mx: int, excluded: list[tuple[Bounds, int]]) -> list[Point]:
        best_cost: int | None = None
        best: list[Point] = [s, e]
        if e[0] >= s[0]:
            for mx in range(s[0], e[0] + 1):
                pts = [s, (mx, s[1]), (mx, e[1]), e]
                # x8 so the preferred-lane tie-breaker never outweighs a real cost
                cost = self.path_cost(pts, excluded) * 8 + abs(mx - preferred_mx)
                if best_cost is None or cost < best_cost:
                    best_cost, best = cost, pts
        for dx1 in (1, 2, 3):
            for dx2 in (1, 2, 3):
                x1, x2 = s[0] + dx1, e[0] - dx2
                for yf in range(self.min_y, self.max_y + 1):
                    pts = [s, (x1, s[1]), (x1, yf), (x2, yf), (x2, e[1]), e]
                    cost = (self.path_cost(pts, excluded) + DETOUR) * 8
                    if best_cost is None or cost < best_cost:
                        best_cost, best = cost, pts
        pts = _simplify(best)
        self._mark(pts)
        return pts

    def _mark(self, pts: list[Point]) -> None:
        rows: set[int] = set()
        cols: set[int] = set()
        for (px, py), (qx, qy) in zip(pts, pts[1:]):
            for x in range(min(px, qx), max(px, qx) + 1):
                for y in range(min(py, qy), max(py, qy) + 1):
                    if (x, y) in self.used:
                        continue
                    self.used.add((x, y))
                    if self._inside(x, y):
                        self.used_grid[y - self.min_y][x - self.gx0] = OVERLAP
                        rows.add(y - self.min_y)
                        cols.add(x - self.gx0)
        for ry in rows:
            self.used_rows[ry] = [0, *accumulate(self.used_grid[ry])]
        for cx in cols:
            self.used_cols[cx] = [0, *accumulate(row[cx] for row in self.used_grid)]


def _simplify(points: list[Point]) -> list[Point]:
    """Drops repeated and collinear points."""
    out: list[Point] = []
    for p in points:
        if out and out[-1] == p:
            continue
        if len(out) >= 2:
            a, b = out[-2], out[-1]
            if (a[0] == b[0] == p[0]) or (a[1] == b[1] == p[1]):
                out.pop()
        out.append(p)
    return out
