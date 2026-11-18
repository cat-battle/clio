mod common;

use std::collections::HashSet;

use clio_viewer::layout::{Layout, LayoutParams, Stretch, Zoom, compute, measure};
use clio_viewer::model::{Graph, NodeId};

fn all_expanded(g: &Graph) -> HashSet<NodeId> {
    (0..g.len()).filter(|&n| !g.children(n).is_empty()).collect()
}

fn all_types(g: &Graph) -> HashSet<String> {
    g.link_types().iter().cloned().collect()
}

fn run(g: &Graph, root: Option<NodeId>, expanded: &HashSet<NodeId>, zoom: Zoom, enabled: &HashSet<String>) -> Layout {
    run_st(g, root, expanded, zoom, enabled, Stretch::default())
}

fn params<'a>(
    root: Option<NodeId>,
    expanded: &'a HashSet<NodeId>,
    zoom: Zoom,
    enabled: &'a HashSet<String>,
    stretch: Stretch,
) -> LayoutParams<'a> {
    LayoutParams {
        root,
        expanded,
        zoom,
        enabled,
        stretch,
    }
}

fn run_st(
    g: &Graph,
    root: Option<NodeId>,
    expanded: &HashSet<NodeId>,
    zoom: Zoom,
    enabled: &HashSet<String>,
    stretch: Stretch,
) -> Layout {
    compute(g, &params(root, expanded, zoom, enabled, stretch))
}

/// Lowest common ancestor of two items, if any.
fn lca(g: &Graph, a: NodeId, b: NodeId) -> Option<NodeId> {
    let (pa, pb) = (g.path_from_root(a), g.path_from_root(b));
    let n = pa.iter().zip(&pb).take_while(|(x, y)| x == y).count();
    n.checked_sub(1).map(|i| pa[i])
}

/// Every cell of every wire lies strictly inside the box that contains both
/// of its ends.
fn check_containment(g: &Graph, l: &Layout) {
    for e in &l.edges {
        let Some(c) = lca(g, e.from, e.to).and_then(|c| l.box_of(c)) else {
            continue;
        };
        let (x0, y0, x1, y1) = c.bounds.interior();
        for w in e.points.windows(2) {
            let (p, q) = (w[0], w[1]);
            for x in p.0.min(q.0)..=p.0.max(q.0) {
                for y in p.1.min(q.1)..=p.1.max(q.1) {
                    assert!(
                        (x0..=x1).contains(&x) && (y0..=y1).contains(&y),
                        "{} edge {}->{} leaves {} at ({x},{y})",
                        e.kind,
                        g.item(e.from).id,
                        g.item(e.to).id,
                        g.item(c.node).id
                    );
                }
            }
        }
    }
}

const STRETCHES: [Stretch; 4] = [
    Stretch { x: 0, y: 0 },
    Stretch { x: 5, y: 2 },
    Stretch { x: 30, y: 6 },
    Stretch { x: 80, y: 20 },
];

fn check_geometry(g: &Graph, l: &Layout) {
    for (i, a) in l.boxes.iter().enumerate() {
        if let Some(p) = a.parent {
            let pb = &l.boxes[p];
            assert!(pb.expanded);
            assert_eq!(g.parent(a.node), Some(pb.node));
            assert!(
                pb.bounds.strictly_contains(&a.bounds),
                "{} not inside {}",
                g.item(a.node).id,
                g.item(pb.node).id
            );
        }
        for b in &l.boxes[i + 1..] {
            if a.parent == b.parent {
                assert!(
                    !a.bounds.intersects(&b.bounds),
                    "siblings {} and {} overlap",
                    g.item(a.node).id,
                    g.item(b.node).id
                );
            }
        }
    }
}

