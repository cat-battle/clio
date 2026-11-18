"""Dependency-cone tracing against the sample fixture (mirrors rust/tests/trace.rs)."""

import asyncio

from clio_viewer.app import ClioViewer
from clio_viewer.model import Document, Graph
from clio_viewer.render import role_color
from clio_viewer.state import ViewerState
from clio_viewer.trace import Role, Trace, TraceMode
from clio_viewer.views import snapshot_styles

from .conftest import sample_doc


def ids(g, nodes):
    return sorted(g.item(n).id for n in nodes)


def state_at(item_id):
    s = ViewerState(Graph(sample_doc()))
    s.reveal(s.graph.find(item_id))
    return s


def test_cone_follows_links_both_ways_and_pulls_in_children():
    s = state_at("S5")
    s.trace_mode = TraceMode.BOTH
    t = s.trace()
    g = s.graph
    assert ids(g, t.focus) == ["S5"]
    assert ids(g, t.upstream) == ["S1", "S4", "T1", "T2", "T5", "T6"]
    assert ids(g, t.downstream) == ["S12", "S6", "S7", "S8", "S9", "T7", "T8"]
    assert len(Trace.outermost(g, t.upstream)) == 2
    assert len(Trace.outermost(g, t.downstream)) == 5


def test_direction_and_link_filters_shape_the_cone():
    s = state_at("S5")
    s.trace_mode = TraceMode.DOWNSTREAM
    t = s.trace()
    assert not t.upstream and s.graph.find("S12") in t.downstream
    s.enabled.discard("relates_to")
    assert ids(s.graph, s.trace().downstream) == ["S6", "S7", "S8", "S9", "T7", "T8"]
    s.trace_mode = TraceMode.UPSTREAM
    assert not s.trace().downstream
    s.trace_mode = TraceMode.OFF
    assert s.trace() is None


def test_container_focus_includes_its_descendants_links():
    s = state_at("E1")
    s.trace_mode = TraceMode.BOTH
    t = s.trace()
    assert not t.upstream
    for i in ("S5", "S8", "T7"):
        assert s.graph.find(i) in t.downstream, i


def test_roles_and_edges_are_lifted_onto_collapsed_boxes():
    s = ViewerState(Graph(sample_doc()))
    s.trace_mode = TraceMode.BOTH
    t = s.trace()
    m1, m2 = s.graph.find("M1"), s.graph.find("M2")
    assert t.role(m1) is Role.FOCUS and t.role(m2) is Role.DOWNSTREAM
    assert t.edge_traced(m1, m2, "blocks") and not t.edge_traced(m2, m1, "blocks")
    assert len(t.hidden(s.graph, s.layout)) == len(Trace.outermost(s.graph, t.downstream)) == 4


def test_cycles_mark_boxes_as_both():
    doc = Document.from_dict({
        "version": 1,
        "items": [{"id": i, "kind": "story", "title": i} for i in "ABC"],
        "links": [{"from": "A", "to": "B", "type": "blocks"}, {"from": "B", "to": "A", "type": "blocks"}],
    })
    s = ViewerState(Graph(doc))
    s.trace_mode = TraceMode.BOTH
    t = s.trace()
    g = s.graph
    assert t.role(g.find("A")) is Role.FOCUS
    assert t.role(g.find("B")) is Role.BOTH
    assert t.role(g.find("C")) is None


def test_reveal_expands_and_drills_out_to_show_every_member():
    s = state_at("S5")
    s.drill.append(s.graph.find("E2"))
    s.relayout()
    s.handle_key("T")
    assert s.trace_mode is TraceMode.BOTH and not s.drill
    assert not s.trace().hidden(s.graph, s.layout)
    for i in ("S1", "S4", "S6", "S7", "S8", "S9", "S12"):
        assert s.layout.visible(s.graph.find(i)), i
    assert s.selected == s.graph.find("S5")
    s.handle_key("T")
    assert s.message == "whole trace already visible"


def test_status_bar_and_key_cycle():
    s = ViewerState(Graph(sample_doc()))
    text, _ = snapshot_styles(s, 160, 30, ["t"])
    assert "trace:both ↑0 ↓4 (4 hidden, T shows)" in text.splitlines()[-1]
    text, _ = snapshot_styles(s, 160, 30, ["t"])
    assert "trace:upstream ↑0 " in text.splitlines()[-1]
    text, _ = snapshot_styles(s, 160, 30, ["t", "t"])
    assert "trace:" not in text


def test_dimming_and_highlight_colours():
    s = ViewerState(Graph(sample_doc()))
    _, plain = snapshot_styles(s, 120, 20, [])
    assert "grey50/-/0" not in plain.split("\n")[0] and role_color(Role.DOWNSTREAM) not in plain
    _, traced = snapshot_styles(s, 120, 20, ["t"])
    assert role_color(Role.DOWNSTREAM) in traced  # M2's border
    s.reveal(s.graph.find("S11"))  # S11 only has a 'duplicates' link to S10
    _, traced = snapshot_styles(s, 160, 40, [])
    assert "grey50" in traced and role_color(Role.UPSTREAM) not in traced


def test_trace_key_in_textual_app():
    async def main():
        app = ClioViewer(ViewerState(Graph(sample_doc())))
        async with app.run_test(size=(160, 40)) as pilot:
            await pilot.press("t")
            await pilot.pause()
            assert app.state.trace_mode is TraceMode.BOTH
            assert "trace:both ↑0 ↓4" in str(app.query_one("#status").render())
            await pilot.press("T")
            await pilot.pause()
            assert app.state.message == "revealed 4 traced items"
            assert app.state.layout.visible(app.state.graph.find("S12"))
            assert "═ M2 " in app.canvas.text()

    asyncio.run(main())
