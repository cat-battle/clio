"""Colour themes (mirrors rust/tests/theme.rs)."""

import asyncio

import pytest

from clio_viewer.__main__ import main
from clio_viewer.app import ClioViewer
from clio_viewer.model import Graph, Status
from clio_viewer.state import ViewerState
from clio_viewer.theme import CLASSIC, MOCHA, resolve
from clio_viewer.views import snapshot_styles

from .conftest import FIXTURES, sample_doc


def styles(theme, keys=()):
    s = ViewerState(Graph(sample_doc()))
    s.theme = theme
    return snapshot_styles(s, 160, 40, list(keys))


def test_resolve_prefers_flag_then_env_then_truecolor_detection():
    assert resolve("classic", "mocha", "truecolor") is CLASSIC
    assert resolve(None, "Classic", "truecolor") is CLASSIC
    assert resolve(None, "", "24bit") is MOCHA, "empty env is ignored"
    assert resolve(None, None, "truecolor") is MOCHA
    assert resolve(None, None, None) is CLASSIC, "no true colour: 16-colour theme"
    with pytest.raises(ValueError, match="mocha, classic"):
        resolve("latte", None, None)


def test_mocha_is_the_clik_catppuccin_palette():
    assert MOCHA.status_color(Status.BLOCKED) == "#f38ba8"
    assert MOCHA.status_color(Status.DONE) == "#a6e3a1"
    _, st = styles(MOCHA)
    assert "#11111b/#89b4fa/1" in st, "selected box: crust on blue"
    assert "#f38ba8/#181825/0" in st, "blocks wires in red, on the mocha background"
    assert "/-/" not in st, "every canvas cell has a background"
    assert "grey" not in st and "/blue/" not in st, "no ANSI colours left"


def test_classic_keeps_the_ansi_look():
    _, st = styles(CLASSIC)
    assert "white/blue/1" in st and "red/-/0" in st and "#" not in st


def test_mocha_paints_the_background_and_fills_cards():
    _, st = styles(MOCHA, [" "])
    assert "-/#181825/0" in st, "canvas background (mantle)"
    assert "-/#313244/0" in st, "closed boxes are filled (surface0)"
    _, classic = styles(CLASSIC, [" "])
    assert "#" not in classic, "classic leaves the terminal background alone"


def test_trace_dims_with_the_theme_faint_colour():
    _, st = styles(MOCHA, [" ", "t"])
    assert "#585b70" in st and "#f5c2e7" in st


def test_shift_c_cycles_themes():
    s = ViewerState(Graph(sample_doc()))
    assert s.theme is MOCHA
    text, _ = snapshot_styles(s, 160, 40, ["C"])
    assert s.theme is CLASSIC and "theme: classic" in text
    snapshot_styles(s, 160, 40, ["C"])
    assert s.theme is MOCHA


def test_cli_theme_flag(capsys, monkeypatch):
    monkeypatch.delenv("CLIO_THEME", raising=False)
    monkeypatch.setenv("COLORTERM", "truecolor")
    args = ["--file", str(FIXTURES / "sample.json"), "--snapshot", "100x20", "--styles"]
    assert main(args) == 0
    assert "#89b4fa" in capsys.readouterr().out
    assert main([*args, "--theme", "classic"]) == 0
    assert "#" not in capsys.readouterr().out.split("--- styles ---")[1]
    monkeypatch.setenv("CLIO_THEME", "nope")
    assert main(args) == 1
    assert "unknown theme 'nope'" in capsys.readouterr().err


def test_theme_switch_in_textual_app():
    async def run():
        app = ClioViewer(ViewerState(Graph(sample_doc())))
        async with app.run_test(size=(160, 40)) as pilot:
            assert app.theme == "catppuccin-mocha", "Textual's own widgets follow the palette"
            assert app.query_one("#status").styles.background.hex == "#313244"
            await pilot.press("C")
            await pilot.pause()
            assert app.state.theme is CLASSIC and app.theme == "textual-dark"
            assert "theme: classic" in str(app.query_one("#status").render())
            await pilot.press("question_mark")
            await pilot.pause()
            assert "colour theme" in str(app.screen.query_one("#help").render())

    asyncio.run(run())
