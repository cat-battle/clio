import json
import shutil

from clio_viewer.adapters import FileLoader
from clio_viewer.keys import parse_keys
from clio_viewer.layout import Zoom
from clio_viewer.model import Graph
from clio_viewer.state import ViewerState
from clio_viewer.views import snapshot

import pytest

from .conftest import FIXTURES


@pytest.fixture
def state(graph):
    return ViewerState(graph)


def press(state, script):
    for k in parse_keys(script):
        assert state.handle_key(k), k
        state.set_viewport(160, 39)


def sel(state):
    return state.graph.item(state.selected).id


def test_expand_drill_and_navigation(state):
    assert sel(state) == "M1"
    press(state, "l")
    assert sel(state) == "M2"
    press(state, "h<space>l")
    assert sel(state) == "E1", "right on an open box steps into it"
    press(state, "p")
    assert sel(state) == "M1"
    press(state, "<enter>")
    assert [state.graph.item(n).id for n in state.drill] == ["M1"] and sel(state) == "E1"
    press(state, "l<enter>")
    assert sel(state) == "S4" and len(state.drill) == 2
    press(state, "<esc><esc>")
    assert not state.drill and sel(state) == "M1"


def test_zoom_and_pan(state):
    press(state, "-")
    assert state.zoom is Zoom.COMPACT
    press(state, "--")
    assert state.zoom is Zoom.OVERVIEW
    press(state, "+=")
    assert state.zoom is Zoom.DETAIL
    cam = state.camera
    press(state, "<S-right>")
    assert state.camera[0] == cam[0] + 8


def test_expand_all_collapse_all_and_leaf_message(state):
    press(state, "e")
    assert len(state.layout.boxes) == len(state.graph)
    press(state, "E")
    assert len(state.layout.boxes) == 2
    state.reveal(state.graph.find("T1"))
    press(state, "<space>")
    assert state.message == "T1 has no children"


def test_link_filters(state):
    before = len(state.layout.edges)
    state.toggle_link_type("blocks")
    assert "blocks" not in state.enabled and len(state.layout.edges) == before - 1
    state.set_enabled(set())
    assert not state.layout.edges
    state.set_enabled({"blocks", "bogus"})
    assert state.enabled == {"blocks"}


def test_search_reveals_and_repeats(state):
    state.search("webhook")
    assert sel(state) == "T7"
    for i in ("M2", "E3", "S8"):
        assert state.graph.find(i) in state.expanded
    state.search("sign-in")
    assert sel(state) == "T3"
    press(state, "n")
    assert sel(state) == "T4"
    state.search("nothing-matches")
    assert sel(state) == "T4" and "no match" in state.message


def test_reload_preserves_view_state(tmp_path):
    path = tmp_path / "tasks.json"
    shutil.copy(FIXTURES / "sample.json", path)
    loader = FileLoader(path)
    state = ViewerState(Graph(loader.load()), loader)
    state.reveal(state.graph.find("S8"))
    state.toggle_link_type("relates_to")
    data = json.loads(path.read_text())
    data["items"].insert(0, {"id": "M0", "kind": "milestone", "title": "Alpha"})
    data["links"].append({"from": "M0", "to": "M1", "type": "precedes"})
    path.write_text(json.dumps(data))
    state.handle_key("R")
    assert state.message == "reloaded 27 items"
    assert sel(state) == "S8"
    assert state.graph.find("E3") in state.expanded
    assert state.enabled == {"blocks", "duplicates", "precedes"}, "old filters kept, new type enabled"
    path.write_text("{broken")
    state.handle_key("R")
    assert state.message.startswith("reload failed")
    assert len(state.graph) == 27


def test_snapshot_text(state):
    out = snapshot(state, 120, 20, parse_keys("<space>"))
    lines = out.splitlines()
    assert len(lines) == 20 and all(len(l) == 120 for l in lines)
    assert "M1 Beta launch" in out and "E1 · epic" in out
    assert lines[-1].startswith(" Top  │ zoom:detail │ ■ blocks")
    with pytest.raises(ValueError, match="snapshot mode"):
        snapshot(state, 80, 20, ["?"])


def test_key_script_parser():
    assert parse_keys("j<space><S-left>E<enter>") == ["j", " ", "shift+left", "E", "enter"]
    with pytest.raises(ValueError):
        parse_keys("<bogus>")
    with pytest.raises(ValueError):
        parse_keys("<space")
