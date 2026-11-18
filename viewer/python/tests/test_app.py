"""Drives the real Textual app headlessly with Pilot."""

import asyncio

from textual.widgets import Input, SelectionList, Static

from clio_viewer.app import ClioViewer, HelpScreen, RelationsScreen
from clio_viewer.model import Graph
from clio_viewer.state import ViewerState

from .conftest import sample_doc

SIZE = (160, 40)


def run(scenario):
    async def main():
        app = ClioViewer(ViewerState(Graph(sample_doc())))
        async with app.run_test(size=SIZE) as pilot:
            await pilot.pause()
            await scenario(app, pilot)

    asyncio.run(main())


def sel(app):
    return app.state.graph.item(app.state.selected).id


def status(app):
    return str(app.query_one("#status", Static).render())


def test_initial_render_and_inline_expand():
    async def scenario(app, pilot):
        text = app.canvas.text()
        assert "M1 · milestone" in text and "M2 · milestone" in text
        assert "E1" not in text
        assert "zoom:detail" in status(app)
        await pilot.press("space")
        await pilot.pause()
        text = app.canvas.text()
        assert "M1 Beta launch" in text and "E1 · epic" in text and "M2 · milestone" in text

    run(scenario)


def test_drill_in_out_and_breadcrumb():
    async def scenario(app, pilot):
        await pilot.press("enter")
        await pilot.pause()
        assert "Top › M1" in status(app)
        assert "M2 · milestone" not in app.canvas.text()
        assert sel(app) == "E1"
        await pilot.press("escape")
        await pilot.pause()
        assert sel(app) == "M1" and not app.state.drill

    run(scenario)


def test_navigation_zoom_and_pan_keys():
    async def scenario(app, pilot):
        await pilot.press("l")
        assert sel(app) == "M2"
        await pilot.press("left", "minus")
        await pilot.pause()
        assert sel(app) == "M1" and "zoom:compact" in status(app)
        await pilot.press("plus")
        assert app.state.zoom.label == "detail"
        x = app.state.camera[0]
        await pilot.press("L")
        assert app.state.camera[0] == x + 8

    run(scenario)


def test_relationship_screen_toggles_live():
    async def scenario(app, pilot):
        before = len(app.state.layout.edges)
        await pilot.press("r")
        await pilot.pause()
        assert isinstance(app.screen, RelationsScreen)
        sl = app.screen.query_one(SelectionList)
        assert set(sl.selected) == {"blocks", "duplicates", "relates_to"}
        await pilot.press("space")  # first option: blocks
        await pilot.pause()
        assert "blocks" not in app.state.enabled
        assert len(app.state.layout.edges) == before - 1
        assert "□ blocks" in status(app)
        await pilot.press("n")
        await pilot.pause()
        assert not app.state.layout.edges
        await pilot.press("a")
        await pilot.pause()
        assert len(app.state.layout.edges) == before
        await pilot.press("escape")
        await pilot.pause()
        assert not isinstance(app.screen, RelationsScreen)

    run(scenario)


def test_search_screen():
    async def scenario(app, pilot):
        await pilot.press("slash")
        await pilot.pause()
        assert isinstance(app.focused, Input)
        await pilot.press(*"webhook", "enter")
        await pilot.pause()
        assert sel(app) == "T7"
        assert "T7" in app.canvas.text()
        await pilot.press("slash", "escape")
        await pilot.pause()
        assert sel(app) == "T7"

    run(scenario)


def test_details_help_and_quit():
    async def scenario(app, pilot):
        await pilot.press("d")
        await pilot.pause()
        details = app.query_one("#details", Static)
        assert details.display
        assert "Assignee" in str(details.render()) and "dana" in str(details.render())
        await pilot.press("question_mark")
        await pilot.pause()
        assert isinstance(app.screen, HelpScreen)
        await pilot.press("x")
        await pilot.pause()
        assert not isinstance(app.screen, HelpScreen)
        await pilot.press("q")
        await pilot.pause()
        assert app.state.should_quit

    run(scenario)
