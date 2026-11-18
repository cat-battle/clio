"""Textual front end: a schematic canvas widget, status bar, details panel
and modal screens for help, relationship filters and search."""

from __future__ import annotations

from rich.segment import Segment
from rich.style import Style
from rich.text import Text
from textual import events
from textual.app import App, ComposeResult
from textual.containers import Vertical
from textual.screen import ModalScreen
from textual.strip import Strip
from textual.widget import Widget
from textual.widgets import Input, SelectionList, Static
from textual.widgets.selection_list import Selection

from .render import CellStyle, Grid, link_color, render_canvas
from .state import ViewerState
from .views import HELP, details_text, status_text


class Canvas(Widget):
    """Renders the current layout through the camera, one Strip per row."""

    DEFAULT_CSS = "Canvas { width: 1fr; height: 1fr; }"

    def __init__(self, state: ViewerState, **kwargs) -> None:
        super().__init__(**kwargs)
        self.state = state
        self._grid: Grid | None = None
        self._styles: dict[CellStyle, Style] = {}

    def invalidate(self) -> None:
        self._grid = None
        self.refresh()

    def grid(self) -> Grid:
        w, h = self.size.width, self.size.height
        if self._grid is None or (self._grid.width, self._grid.height) != (w, h):
            s = self.state
            s.set_viewport(w, h)
            self._grid = render_canvas(s.graph, s.layout, s.camera, s.selected, s.zoom, w, h, s.trace())
        return self._grid

    def text(self) -> str:
        """Plain-text contents (used by tests)."""
        return "\n".join(self.grid().text_lines())

    def _rich(self, cs: CellStyle) -> Style:
        st = self._styles.get(cs)
        if st is None:
            st = self._styles[cs] = Style(color=cs.fg, bgcolor=cs.bg, bold=cs.bold or None)
        return st

    def render_line(self, y: int) -> Strip:
        g = self.grid()
        if y >= g.height:
            return Strip.blank(self.size.width)
        chars, styles = g.chars[y], g.styles[y]
        segments: list[Segment] = []
        run_start = 0
        for x in range(1, g.width + 1):
            if x == g.width or styles[x] != styles[run_start]:
                segments.append(Segment("".join(chars[run_start:x]), self._rich(styles[run_start])))
                run_start = x
        return Strip(segments, g.width)


class HelpScreen(ModalScreen[None]):
    DEFAULT_CSS = """
    HelpScreen { align: center middle; }
    #help { width: 64; height: auto; border: round $accent; padding: 0 1; background: $surface; }
    """

    def compose(self) -> ComposeResult:
        t = Text()
        for k, v in HELP:
            t.append(f"{k:<22}", style="yellow")
            t.append(f"{v}\n")
        t.append("\nany key closes", style="grey50")
        yield Static(t, id="help")

    def on_mount(self) -> None:
        self.query_one("#help").border_title = "Keys"

    def on_key(self, event: events.Key) -> None:
        event.stop()
        self.dismiss()


class RelationsScreen(ModalScreen[None]):
    """Checklist of the link types in the data; changes apply live."""

    DEFAULT_CSS = """
    RelationsScreen { align: center middle; }
    #relations-box { width: 50; height: auto; border: round $accent; background: $surface; }
    #relations-box SelectionList { height: auto; max-height: 20; border: none; }
    #relations-hint { color: $text-muted; padding: 0 1; }
    """
    BINDINGS = [
        ("escape", "close", "Close"),
        ("r", "close", "Close"),
        ("q", "close", "Close"),
        ("a", "all", "All"),
        ("n", "none", "None"),
    ]

    def __init__(self, state: ViewerState) -> None:
        super().__init__()
        self.state = state

    def compose(self) -> ComposeResult:
        g = self.state.graph
        options = []
        for t in g.link_types:
            count = sum(1 for l in g.links if l.type == t)
            label = Text.assemble((f"{t:<20}", link_color(g.link_types, t)), f"{count:>4}")
            options.append(Selection(label, t, t in self.state.enabled))
        with Vertical(id="relations-box"):
            if options:
                yield SelectionList[str](*options)
            else:
                yield Static("(no relationships in this data)")
            yield Static("space toggle · a all · n none · esc close", id="relations-hint")

    def on_mount(self) -> None:
        self.query_one("#relations-box").border_title = "Relationships"

    def on_selection_list_selected_changed(self, event: SelectionList.SelectedChanged) -> None:
        self.state.set_enabled(set(event.selection_list.selected))
        self.app.refresh_view()  # type: ignore[attr-defined]

    def action_all(self) -> None:
        for sl in self.query(SelectionList):
            sl.select_all()

    def action_none(self) -> None:
        for sl in self.query(SelectionList):
            sl.deselect_all()

    def action_close(self) -> None:
        self.dismiss()


class SearchScreen(ModalScreen[str | None]):
    DEFAULT_CSS = """
    SearchScreen { align: center bottom; background: $background 0%; }
    SearchScreen Input { dock: bottom; width: 100%; }
    """
    BINDINGS = [("escape", "cancel", "Cancel")]

    def compose(self) -> ComposeResult:
        yield Input(placeholder="search id or title — enter: find, esc: cancel")

    def on_input_submitted(self, event: Input.Submitted) -> None:
        self.dismiss(event.value)

    def action_cancel(self) -> None:
        self.dismiss(None)


class ClioViewer(App[None]):
    TITLE = "Clio viewer"
    ENABLE_COMMAND_PALETTE = False
    CSS = """
    #main { height: 1fr; layout: horizontal; }
    #details { width: 42; height: 1fr; border: round $panel; padding: 0 1; overflow-y: auto; }
    #status { height: 1; dock: bottom; }
    """

    def __init__(self, state: ViewerState) -> None:
        super().__init__()
        self.state = state

    def compose(self) -> ComposeResult:
        with Widget(id="main"):
            yield Canvas(self.state, id="canvas")
            yield Static(id="details")
        yield Static(id="status")

    def on_mount(self) -> None:
        self.query_one("#details").border_title = "Details"
        self.refresh_view()

    @property
    def canvas(self) -> Canvas:
        return self.query_one(Canvas)

    def refresh_view(self) -> None:
        details = self.query_one("#details", Static)
        details.display = self.state.show_details
        if self.state.show_details:
            details.update(details_text(self.state))
        self.query_one("#status", Static).update(status_text(self.state, self.state.trace()))
        self.canvas.invalidate()

    def on_key(self, event: events.Key) -> None:
        if self.screen is not self.screen_stack[0]:
            return  # a modal screen is active
        key = event.character if event.character and len(event.character) == 1 and event.character.isprintable() else event.key
        # Screens are pushed after this key is fully handled, so the same
        # keypress can't also trigger a binding (e.g. "r" = close) on them.
        if key == "?":
            self.call_later(self.push_screen, HelpScreen())
        elif key == "r":
            self.call_later(self.push_screen, RelationsScreen(self.state), lambda _: self.refresh_view())
        elif key == "/":
            self.call_later(self.push_screen, SearchScreen(), self._on_search)
        elif self.state.handle_key(key):
            if self.state.should_quit:
                self.exit()
                return
        else:
            return
        event.stop()
        self.refresh_view()

    def _on_search(self, query: str | None) -> None:
        if query:
            self.state.search(query)
        self.refresh_view()
