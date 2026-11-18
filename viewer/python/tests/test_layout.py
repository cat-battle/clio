import pytest

from clio_viewer.layout import Layout, Stretch, Zoom, compute, measure
from clio_viewer.render import wrap


def all_expanded(g):
    return {n for n in range(len(g)) if g.children(n)}


def run(g, root=None, expanded=(), zoom=Zoom.DETAIL, enabled=None) -> Layout:
    return compute(g, root, set(expanded), zoom, set(g.link_types) if enabled is None else enabled)


def check_geometry(g, l: Layout):
    for i, a in enumerate(l.boxes):
        if a.parent is not None:
            pb = l.boxes[a.parent]
            assert pb.expanded
            assert g.parent(a.node) == pb.node
            assert pb.bounds.strictly_contains(a.bounds), (g.item(a.node).id, g.item(pb.node).id)
        for b in l.boxes[i + 1 :]:
            if a.parent == b.parent:
                assert not a.bounds.intersects(b.bounds), (g.item(a.node).id, g.item(b.node).id)


@pytest.mark.parametrize("zoom", list(Zoom))
@pytest.mark.parametrize("which", ["none", "partial", "all"])
def test_geometry_is_sound(graph, zoom, which):
    expanded = {
        "none": set(),
        "partial": {graph.find(i) for i in ("M1", "E2", "S4")},
        "all": all_expanded(graph),
    }[which]
    l = run(graph, expanded=expanded, zoom=zoom)
    check_geometry(graph, l)
    if which == "none":
        assert len(l.boxes) == 2
    if which == "all":
        assert len(l.boxes) == len(graph)


def test_collapsed_view_lifts_and_merges_edges(graph):
    l = run(graph)
    m1, m2 = graph.find("M1"), graph.find("M2")
    # blocks: S5->S7, S3->S8; relates_to: S6->S12, S2->T7
    by_type = {e.type: e for e in l.edges}
    assert set(by_type) == {"blocks", "relates_to"}
    assert (by_type["blocks"].source, by_type["blocks"].target, by_type["blocks"].count) == (m1, m2, 2)
    assert (by_type["relates_to"].source, by_type["relates_to"].target, by_type["relates_to"].count) == (m1, m2, 2)


@pytest.mark.parametrize("expand", [False, True])
def test_edges_attach_to_visible_boxes_and_are_orthogonal(graph, expand):
    l = run(graph, expanded=all_expanded(graph) if expand else (), zoom=Zoom.COMPACT)
    assert l.edges
    for e in l.edges:
        a, b = l.box_of(e.source), l.box_of(e.target)
        assert e.points[0] == (a.bounds.right, e.from_port[1])
        assert e.points[-1] == (b.bounds.x - 1, e.to_port[1])
        assert a.bounds.contains_point(*e.from_port)
        assert b.bounds.contains_point(*e.to_port)
        for p, q in zip(e.points, e.points[1:]):
            assert p[0] == q[0] or p[1] == q[1], f"diagonal segment {p}->{q}"


def test_routes_avoid_unrelated_leaf_boxes(graph):
    l = run(graph, expanded=all_expanded(graph), zoom=Zoom.COMPACT)
    for e in l.edges:
        for p, q in zip(e.points, e.points[1:]):
            cells = [(x, y) for x in range(min(p[0], q[0]), max(p[0], q[0]) + 1) for y in range(min(p[1], q[1]), max(p[1], q[1]) + 1)]
            for b in l.boxes:
                if b.expanded or b.node in (e.source, e.target):
                    continue
                assert not any(b.bounds.contains_point(*c) for c in cells), (
                    f"{e.type} edge {graph.item(e.source).id}->{graph.item(e.target).id} crosses {graph.item(b.node).id}"
                )


def test_disabling_a_link_type_removes_its_edges(graph):
    expanded = all_expanded(graph)
    enabled = set(graph.link_types)
    assert any(e.type == "relates_to" for e in run(graph, expanded=expanded, enabled=enabled).edges)
    enabled.discard("relates_to")
    l = run(graph, expanded=expanded, enabled=enabled)
    assert all(e.type != "relates_to" for e in l.edges)
    assert any(e.type == "blocks" for e in l.edges)
    assert not run(graph, expanded=expanded, enabled=set()).edges


def test_dependencies_flow_left_to_right(graph):
    l = run(graph, root=graph.find("M1"), expanded={graph.find("E1")})
    x = lambda i: l.box_of(graph.find(i)).bounds.x  # noqa: E731
    assert x("S1") < x("S3")
    assert x("S2") < x("S3")
    assert x("S1") == x("S2")
    assert x("E1") < x("E2")  # S1 blocks S5 projects to E1 -> E2


