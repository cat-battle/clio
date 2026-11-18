"""Colour themes; port of ``rust/src/theme.rs``. Every colour the viewer
draws comes from a :class:`Theme`, so a new palette is one more constant.

* **mocha** (default): the Catppuccin Mocha true-colour palette, the same one
  the clik project uses.
* **classic**: the 16 standard ANSI colours, for terminals without true colour.
"""

from __future__ import annotations

from dataclasses import dataclass

from .model import Status
from .trace import Role

LinkStyle = tuple[str, str, str]  # (horizontal glyph, vertical glyph, colour)


@dataclass(frozen=True)
class Theme:
    name: str
    text: str  # ids and container titles (bold)
    body: str | None  # box titles (None = terminal default)
    subtle: str  # kinds, assignees, descriptions, status-bar separators
    faint: str  # everything outside a dependency trace
    accent: str  # messages, the search prompt, key names in help
    status: tuple[str, str, str, str]  # todo, in progress, done, blocked
    links: tuple[LinkStyle, ...]  # relationship types in sorted order; repeats
    upstream: str
    downstream: str
    both: str
    selected_fg: str
    selected_bg: str
    background: str | None  # canvas and panel background (None = the terminal's)
    card: str | None  # fill of collapsed boxes
    bar: str | None  # status bar background
    textual: str  # Textual theme for the app's own widgets (popups, details, scrollbars)

    def status_color(self, s: Status) -> str:
        return self.status[[Status.TODO, Status.IN_PROGRESS, Status.DONE, Status.BLOCKED].index(s)]

    def link(self, types: list[str], kind: str) -> LinkStyle:
        """Style of ``kind``, by its index in the graph's sorted link types."""
        i = types.index(kind) if kind in types else 0
        return self.links[i % len(self.links)]

    def link_color(self, types: list[str], kind: str) -> str:
        return self.link(types, kind)[2]

    def role(self, r: Role) -> str | None:
        """Border colour of a traced box (None for the focus: keeps its own)."""
        return {Role.FOCUS: None, Role.UPSTREAM: self.upstream, Role.DOWNSTREAM: self.downstream, Role.BOTH: self.both}[r]

    def next(self) -> Theme:
        """The theme after this one (for the ``C`` key)."""
        return THEMES[(THEMES.index(self) + 1) % len(THEMES)]


# Catppuccin Mocha (https://catppuccin.com/palette), as in clik's theme.py.
MOCHA = Theme(
    name="mocha",
    text="#cdd6f4",  # text
    body="#bac2de",  # subtext1
    subtle="#7f849c",  # overlay1
    faint="#585b70",  # surface2
    accent="#f9e2af",  # yellow
    status=("#9399b2", "#f9e2af", "#a6e3a1", "#f38ba8"),  # overlay2, yellow, green, red
    links=(
        ("─", "│", "#f38ba8"),  # red
        ("┄", "┆", "#89dceb"),  # sky
        ("╌", "╎", "#cba6f7"),  # mauve
        ("━", "┃", "#89b4fa"),  # blue
        ("─", "│", "#a6e3a1"),  # green
        ("┄", "┆", "#fab387"),  # peach
    ),
    upstream="#94e2d5",  # teal
    downstream="#f5c2e7",  # pink
    both="#fab387",  # peach
    selected_fg="#11111b",  # crust
    selected_bg="#89b4fa",  # blue
    # Same as Textual's built-in catppuccin-mocha theme, which the app switches to.
    background="#181825",  # mantle
    card="#313244",  # surface0
    bar="#313244",  # surface0
    textual="catppuccin-mocha",
)

# The original 16-colour look.
CLASSIC = Theme(
    name="classic",
    text="white",
    body=None,
    subtle="grey50",
    faint="grey50",
    accent="yellow",
    status=("grey70", "yellow", "green", "red"),
    links=(
        ("─", "│", "red"),
        ("┄", "┆", "cyan"),
        ("╌", "╎", "magenta"),
        ("━", "┃", "blue"),
        ("─", "│", "green"),
        ("┄", "┆", "bright_yellow"),
    ),
    upstream="bright_cyan",
    downstream="bright_magenta",
    both="bright_yellow",
    selected_fg="white",
    selected_bg="blue",
    background=None,
    card=None,
    bar=None,
    textual="textual-dark",
)

THEMES: list[Theme] = [MOCHA, CLASSIC]


def by_name(name: str) -> Theme | None:
    return next((t for t in THEMES if t.name == name.lower()), None)


def resolve(flag: str | None, env_theme: str | None, colorterm: str | None) -> Theme:
    """``--theme`` if given, else ``$CLIO_THEME``, else mocha on true-colour
    terminals (``$COLORTERM`` = truecolor / 24bit) and classic elsewhere."""
    name = flag or env_theme
    if name:
        theme = by_name(name)
        if theme is None:
            raise ValueError(f"unknown theme {name!r} (choose from: {', '.join(t.name for t in THEMES)})")
        return theme
    return MOCHA if (colorterm or "").lower() in ("truecolor", "24bit") else CLASSIC
