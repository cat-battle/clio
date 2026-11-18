"""Draws a :class:`Layout` into a character grid through a camera.

The grid is UI-agnostic: the Textual canvas turns it into Strips and the
``--snapshot`` mode prints it as text. Drawing rules mirror
``viewer/rust/src/render.rs`` cell for cell.
"""

from __future__ import annotations

from dataclasses import dataclass

from .layout import Edge, Layout, PlacedBox, Zoom, header_text
from .model import Graph, Kind, NodeId, Status
from .trace import Role, Trace


@dataclass(frozen=True)
class CellStyle:
    fg: str | None = None
    bg: str | None = None
    bold: bool = False


PLAIN_STYLE = CellStyle()

# (top_left, top_right, bottom_left, bottom_right, vertical, horizontal)
BORDERS = {
    Kind.MILESTONE: ("╔", "╗", "╚", "╝", "║", "═"),
    Kind.EPIC: ("┏", "┓", "┗", "┛", "┃", "━"),
    Kind.STORY: ("╭", "╮", "╰", "╯", "│", "─"),
    Kind.SUBTASK: ("┌", "┐", "└", "┘", "│", "─"),
}

# (horizontal glyph, vertical glyph, colour) per link type index.
LINK_PALETTE = [
    ("─", "│", "red"),
    ("┄", "┆", "cyan"),
    ("╌", "╎", "magenta"),
    ("━", "┃", "blue"),
    ("─", "│", "green"),
    ("┄", "┆", "bright_yellow"),
]

STATUS_COLORS = {
    Status.TODO: "grey70",
    Status.IN_PROGRESS: "yellow",
    Status.DONE: "green",
    Status.BLOCKED: "red",
}


DIM = "grey50"  # anything outside the traced cone

ROLE_COLORS = {
    Role.FOCUS: None,
    Role.UPSTREAM: "bright_cyan",
    Role.DOWNSTREAM: "bright_magenta",
    Role.BOTH: "bright_yellow",
}


def role_color(r: Role) -> str | None:
    return ROLE_COLORS[r]


def link_style(types: list[str], kind: str) -> tuple[str, str, str]:
    i = types.index(kind) if kind in types else 0
    return LINK_PALETTE[i % len(LINK_PALETTE)]


def link_color(types: list[str], kind: str) -> str:
    return link_style(types, kind)[2]


def status_color(s: Status) -> str:
    return STATUS_COLORS[s]


def truncate(s: str, max_len: int) -> str:
    if len(s) <= max_len:
        return s
    if max_len <= 0:
        return ""
    return s[: max_len - 1] + "…"


class Grid:
    """A width x height matrix of (symbol, style) cells."""

    def __init__(self, width: int, height: int) -> None:
        self.width = width
        self.height = height
        self.chars = [[" "] * width for _ in range(height)]
        self.styles = [[PLAIN_STYLE] * width for _ in range(height)]

    def text_lines(self) -> list[str]:
        return ["".join(row) for row in self.chars]

    def style_lines(self) -> list[str]:
        """Run-length ``N*fg/bg/b`` style tokens per row, in the format of
        the Rust ``--styles`` dump (blank cells only report background)."""
        out = []
        for chars, styles in zip(self.chars, self.styles):
            runs: list[list] = []
            for ch, st in zip(chars, styles):
                blank = ch == " "
                tok = f"{'-' if blank else st.fg or '-'}/{st.bg or '-'}/{int(not blank and st.bold)}"
                if runs and runs[-1][0] == tok:
                    runs[-1][1] += 1
                else:
                    runs.append([tok, 1])
            out.append(" ".join(f"{n}*{t}" for t, n in runs))
        return out


class _Pen:
    def __init__(self, grid: Grid, camera: tuple[int, int]) -> None:
        self.grid = grid
        self.cx, self.cy = camera

    def _pos(self, x: int, y: int) -> tuple[int, int] | None:
        sx, sy = x - self.cx, y - self.cy
        if 0 <= sx < self.grid.width and 0 <= sy < self.grid.height:
            return sx, sy
        return None

    def put(self, x: int, y: int, sym: str, style: CellStyle) -> None:
        p = self._pos(x, y)
        if p:
            self.grid.chars[p[1]][p[0]] = sym
            self.grid.styles[p[1]][p[0]] = style

    def put_fg(self, x: int, y: int, sym: str, fg: str) -> None:
        p = self._pos(x, y)
        if p:
            old = self.grid.styles[p[1]][p[0]]
            self.grid.chars[p[1]][p[0]] = sym
            self.grid.styles[p[1]][p[0]] = CellStyle(fg, old.bg, old.bold)

    def text(self, x: int, y: int, s: str, max_len: int, style: CellStyle) -> None:
        for i, ch in enumerate(s[: max(max_len, 0)]):
            self.put(x + i, y, ch, style)


def _dir(a: tuple[int, int], b: tuple[int, int]) -> str:
    if b[0] > a[0]:
        return "R"
    if b[0] < a[0]:
        return "L"
    return "D" if b[1] > a[1] else "U"


_CORNERS = {
    ("R", "D"): "┐", ("U", "L"): "┐",
    ("R", "U"): "┘", ("D", "L"): "┘",
    ("L", "D"): "┌", ("U", "R"): "┌",
    ("L", "U"): "└", ("D", "R"): "└",
}
_ARROWS = {"R": "▶", "L": "◀", "D": "▼", "U": "▲"}


