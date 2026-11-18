import pytest

from clio_viewer.layout import Layout, Zoom, compute


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
