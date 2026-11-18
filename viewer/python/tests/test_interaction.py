"""Auto-fit zoom, the ``x`` close key and mouse input (mirrors rust/tests/interaction.rs)."""

import asyncio

from clio_viewer.app import ClioViewer
from clio_viewer.keys import parse_keys
from clio_viewer.layout import Zoom
from clio_viewer.model import Graph
from clio_viewer.state import ViewerState
from clio_viewer.views import snapshot

from .conftest import sample_doc


def new():
    return ViewerState(Graph(sample_doc()))


def screen(s, w, h, keys=""):
    return snapshot(s, w, h, parse_keys(keys))


def sel(s):
    return s.graph.item(s.selected).id


def find(s, i):
    return s.graph.find(i)


def test_auto_fit_picks_most_detailed_zoom_that_fits():
    s = new()
    assert s.auto_fit
    assert "zoom:detail (auto)" in screen(s, 150, 38)
    screen(s, 150, 38, "e")
    assert s.zoom is Zoom.OVERVIEW
    screen(s, 150, 38, "E")
    assert s.zoom is Zoom.DETAIL
    screen(s, 150, 38, "e")
    screen(s, 600, 60)
    assert s.zoom is Zoom.DETAIL, "a bigger terminal refits"
    assert s.camera == (0, 0)


def test_auto_fit_shows_overflowing_views_from_the_top_left():
    s = new()
    screen(s, 150, 38, "e")
    assert s.layout.width > 150 and s.layout.height <= 37
    assert s.camera == (0, 0)


def test_manual_zoom_turns_auto_fit_off_and_z_turns_it_back_on():
    s = new()
    out = screen(s, 150, 38, "-")
    assert not s.auto_fit and s.zoom is Zoom.COMPACT
    assert "zoom:compact │" in out and "auto-fit off (z turns it back on)" in out
    screen(s, 150, 38, "e")
    assert s.zoom is Zoom.COMPACT
    out = screen(s, 150, 38, "z")
    assert s.auto_fit and s.zoom is Zoom.OVERVIEW and "auto-fit on" in out


def test_x_closes_outward_one_level_at_a_time():
    s = new()
    s.reveal(find(s, "T1"))
    for expect in ("S1", "E1", "M1"):
        screen(s, 200, 50, "x")
        assert sel(s) == expect
    assert not s.expanded
    assert "nothing to close" in screen(s, 200, 50, "x")


def test_x_on_an_open_box_closes_it_and_drills_out_at_the_top_of_a_view():
    s = new()
    screen(s, 200, 50, "<space>x")
    assert sel(s) == "M1" and not s.expanded
    screen(s, 200, 50, "<enter>")
    assert sel(s) == "E1" and len(s.drill) == 1
    screen(s, 200, 50, "x")
    assert not s.drill and sel(s) == "M1"


def click(s, x, y):
    s.click(x - s.camera[0], y - s.camera[1])
    screen(s, 200, 50)


def test_clicks_select_open_and_close_boxes():
    s = new()
    screen(s, 200, 50)
    m2 = s.layout.box_of(find(s, "M2")).bounds
    click(s, m2.x + 3, m2.y + 2)
    assert sel(s) == "M2" and not s.expanded
    click(s, m2.right - 3, m2.bottom - 1)
    assert find(s, "M2") in s.expanded, "clicking [+N] opens"
    e3 = s.layout.box_of(find(s, "E3")).bounds
    click(s, e3.x + 2, e3.y + 1)
    assert sel(s) == "E3", "innermost box wins"
    m2 = s.layout.box_of(find(s, "M2")).bounds
    click(s, m2.x + 6, m2.y)
    assert find(s, "M2") not in s.expanded and sel(s) == "M2"


def test_click_on_empty_space_hits_nothing():
    s = new()
    screen(s, 200, 50)
    assert s.click(199, 48) is False
    assert sel(s) == "M1"


def test_wheel_pans():
    s = new()
    screen(s, 120, 30, "-e")
    cam = s.camera
    s.scroll(down=True)
    assert s.camera == (cam[0], cam[1] + 4)
    s.scroll(down=True, sideways=True)
    assert s.camera == (cam[0] + 8, cam[1] + 4)


def test_mouse_in_textual_app():
    async def main():
        app = ClioViewer(new())
        async with app.run_test(size=(160, 40)) as pilot:
            await pilot.pause()
            st = app.state
            m1 = st.layout.box_of(find(st, "M1")).bounds
            cx, cy = st.camera
            # [+2] tag on M1's bottom border opens it ...
            await pilot.click("#canvas", offset=(m1.right - 3 - cx, m1.bottom - 1 - cy))
            await pilot.pause()
            assert find(st, "M1") in st.expanded
            assert "zoom:" in str(app.query_one("#status").render())
            # ... and its title bar closes it again.
            m1 = st.layout.box_of(find(st, "M1")).bounds
            cx, cy = st.camera
            await pilot.click("#canvas", offset=(m1.x + 5 - cx, m1.y - cy))
            await pilot.pause()
            assert not st.expanded and sel(st) == "M1"
            await pilot.press("x")
            await pilot.pause()
            assert st.message == "nothing to close"

    asyncio.run(main())


def test_plus_tag_hit_area_is_exactly_the_tag():
    s = new()
    screen(s, 200, 50)
    m2 = s.layout.box_of(find(s, "M2")).bounds
    x1 = m2.right - 1  # bottom-right corner; "[+2]" occupies x1-4 .. x1-1
    click(s, x1 - 5, m2.bottom - 1)
    click(s, x1, m2.bottom - 1)
    assert not s.expanded, "border cells beside the tag only select"
    click(s, x1 - 4, m2.bottom - 1)
    assert find(s, "M2") in s.expanded, "the '[' opens"
