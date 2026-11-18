"""UI-independent viewer state and navigation (port of ``rust/src/app.rs``).

Popups (help, relationship filters, search prompt) are Textual screens in
:mod:`clio_viewer.app`; everything they change goes through methods here.
"""

from __future__ import annotations

from .adapters import Loader
from .layout import Layout, Stretch, Zoom, compute, measure, tdiv
from .model import Graph, NodeId
from .theme import MOCHA, Theme
from .trace import Trace, TraceMode

PAN_X = 8
PAN_Y = 4


class ViewerState:
    def __init__(self, graph: Graph, source: Loader | None = None) -> None:
        self.graph = graph
        self.source = source
        self.expanded: set[NodeId] = set()
        self.drill: list[NodeId] = []  # last entry = current view root
        self.zoom = Zoom.DETAIL
        self.stretch = Stretch()  # growth beyond the zoom's base sizes (auto-fit)
        self.enabled: set[str] = set(graph.link_types)
        self.selected: NodeId | None = None
        self.camera = (0, 0)
        self.viewport = (80, 24)
        self.layout = Layout()
        self.show_details = False
        self.trace_mode = TraceMode.OFF
        self.auto_fit = True  # most detailed zoom at which the whole view fits
        self.theme: Theme = MOCHA
        self.message: str | None = None
        self.last_search: str | None = None
        self.should_quit = False
        self._follow_pending = True
        self._center_pending = False
        self._fit_pending = True
        self.relayout()

    # ---------------------------------------------------------------- layout

    @property
    def root(self) -> NodeId | None:
        return self.drill[-1] if self.drill else None

    def relayout(self) -> None:
        # With auto-fit, the next frame refits (and re-stretches) anyway, so
        # lay out at base sizes: routing a stretched layout is the slow part.
        if self.auto_fit:
            self.stretch = Stretch()
        self.layout = self._layout_at(self.zoom, self.stretch)
        self._fix_selection()
        self._follow_pending = True
        self._fit_pending = True

    def _layout_at(self, zoom: Zoom, stretch: Stretch) -> Layout:
        return compute(self.graph, self.root, self.expanded, zoom, self.enabled, stretch)

    def _measure(self, zoom: Zoom, stretch: Stretch) -> tuple[int, int]:
        return measure(self.graph, self.root, self.expanded, zoom, self.enabled, stretch)

    def _fix_selection(self) -> None:
        """Moves the selection to its nearest visible ancestor (or the first
        box) when it is not visible in the current layout."""
        if self.selected is None or not self.layout.visible(self.selected):
            fallback = None
            if self.selected is not None:
                fallback = next((a for a in self.graph.ancestors(self.selected) if self.layout.visible(a)), None)
            if fallback is None and self.layout.boxes:
                fallback = self.layout.boxes[0].node
            self.selected = fallback

    def _fit(self) -> None:
        """Auto-fit: picks the most detailed zoom level at which the whole
        view fits the viewport (overview if none does), then stretches the
        layout along each axis that fits until it fills the viewport. When it
        fits, ``set_viewport`` centres it."""
        vw, vh = self.viewport
        old = self.zoom
        zero = Stretch()
        zoom = Zoom.OVERVIEW
        for z in (Zoom.DETAIL, Zoom.COMPACT):
            w, h = self._measure(z, zero)
            if w <= vw and h <= vh:
                zoom = z
                break
        w0, h0 = self._measure(zoom, zero)
        sx = _max_level(vw, lambda k: self._measure(zoom, Stretch(k, 0))[0] <= vw) if w0 <= vw else 0
        sy = _max_level(vh, lambda k: self._measure(zoom, Stretch(0, k))[1] <= vh) if h0 <= vh else 0
        # Routed wires can occasionally reach past the boxes; back off until
        # the real layout fits too.
        while True:
            st = Stretch(sx, sy)
            layout = self.layout if (zoom, st) == (self.zoom, self.stretch) else self._layout_at(zoom, st)
            shrink_x = layout.width > vw and sx > 0
            shrink_y = layout.height > vh and sy > 0
            if not shrink_x and not shrink_y:
                break
            sx -= shrink_x
            sy -= shrink_y
        self.zoom, self.stretch, self.layout = zoom, st, layout
        self._fix_selection()
        if self.zoom is not old:
            # Start from the top-left and scroll only as far as the selection needs.
            self.camera = (0, 0)
            self._follow_pending = True

    def set_viewport(self, w: int, h: int) -> None:
        """Called with the canvas size before each frame; applies any pending
        camera adjustment."""
        resized = (w, h) != self.viewport
        self.viewport = (w, h)
        if self.auto_fit and (self._fit_pending or resized):
            self._fit_pending = False
            self._fit()
        if self._center_pending:
            self._center_pending = False
            self._follow_pending = False
            self.center_on_selected()
        elif self._follow_pending:
            self._follow_pending = False
            self._ensure_selected_visible()
        # With auto-fit, an axis on which the whole view fits never scrolls;
        # the view is centred on it.
        if self.auto_fit:
            cx, cy = self.camera
            if self.layout.width <= w:
                cx = -((w - self.layout.width) // 2)
            if self.layout.height <= h:
                cy = -((h - self.layout.height) // 2)
            self.camera = (cx, cy)

    def _clamp_camera(self) -> None:
        vw, vh = self.viewport
        lo_x, lo_y = tdiv(-vw, 2), tdiv(-vh, 2)
        max_x = max(self.layout.width - vw // 2, lo_x)
        max_y = max(self.layout.height - vh // 2, lo_y)
        self.camera = (min(max(self.camera[0], lo_x), max_x), min(max(self.camera[1], lo_y), max_y))

    def center_on_selected(self) -> None:
        b = self.layout.box_of(self.selected) if self.selected is not None else None
        if b:
            cx, cy = b.bounds.center
            self.camera = (cx - self.viewport[0] // 2, cy - self.viewport[1] // 2)
            self._clamp_camera()

    def request_center(self) -> None:
        self._center_pending = True

    def _ensure_selected_visible(self) -> None:
        b = self.layout.box_of(self.selected) if self.selected is not None else None
        if not b:
            return
        r = b.bounds
        vw, vh = self.viewport
        cx, cy = self.camera
        if r.w + 2 > vw or r.x - 1 < cx:
            cx = r.x - 1
        elif r.right + 1 > cx + vw:
            cx = r.right + 1 - vw
        if r.h + 2 > vh or r.y - 1 < cy:
            cy = r.y - 1
        elif r.bottom + 1 > cy + vh:
            cy = r.bottom + 1 - vh
        self.camera = (cx, cy)

    # ------------------------------------------------------------------ keys

    def handle_key(self, key: str) -> bool:
        """Handles a normal-mode key (Textual key names, or the character for
        printable keys). Returns False if the key is not one of ours."""
        self.message = None
        shift_pan = {
            "shift+left": (-PAN_X, 0), "H": (-PAN_X, 0),
            "shift+right": (PAN_X, 0), "L": (PAN_X, 0),
            "shift+up": (0, -PAN_Y), "K": (0, -PAN_Y),
            "shift+down": (0, PAN_Y), "J": (0, PAN_Y),
        }
        moves = {"left": "L", "h": "L", "right": "R", "l": "R", "up": "U", "k": "U", "down": "D", "j": "D"}
        if key in shift_pan:
            self.pan(*shift_pan[key])
        elif key in moves:
            self.move_selection(moves[key])
        elif key in ("+", "="):
            self.manual_zoom(self.zoom.zoom_in())
        elif key in ("-", "_"):
            self.manual_zoom(self.zoom.zoom_out())
        elif key == "z":
            self.toggle_auto_fit()
        elif key == "x":
            self.close_enclosing()
        elif key in (" ", "space"):
            self.toggle_expand()
        elif key == "enter":
            self.drill_in()
        elif key in ("escape", "backspace"):
            self.drill_out()
        elif key == "e":
            self.expand_all()
        elif key == "E":
            self.collapse_all()
        elif key == "p":
            self.select_parent()
        elif key == "c":
            self.request_center()
        elif key == "R":
            self.reload()
        elif key == "C":
            self.theme = self.theme.next()
            self.message = f"theme: {self.theme.name}"
        elif key == "d":
            self.show_details = not self.show_details
        elif key == "t":
            self.trace_mode = self.trace_mode.next()
        elif key == "T":
            self.reveal_trace()
        elif key == "n":
            if self.last_search:
                self.search(self.last_search)
            else:
                self.message = "no previous search"
        elif key == "q":
            self.should_quit = True
        else:
            return False
        return True

    # --------------------------------------------------------------- actions

    def pan(self, dx: int, dy: int) -> None:
        self.camera = (self.camera[0] + dx, self.camera[1] + dy)
        self._clamp_camera()

    def manual_zoom(self, z: Zoom) -> None:
        """``+``/``-``: choosing a zoom level by hand turns auto-fit off."""
        if self.auto_fit:
            self.auto_fit = False
            self.message = "auto-fit off (z turns it back on)"
        if self.stretch != Stretch():
            # Picking a zoom level by hand means its base sizes.
            self.stretch = Stretch()
            self.relayout()
        self.set_zoom(z)

    def toggle_auto_fit(self) -> None:
        self.auto_fit = not self.auto_fit
        self._fit_pending = self.auto_fit
        self.message = f"auto-fit {'on' if self.auto_fit else 'off'}"

    def close_enclosing(self) -> None:
        """``x``: closes the selected box if it is open, otherwise the box
        that contains it (selecting that box), so repeated presses close
        outward. At the top of a drilled-in view it drills out instead."""
        s = self.selected
        if s is None:
            return
        if s in self.expanded:
            self.expanded.discard(s)
            self.relayout()
            return
        p = self.graph.parent(s)
        if p is not None and self.layout.visible(p):
            self.expanded.discard(p)
            self.selected = p
            self.relayout()
        elif p is not None and p == self.root:
            self.drill_out()
        else:
            self.message = "nothing to close"

    def click(self, sx: int, sy: int) -> bool:
        """Mouse click at canvas cell (sx, sy): selects the innermost box
        there. Clicking an open box's top border (its title or ``[-]``)
        closes it; clicking a closed box's ``[+N]`` tag opens it. Returns
        False when nothing was hit."""
        self.message = None
        x, y = sx + self.camera[0], sy + self.camera[1]
        hit = next((b for b in reversed(self.layout.boxes) if b.bounds.contains_point(x, y)), None)
        if hit is None:
            return False
        n, r = hit.node, hit.bounds
        self.selected = n
        kids = len(self.graph.children(n))
        if hit.expanded and y == r.y:
            self.expanded.discard(n)
            self.relayout()
        elif not hit.expanded and kids and y == r.bottom - 1:
            tag = 1 if self.zoom is Zoom.OVERVIEW else len(f"[+{kids}]")
            x1 = r.right - 1
            if x1 - tag <= x < x1:
                self.expanded.add(n)
                self.relayout()
        return True

    def scroll(self, down: bool, sideways: bool = False) -> None:
        """Mouse wheel: pans vertically (sideways with Shift)."""
        step = 1 if down else -1
        self.pan(step * PAN_X, 0) if sideways else self.pan(0, step * PAN_Y)

    def set_zoom(self, z: Zoom) -> None:
        if z is not self.zoom:
            self.zoom = z
            self.relayout()
            self._center_pending = True

    def move_selection(self, d: str) -> None:
        """Moves to the nearest visible box in direction ``d`` (L/R/U/D);
        Right/Down on an open container steps into its first child."""
        sel = self.selected
        if sel is None:
            self.selected = self.layout.boxes[0].node if self.layout.boxes else None
            return
        sb = self.layout.box_of(sel)
        if sb is None:
            return
        if sb.expanded and d in "RD":
            kids = [b for b in self.layout.boxes if self.graph.parent(b.node) == sel]
            if kids:
                first = min(kids, key=lambda b: (b.bounds.x, b.bounds.y))
                self.selected = first.node
                self._follow_pending = True
                return
        sx, sy = sb.bounds.center
        best: tuple[int, NodeId] | None = None
        for b in self.layout.boxes:
            if b.node == sel or self.graph.is_ancestor(b.node, sel):
                continue
            bx, by = b.bounds.center
            dx, dy = bx - sx, (by - sy) * 2  # cells are ~twice as tall as wide
            along, across = {"R": (dx, abs(dy)), "L": (-dx, abs(dy)), "D": (dy, abs(dx)), "U": (-dy, abs(dx))}[d]
            if along <= 0:
                continue
            score = along + 2 * across
            if best is None or score < best[0]:
                best = (score, b.node)
        if best:
            self.selected = best[1]
            self._follow_pending = True

    def select_parent(self) -> None:
        if self.selected is None:
            return
        p = self.graph.parent(self.selected)
        if p is not None and self.layout.visible(p):
            self.selected = p
            self._follow_pending = True

    def toggle_expand(self) -> None:
        s = self.selected
        if s is None:
            return
        if not self.graph.children(s):
            self.message = f"{self.graph.item(s).id} has no children"
            return
        self.expanded ^= {s}
        self.relayout()

    def drill_in(self) -> None:
        s = self.selected
        if s is None:
            return
        if not self.graph.children(s):
            self.message = f"{self.graph.item(s).id} has no children"
            return
        self.drill.append(s)
        self.selected = self.graph.children(s)[0]
        self.camera = (0, 0)
        self.relayout()

    def drill_out(self) -> None:
        if self.drill:
            self.selected = self.drill.pop()
            self.relayout()
            self._center_pending = True

    def _scope(self) -> list[NodeId]:
        return self.graph.descendants(self.root) if self.root is not None else list(range(len(self.graph)))

    def expand_all(self) -> None:
        self.expanded |= {n for n in self._scope() if self.graph.children(n)}
        self.relayout()

    def collapse_all(self) -> None:
        self.expanded -= set(self._scope())
        self.relayout()

    def set_enabled(self, types: set[str]) -> None:
        self.enabled = set(types) & set(self.graph.link_types)
        self.relayout()

    def toggle_link_type(self, t: str) -> None:
        self.set_enabled(self.enabled ^ {t})

    def search(self, query: str) -> None:
        """Selects the next item (after the selection) whose id or title
        contains ``query``, expanding / drilling out to make it visible."""
        self.last_search = query
        q = query.lower()
        n = len(self.graph)
        start = 0 if self.selected is None else self.selected + 1
        for k in range(n):
            i = (start + k) % n
            it = self.graph.item(i)
            if q in it.id.lower() or q in it.title.lower():
                self.reveal(i)
                return
        self.message = f"no match for {query!r}"

    def reveal(self, n: NodeId) -> None:
        while self.drill and not self.graph.is_ancestor(self.drill[-1], n):
            self.drill.pop()
        for a in self.graph.ancestors(n):
            if a == self.root:
                break
            self.expanded.add(a)
        self.selected = n
        self.relayout()
        self._center_pending = True

    def trace(self) -> Trace | None:
        """The dependency cone of the selection, when tracing is on."""
        if self.trace_mode is TraceMode.OFF or self.selected is None:
            return None
        return Trace.compute(self.graph, self.layout, self.selected, self.trace_mode, self.enabled)

    def reveal_trace(self) -> None:
        """Expands (and drills out) just enough to give every traced item its
        own box."""
        if self.trace_mode is TraceMode.OFF:
            self.trace_mode = TraceMode.BOTH
        t = self.trace()
        if t is None:
            return
        hidden = t.hidden(self.graph, self.layout)
        if not hidden:
            self.message = "whole trace already visible"
            return
        g = self.graph
        members = Trace.outermost(g, t.upstream) + Trace.outermost(g, t.downstream)
        if self.selected is not None:
            members.append(self.selected)
        while self.drill and not all(g.is_ancestor(self.drill[-1], n) for n in members):
            self.drill.pop()
        for n in members:
            for a in g.ancestors(n):
                if a == self.root:
                    break
                self.expanded.add(a)
        self.relayout()
        self.message = f"revealed {len(hidden)} traced items"

    def reload(self) -> None:
        """Re-reads the data source, keeping expansion, drill path, selection
        and relationship filters (matched by item id / link type)."""
        if self.source is None:
            self.message = "no source to reload"
            return
        try:
            graph = Graph(self.source.load())
        except ValueError as e:
            self.message = f"reload failed: {e}"
            return
        old = self.graph
        expanded = {old.item(n).id for n in self.expanded}
        drill = [old.item(n).id for n in self.drill]
        selected = old.item(self.selected).id if self.selected is not None else None
        old_types = set(old.link_types)
        self.enabled = {t for t in graph.link_types if t in self.enabled or t not in old_types}
        self.graph = graph
        self.expanded = {n for i in expanded if (n := graph.find(i)) is not None}
        self.drill = []
        for i in drill:
            n = graph.find(i)
            if n is None:
                break
            self.drill.append(n)
        self.selected = graph.find(selected) if selected is not None else None
        self.relayout()
        self.message = f"reloaded {len(graph)} items"


def _max_level(limit: int, fits) -> int:
    """Largest ``k`` in ``0..limit`` with ``fits(k)``, assuming ``fits`` is
    monotone (true up to some point, then false) and ``fits(0)`` holds."""
    lo, hi = 0, max(limit, 0)
    while lo < hi:
        mid = lo + (hi - lo + 1) // 2
        if fits(mid):
            lo = mid
        else:
            hi = mid - 1
    return lo