def render_canvas(
    graph: Graph,
    layout: Layout,
    camera: tuple[int, int],
    selected: NodeId | None,
    zoom: Zoom,
    width: int,
    height: int,
    trace: Trace | None = None,
) -> Grid:
    """When ``trace`` is given, boxes and edges outside the cone are dimmed."""
    grid = Grid(width, height)
    pen = _Pen(grid, camera)
    # Containers first, then wires, then leaf boxes on top of any wire that
    # happens to cross them, then the port marks on the borders.
    for b in layout.boxes:
        if b.expanded:
            _draw_box(pen, graph, b, selected, zoom, trace)
    # Traced edges go last so they win where wires overlap.
    lit = [e for e in layout.edges if trace is None or trace.edge_traced(e.source, e.target, e.type)]
    grey = [e for e in layout.edges if trace is not None and not trace.edge_traced(e.source, e.target, e.type)]
    for e in grey + lit:
        _draw_edge(pen, graph, e, _edge_color(graph, e, trace))
    for b in layout.boxes:
        if not b.expanded:
            _draw_box(pen, graph, b, selected, zoom, trace)
    for e in grey + lit:
        color = _edge_color(graph, e, trace)
        pen.put_fg(e.from_port[0], e.from_port[1], "├", color)
        pen.put_fg(e.to_port[0], e.to_port[1], "┤", color)
    return grid


def _edge_color(graph: Graph, e: Edge, trace: Trace | None) -> str:
    if trace is not None and not trace.edge_traced(e.source, e.target, e.type):
        return DIM
    return link_color(graph.link_types, e.type)


def _draw_box(
    pen: _Pen, graph: Graph, b: PlacedBox, selected: NodeId | None, zoom: Zoom, trace: Trace | None
) -> None:
    item = graph.item(b.node)
    tl, tr, bl, br, vert, horiz = BORDERS[item.kind]
    is_sel = selected == b.node
    role = trace.role(b.node) if trace is not None else None
    dimmed = trace is not None and role is None

    def paint(s: CellStyle) -> CellStyle:
        return CellStyle(DIM) if dimmed else s

    rc = role_color(role) if role is not None else None
    bstyle = paint(CellStyle(rc, None, True) if rc else CellStyle(status_color(item.status)))
    if is_sel:
        bstyle = CellStyle("white", "blue", True)
    r = b.bounds
    x0, y0, x1, y1 = r.x, r.y, r.right - 1, r.bottom - 1

    for y in range(y0 + 1, y1):
        for x in range(x0 + 1, x1):
            pen.put(x, y, " ", PLAIN_STYLE)
    for x in range(x0 + 1, x1):
        pen.put(x, y0, horiz, bstyle)
        pen.put(x, y1, horiz, bstyle)
    for y in range(y0 + 1, y1):
        pen.put(x0, y, vert, bstyle)
        pen.put(x1, y, vert, bstyle)
    pen.put(x0, y0, tl, bstyle)
    pen.put(x1, y0, tr, bstyle)
    pen.put(x0, y1, bl, bstyle)
    pen.put(x1, y1, br, bstyle)

    inner = r.w - 2
    bold = paint(CellStyle("white", None, True))
    title_style = bstyle if is_sel else bold
    if b.expanded:
        pen.text(x0 + 2, y0, f" {header_text(graph, b.node, zoom)} ", inner - 5, title_style)
        pen.text(x1 - 3, y0, "[-]", 3, bstyle)
        return

    dim = CellStyle(DIM)
    plain = paint(PLAIN_STYLE)
    if zoom is Zoom.DETAIL:
        ident = truncate(item.id, inner - 1)
        pen.text(x0 + 1, y0 + 1, ident, inner, bold)
        used = len(ident)
        pen.text(x0 + 1 + used, y0 + 1, f" · {item.kind.label}", inner - used, dim)
        pen.text(x0 + 1, y0 + 2, truncate(item.title, inner), inner, plain)
        st = f"{item.status.glyph} {item.status.label}"
        pen.text(x0 + 1, y0 + 3, st, inner, paint(CellStyle(status_color(item.status))))
        if item.assignee:
            used = len(st) + 1
            who = truncate(f"@{item.assignee}", max(inner - used, 0))
            pen.text(x1 - len(who), y0 + 3, who, inner - used, dim)
    elif zoom is Zoom.COMPACT:
        pen.text(x0 + 1, y0 + 1, truncate(f"{item.id} {item.title}", inner), inner, title_style)
    else:
        pen.text(x0 + 1, y0 + 1, truncate(item.id, inner), inner, title_style)

    kids = len(graph.children(b.node))
    if kids:
        tag = "+" if zoom is Zoom.OVERVIEW else f"[+{kids}]"
        pen.text(x1 - len(tag), y1, tag, len(tag), bstyle)


def _draw_edge(pen: _Pen, graph: Graph, e: Edge, color: str) -> None:
    h, v, _ = link_style(graph.link_types, e.type)
    pts = e.points
    if not pts:
        return
    for a, b in zip(pts, pts[1:]):
        if a[1] == b[1]:
            for x in range(min(a[0], b[0]), max(a[0], b[0]) + 1):
                pen.put_fg(x, a[1], h, color)
        else:
            for y in range(min(a[1], b[1]), max(a[1], b[1]) + 1):
                pen.put_fg(a[0], y, v, color)
    for i in range(1, len(pts) - 1):
        d1, d2 = _dir(pts[i - 1], pts[i]), _dir(pts[i], pts[i + 1])
        c = _CORNERS.get((d1, d2), h if d1 in "LR" else v)
        pen.put_fg(pts[i][0], pts[i][1], c, color)
    last = pts[-1]
    arrow = _ARROWS[_dir(pts[-2], last)] if len(pts) >= 2 else "▶"
    pen.put_fg(last[0], last[1], arrow, color)
    if e.count > 1:
        pen.text(pts[0][0], pts[0][1], f"×{e.count}", 4, CellStyle(color, None, True))