def test_drill_root_limits_view_and_counts_hidden_links(graph):
    l = run(graph, root=graph.find("E2"))
    assert {graph.item(b.node).id for b in l.boxes} == {"S4", "S5", "S6"}
    assert l.hidden_links == 3  # S1->S5, S5->S7, S6->S12 cross the E2 boundary


def test_zoom_shrinks_boxes(graph):
    w = [run(graph, zoom=z).width for z in Zoom]
    assert w[0] > w[1] > w[2]


def test_cycles_do_not_break_layering(graph):
    from clio_viewer.layout import _layer_assignment

    assert _layer_assignment(3, [(0, 1), (1, 2)]) == [0, 1, 2]
    assert _layer_assignment(3, [(2, 1), (1, 0)]) == [2, 1, 0]
    assert _layer_assignment(2, [(0, 1), (1, 0)]) == [0, 1]


# ------------------------------------------------- containment and stretch

STRETCHES = [Stretch(0, 0), Stretch(5, 2), Stretch(30, 6), Stretch(80, 20)]


def lca(g, a, b):
    pa, pb = g.path_from_root(a), g.path_from_root(b)
    n = 0
    while n < min(len(pa), len(pb)) and pa[n] == pb[n]:
        n += 1
    return pa[n - 1] if n else None


def check_containment(g, l: Layout):
    """Every cell of every wire lies strictly inside the box containing both ends."""
    for e in l.edges:
        c = lca(g, e.source, e.target)
        box = l.box_of(c) if c is not None else None
        if box is None:
            continue
        x0, y0, x1, y1 = box.bounds.interior()
        for (px, py), (qx, qy) in zip(e.points, e.points[1:]):
            for x in range(min(px, qx), max(px, qx) + 1):
                for y in range(min(py, qy), max(py, qy) + 1):
                    assert x0 <= x <= x1 and y0 <= y <= y1, (
                        f"{e.type} edge {g.item(e.source).id}->{g.item(e.target).id} "
                        f"leaves {g.item(c).id} at {(x, y)}"
                    )


@pytest.mark.parametrize("zoom", list(Zoom))
@pytest.mark.parametrize("st", STRETCHES, ids=lambda s: f"{s.x}x{s.y}")
def test_routes_stay_inside_their_common_container(graph, zoom, st):
    ids = lambda *v: {graph.find(i) for i in v}
    enabled = set(graph.link_types)
    checked = 0
    for expanded in (ids("M1"), ids("M1", "E1", "E2"), ids("M1", "M2", "E2", "E3", "S4"), all_expanded(graph)):
        for root in (None, graph.find("M1")):
            l = compute(graph, root, expanded, zoom, enabled, st)
            check_geometry(graph, l)
            check_containment(graph, l)
            checked += len(l.edges)
    assert checked > 40


def test_stretch_widens_boxes_first_then_spacing(graph):
    enabled = set(graph.link_types)
    m1 = graph.find("M1")

    def size(x, y):
        l = compute(graph, None, set(), Zoom.DETAIL, enabled, Stretch(x, y))
        b = l.box_of(m1).bounds
        return b.w, b.h, l.width, l.height

    assert size(0, 0)[0] == 26
    assert size(10, 0)[0] == 36
    assert size(26, 0)[0] == 52, "boxes stop at twice their width"
    assert size(60, 0)[0] == 52 and size(60, 0)[2] > size(26, 0)[2], "then the gaps grow"
    assert size(0, 3)[1] == 8
    assert size(0, 9)[1] == 9, "detail boxes gain at most 4 rows"


@pytest.mark.parametrize("zoom", [Zoom.DETAIL, Zoom.OVERVIEW])
@pytest.mark.parametrize("st", STRETCHES, ids=lambda s: f"{s.x}x{s.y}")
def test_measure_matches_compute_without_edges(graph, zoom, st):
    all_ = all_expanded(graph)
    l = compute(graph, None, all_, zoom, set(), st)
    assert measure(graph, None, all_, zoom, set(), st) == (l.width, l.height)


def test_wrap():
    assert wrap("Login page", 20, 1) == ["Login page"]
    assert wrap("General availability: billing and production", 20, 2) == [
        "General", "availability: billi…",
    ]
    assert wrap("one two three four five", 9, 2) == ["one two", "three fo…"]
    assert wrap("one two three", 9, 5) == ["one two", "three"]
    assert wrap("abcdefghijkl xy", 5, 4) == ["abcde", "fghij", "kl xy"]
    assert wrap("anything", 0, 3) == [] and wrap("anything", 5, 0) == []
    assert wrap("a  b\n c", 10, 1) == ["a b c"]
