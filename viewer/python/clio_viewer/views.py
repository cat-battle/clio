"""Text builders for the status bar and details panel, plus the headless
``--snapshot`` renderer. Shared by the Textual app and the CLI."""

from __future__ import annotations

from rich.text import Text

from .render import link_color, render_canvas, role_color, status_color
from .state import ViewerState
from .trace import Role, Trace, TraceMode

HELP: list[tuple[str, str]] = [
    ("arrows / hjkl", "move selection (→/↓ on open box: enter)"),
    ("Shift+arrows / HJKL", "pan the camera"),
    ("+ / -", "zoom in / out (detail level)"),
    ("space", "expand / collapse box in place"),
    ("enter", "drill into box"),
    ("esc / backspace", "drill out"),
    ("e / E", "expand all / collapse all"),
    ("p", "select parent box"),
    ("c", "center on selection"),
    ("r", "relationship filters"),
    ("d", "details panel"),
    ("t", "trace deps: both / upstream / downstream / off"),
    ("T", "expand to show the whole trace"),
    ("/ , n", "search id/title, next match"),
    ("R", "reload data source"),
    ("q", "quit"),
]


def status_text(state: ViewerState, trace: Trace | None = None) -> Text:
    g = state.graph
    t = Text()
    t.append(" Top", style="bold")
    for n in state.drill:
        t.append(" › ", style="grey50")
        t.append(g.item(n).id, style="bold")
    t.append(f"  │ zoom:{state.zoom.label} │", style="grey50")
    for lt in g.link_types:
        if lt in state.enabled:
            t.append(f" ■ {lt}", style=link_color(g.link_types, lt))
        else:
            t.append(f" □ {lt}", style="grey50")
    if state.layout.hidden_links:
        t.append(f"  ({state.layout.hidden_links} links leave this view)", style="grey50")
    if trace is not None:
        t.append(f"  │ trace:{state.trace_mode.label}", style="grey50")
        if state.trace_mode is not TraceMode.DOWNSTREAM:
            t.append(f" ↑{len(Trace.outermost(g, trace.upstream))}", style=role_color(Role.UPSTREAM))
        if state.trace_mode is not TraceMode.UPSTREAM:
            t.append(f" ↓{len(Trace.outermost(g, trace.downstream))}", style=role_color(Role.DOWNSTREAM))
        hidden = len(trace.hidden(g, state.layout))
        if hidden:
            t.append(f" ({hidden} hidden, T shows)", style="grey50")
    if state.message:
        t.append(f"  │ {state.message}", style="yellow")
    else:
        t.append("  │ ?: help", style="grey50")
    return t


def details_text(state: ViewerState) -> Text:
    n = state.selected
    if n is None:
        return Text("nothing selected")
    g = state.graph
    it = g.item(n)
    t = Text()
    t.append(it.id, style="bold")
    t.append(f" · {it.kind.label}\n", style="grey50")
    t.append(f"{it.title}\n\n")
    t.append("Status   ", style="grey50")
    t.append(f"{it.status.glyph} {it.status.label}\n", style=status_color(it.status))
    t.append("Assignee ", style="grey50")
    t.append(f"{it.assignee or '—'}\n")
    t.append("Path     ", style="grey50")
    t.append(" › ".join(g.item(p).id for p in g.path_from_root(n)) + "\n")
    t.append("Children ", style="grey50")
    t.append(f"{len(g.children(n))}\n")
    if it.description:
        t.append(f"\n{it.description}\n")
    for title, links, arrow in (
        ("Links out", [l for l in g.links if l.source == n], "→"),
        ("Links in", [l for l in g.links if l.target == n], "←"),
    ):
        if not links:
            continue
        t.append(f"\n{title}\n", style="bold")
        for l in links:
            other = g.item(l.target if l.source == n else l.source)
            t.append(f" {l.type} {arrow} ", style=link_color(g.link_types, l.type))
            t.append(f"{other.id} {other.title}\n")
    return t


def snapshot(state: ViewerState, width: int, height: int, keys: list[str]) -> str:
    """Renders the canvas + status bar at ``width`` x ``height`` after
    replaying normal-mode ``keys``; same frame shape as the Rust
    ``--snapshot`` (details panel and popups are interactive-only)."""
    return snapshot_styles(state, width, height, keys)[0]


def snapshot_styles(state: ViewerState, width: int, height: int, keys: list[str]) -> tuple[str, str]:
    """Like :func:`snapshot`, plus the canvas style dump of Rust's
    ``--snapshot --styles``."""
    canvas_h = max(height - 1, 0)
    state.set_viewport(width, canvas_h)
    for k in keys:
        if not state.handle_key(k):
            raise ValueError(f"key {k!r} is not available in snapshot mode")
        state.set_viewport(width, canvas_h)
    trace = state.trace()
    grid = render_canvas(
        state.graph, state.layout, state.camera, state.selected, state.zoom, width, canvas_h, trace
    )
    lines = grid.text_lines()
    status = status_text(state, trace).plain[:width]
    lines.append(status + " " * (width - len(status)))
    return "\n".join(lines) + "\n", "".join(l + "\n" for l in grid.style_lines())