#[test]
fn geometry_is_sound_at_every_zoom_and_expansion() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let none = HashSet::new();
    let all = all_expanded(&g);
    let partial: HashSet<NodeId> = ["M1", "E2", "S4"].iter().map(|id| g.find(id).unwrap()).collect();
    for zoom in [Zoom::Detail, Zoom::Compact, Zoom::Overview] {
        for expanded in [&none, &partial, &all] {
            let l = run(&g, None, expanded, zoom, &enabled);
            check_geometry(&g, &l);
            let expected = if expanded.is_empty() { 2 } else { l.boxes.len() };
            assert_eq!(l.boxes.len(), expected);
        }
    }
    let l = run(&g, None, &all, Zoom::Detail, &enabled);
    assert_eq!(l.boxes.len(), g.len());
}

#[test]
fn routes_stay_inside_their_common_container() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let ids = |v: &[&str]| -> HashSet<NodeId> { v.iter().map(|id| g.find(id).unwrap()).collect() };
    let expansions = [ids(&["M1"]), ids(&["M1", "E1", "E2"]), ids(&["M1", "M2", "E2", "E3", "S4"]), all_expanded(&g)];
    let mut checked = 0;
    for zoom in [Zoom::Detail, Zoom::Compact, Zoom::Overview] {
        for st in STRETCHES {
            for expanded in &expansions {
                for root in [None, g.find("M1")] {
                    let l = run_st(&g, root, expanded, zoom, &enabled, st);
                    check_geometry(&g, &l);
                    check_containment(&g, &l);
                    checked += l.edges.len();
                }
            }
        }
    }
    assert!(checked > 500, "{checked}");
}

#[test]
fn stretch_widens_boxes_first_then_spacing() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let none = HashSet::new();
    let m1 = g.find("M1").unwrap();
    let size = |st| {
        let l = run_st(&g, None, &none, Zoom::Detail, &enabled, st);
        let b = l.box_of(m1).unwrap().bounds;
        (b.w, b.h, l.width, l.height)
    };
    assert_eq!(size(Stretch { x: 0, y: 0 }).0, 26);
    assert_eq!(size(Stretch { x: 10, y: 0 }).0, 36);
    assert_eq!(size(Stretch { x: 26, y: 0 }).0, 52, "boxes stop at twice their width");
    let (w52, ..) = size(Stretch { x: 26, y: 0 });
    let (w60, _, lw60, _) = size(Stretch { x: 60, y: 0 });
    assert_eq!(w60, w52);
    assert!(lw60 > size(Stretch { x: 26, y: 0 }).2, "then the gaps grow");
    assert_eq!(size(Stretch { x: 0, y: 3 }).1, 8);
    assert_eq!(size(Stretch { x: 0, y: 9 }).1, 9, "detail boxes gain at most 4 rows");
}

#[test]
fn measure_matches_compute_without_edges() {
    let g = common::sample_graph();
    let none_types = HashSet::new();
    let all = all_expanded(&g);
    for zoom in [Zoom::Detail, Zoom::Overview] {
        for st in STRETCHES {
            let p = params(None, &all, zoom, &none_types, st);
            let l = compute(&g, &p);
            assert_eq!(measure(&g, &p), (l.width, l.height));
        }
    }
}

#[test]
fn collapsed_view_lifts_and_merges_edges_onto_visible_boxes() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let l = run(&g, None, &HashSet::new(), Zoom::Detail, &enabled);
    let (m1, m2) = (g.find("M1").unwrap(), g.find("M2").unwrap());
    // blocks: S5->S7, S3->S8; relates_to: S6->S12, S2->T7 (the others stay inside one milestone)
    let blocks = l.edges.iter().find(|e| e.kind == "blocks").unwrap();
    assert_eq!((blocks.from, blocks.to, blocks.count), (m1, m2, 2));
    let rel = l.edges.iter().find(|e| e.kind == "relates_to").unwrap();
    assert_eq!((rel.from, rel.to, rel.count), (m1, m2, 2));
    assert_eq!(l.edges.len(), 2);
}

