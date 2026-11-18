"""Dependency-cone tracing (RTL "fan-in / fan-out" for tasks); port of
``rust/src/trace.rs``.

The *focus* is the selected item plus its descendants. Following the enabled
link types forwards gives everything **downstream** of it (what depends on
it); following them backwards gives everything **upstream** (what it depends
on). Reaching an item pulls in its descendants too, because a link to an epic
applies to the work inside it, and their links are followed in turn.

The result is mapped onto the current layout the same way edges are lifted:
a member hidden inside a collapsed box marks that box.
"""

from __future__ import annotations

from collections.abc import Collection
from dataclasses import dataclass, field
from enum import Enum

from .layout import Layout
from .model import Graph, NodeId


class TraceMode(Enum):
    OFF = "off"
    BOTH = "both"
    UPSTREAM = "upstream"
    DOWNSTREAM = "downstream"

    @property
    def label(self) -> str:
        return self.value

    def next(self) -> TraceMode:
        order = list(TraceMode)
        return order[(order.index(self) + 1) % len(order)]

    @property
    def up(self) -> bool:
        return self in (TraceMode.BOTH, TraceMode.UPSTREAM)

    @property
    def down(self) -> bool:
        return self in (TraceMode.BOTH, TraceMode.DOWNSTREAM)


class Role(Enum):
    FOCUS = "focus"
    UPSTREAM = "upstream"
    DOWNSTREAM = "downstream"
    BOTH = "both"  # upstream and downstream: the focus sits on a cycle with it


@dataclass
class Trace:
    focus: set[NodeId] = field(default_factory=set)
    upstream: set[NodeId] = field(default_factory=set)
    downstream: set[NodeId] = field(default_factory=set)
    roles: dict[NodeId, Role] = field(default_factory=dict)  # by visible box
    edges: set[tuple[NodeId, NodeId, str]] = field(default_factory=set)  # drawn edges carrying a traced link

    @classmethod
    def compute(
        cls, graph: Graph, layout: Layout, selected: NodeId, mode: TraceMode, enabled: Collection[str]
    ) -> Trace:
        t = cls(focus={selected, *graph.descendants(selected)})
        links = [(l.source, l.target, l.type) for l in graph.links if l.type in enabled]
        traced: list[tuple[NodeId, NodeId, str]] = []
        if mode.down:
            t.downstream = _closure(graph, t.focus, links, reverse=False)
            reach = t.focus | t.downstream
            traced += [l for l in links if l[0] in reach and l[1] in t.downstream]
        if mode.up:
            t.upstream = _closure(graph, t.focus, links, reverse=True)
            reach = t.focus | t.upstream
            traced += [l for l in links if l[1] in reach and l[0] in t.upstream]

        def rep(n: NodeId) -> NodeId | None:
            return visible_rep(graph, layout, n)

        for members, role in ((t.upstream, Role.UPSTREAM), (t.downstream, Role.DOWNSTREAM)):
            for n in members:
                b = rep(n)
                if b is None:
                    continue
                cur = t.roles.setdefault(b, role)
                if cur is not role and cur is not Role.FOCUS:
                    t.roles[b] = Role.BOTH
        for n in t.focus:
            b = rep(n)
            if b is not None:
                t.roles[b] = Role.FOCUS
        for a, b, kind in traced:
            ra, rb = rep(a), rep(b)
            if ra is not None and rb is not None and ra != rb:
                t.edges.add((ra, rb, kind))
        return t

    def role(self, n: NodeId) -> Role | None:
        return self.roles.get(n)

    def edge_traced(self, source: NodeId, target: NodeId, kind: str) -> bool:
        return (source, target, kind) in self.edges

    @staticmethod
    def outermost(graph: Graph, members: set[NodeId]) -> list[NodeId]:
        """Members whose parent is not also a member (an epic pulled in with
        its stories counts once)."""
        return sorted(n for n in members if graph.parent(n) not in members)

    def hidden(self, graph: Graph, layout: Layout) -> list[NodeId]:
        """Outermost upstream/downstream members without a box of their own
        (inside a collapsed box or outside the drilled-in view)."""
        out = set(self.outermost(graph, self.upstream)) | set(self.outermost(graph, self.downstream))
        return sorted(n for n in out if not layout.visible(n))


def _closure(
    graph: Graph, start: set[NodeId], links: list[tuple[NodeId, NodeId, str]], *, reverse: bool
) -> set[NodeId]:
    adj: dict[NodeId, list[NodeId]] = {}
    for a, b, _ in links:
        s, d = (b, a) if reverse else (a, b)
        adj.setdefault(s, []).append(d)
    out: set[NodeId] = set()
    stack = list(start)
    while stack:
        u = stack.pop()
        for v in adj.get(u, ()):
            if v in start or v in out:
                continue
            for w in (v, *graph.descendants(v)):
                if w not in start and w not in out:
                    out.add(w)
                    stack.append(w)
    return out


def visible_rep(graph: Graph, layout: Layout, n: NodeId) -> NodeId | None:
    """The box an item is drawn as: itself if visible, else its nearest
    visible ancestor (None when outside the current view)."""
    if layout.visible(n):
        return n
    return next((a for a in graph.ancestors(n) if layout.visible(a)), None)