#[test]
fn edges_attach_to_visible_boxes_and_are_orthogonal() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    for expanded in [HashSet::new(), all_expanded(&g)] {
        let l = run(&g, None, &expanded, Zoom::Compact, &enabled);
        for e in &l.edges {
            let (a, b) = (l.box_of(e.from).unwrap(), l.box_of(e.to).unwrap());
            let first = e.points[0];
            let last = *e.points.last().unwrap();
            assert_eq!(first, (a.bounds.right(), e.from_port.1));
            assert_eq!(last, (b.bounds.x - 1, e.to_port.1));
            assert!(a.bounds.contains_point(e.from_port.0, e.from_port.1));
            assert!(b.bounds.contains_point(e.to_port.0, e.to_port.1));
            for w in e.points.windows(2) {
                assert!(w[0].0 == w[1].0 || w[0].1 == w[1].1, "diagonal segment {w:?}");
            }
        }
    }
}

#[test]
fn routes_avoid_unrelated_leaf_boxes() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let l = run(&g, None, &all_expanded(&g), Zoom::Compact, &enabled);
    for e in &l.edges {
        for w in e.points.windows(2) {
            let (p, q) = (w[0], w[1]);
            for b in l.boxes.iter().filter(|b| !b.expanded && b.node != e.from && b.node != e.to) {
                let crosses = (p.0.min(q.0)..=p.0.max(q.0))
                    .any(|x| (p.1.min(q.1)..=p.1.max(q.1)).any(|y| b.bounds.contains_point(x, y)));
                assert!(
                    !crosses,
                    "{} edge {}->{} crosses {}",
                    e.kind,
                    g.item(e.from).id,
                    g.item(e.to).id,
                    g.item(b.node).id
                );
            }
        }
    }
}

#[test]
fn disabling_a_link_type_removes_its_edges() {
    let g = common::sample_graph();
    let all = all_expanded(&g);
    let mut enabled = all_types(&g);
    let l = run(&g, None, &all, Zoom::Detail, &enabled);
    assert!(l.edges.iter().any(|e| e.kind == "relates_to"));
    enabled.remove("relates_to");
    let l = run(&g, None, &all, Zoom::Detail, &enabled);
    assert!(l.edges.iter().all(|e| e.kind != "relates_to"));
    assert!(l.edges.iter().any(|e| e.kind == "blocks"));
    let l = run(&g, None, &all, Zoom::Detail, &HashSet::new());
    assert!(l.edges.is_empty());
}

#[test]
fn dependencies_flow_left_to_right() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let e1 = g.find("E1").unwrap();
    let expanded = HashSet::from([e1]);
    let l = run(&g, Some(g.find("M1").unwrap()), &expanded, Zoom::Detail, &enabled);
    let x = |id: &str| l.box_of(g.find(id).unwrap()).unwrap().bounds.x;
    assert!(x("S1") < x("S3"));
    assert!(x("S2") < x("S3"));
    assert_eq!(x("S1"), x("S2"));
    assert!(x("E1") < x("E2")); // S1 blocks S5 projects to E1 -> E2
}

#[test]
fn drill_root_limits_view_and_counts_hidden_links() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let e2 = g.find("E2").unwrap();
    let l = run(&g, Some(e2), &HashSet::new(), Zoom::Detail, &enabled);
    let ids: HashSet<&str> = l.boxes.iter().map(|b| g.item(b.node).id.as_str()).collect();
    assert_eq!(ids, HashSet::from(["S4", "S5", "S6"]));
    // S1->S5, S5->S7, S6->S12 cross the E2 boundary
    assert_eq!(l.hidden_links, 3);
}

#[test]
fn zoom_shrinks_boxes() {
    let g = common::sample_graph();
    let enabled = all_types(&g);
    let w = |z| run(&g, None, &HashSet::new(), z, &enabled).width;
    assert!(w(Zoom::Detail) > w(Zoom::Compact));
    assert!(w(Zoom::Compact) > w(Zoom::Overview));
}
