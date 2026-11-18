//! Dependency-cone tracing against the sample fixture.

mod common;

use std::collections::HashSet;

use clio_viewer::app::App;
use clio_viewer::keys::parse_keys;
use clio_viewer::model::{Document, Graph, NodeId};
use clio_viewer::trace::{Role, Trace, TraceMode};
use clio_viewer::ui::snapshot;

fn ids(g: &Graph, set: &HashSet<NodeId>) -> Vec<String> {
    let mut v: Vec<String> = set.iter().map(|&n| g.item(n).id.clone()).collect();
    v.sort();
    v
}

fn app_at(id: &str) -> App {
    let mut a = App::new(common::sample_graph(), None);
    let n = a.graph.find(id).unwrap();
    a.reveal(n);
    a
}

fn trace(a: &App) -> Trace {
    a.trace().expect("tracing is on")
}

#[test]
fn cone_follows_links_both_ways_and_pulls_in_children() {
    let mut a = app_at("S5");
    a.trace_mode = TraceMode::Both;
    let t = trace(&a);
    let g = &a.graph;
    assert_eq!(ids(g, &t.focus), ["S5"]);
    // S4 and S1 block S5 (and so do their subtasks); T6 -> S5 is inside S4.
    assert_eq!(ids(g, &t.upstream), ["S1", "S4", "T1", "T2", "T5", "T6"]);
    // S5 -> S6, S7; S7 -> S8 (+T7, T8) -> S9; S6, S9 relate to S12.
    assert_eq!(ids(g, &t.downstream), ["S12", "S6", "S7", "S8", "S9", "T7", "T8"]);
    assert_eq!(Trace::outermost(g, &t.upstream).len(), 2);
    assert_eq!(Trace::outermost(g, &t.downstream).len(), 5);
}

#[test]
fn direction_and_link_filters_shape_the_cone() {
    let mut a = app_at("S5");
    a.trace_mode = TraceMode::Downstream;
    let t = trace(&a);
    assert!(t.upstream.is_empty());
    assert!(t.downstream.contains(&a.graph.find("S12").unwrap()));

    a.enabled.remove("relates_to");
    let t = trace(&a);
    assert_eq!(ids(&a.graph, &t.downstream), ["S6", "S7", "S8", "S9", "T7", "T8"]);

    a.trace_mode = TraceMode::Upstream;
    assert!(trace(&a).downstream.is_empty());
    a.trace_mode = TraceMode::Off;
    assert!(a.trace().is_none());
}

#[test]
fn container_focus_includes_its_descendants_links() {
    let mut a = app_at("E1");
    a.trace_mode = TraceMode::Both;
    let t = trace(&a);
    let g = &a.graph;
    assert!(t.upstream.is_empty(), "nothing outside E1 points into it");
    for id in ["S5", "S8", "T7"] {
        assert!(t.downstream.contains(&g.find(id).unwrap()), "{id}");
    }
}

#[test]
fn roles_and_edges_are_lifted_onto_collapsed_boxes() {
    let mut a = App::new(common::sample_graph(), None);
    a.selected = a.graph.find("M1");
    a.trace_mode = TraceMode::Both;
    let t = trace(&a);
    let (m1, m2) = (a.graph.find("M1").unwrap(), a.graph.find("M2").unwrap());
    assert_eq!(t.role(m1), Some(Role::Focus));
    assert_eq!(t.role(m2), Some(Role::Downstream));
    assert!(t.edge_traced(m1, m2, "blocks"));
    assert!(!t.edge_traced(m2, m1, "blocks"));
    // Every downstream item sits inside collapsed M2.
    assert_eq!(t.hidden(&a.graph, &a.layout).len(), Trace::outermost(&a.graph, &t.downstream).len());
}

#[test]
fn cycles_mark_boxes_as_both() {
    let doc: Document = serde_json::from_str(
        r#"{"version":1,"items":[
            {"id":"A","kind":"story","title":"a"},
            {"id":"B","kind":"story","title":"b"},
            {"id":"C","kind":"story","title":"c"}],
          "links":[{"from":"A","to":"B","type":"blocks"},{"from":"B","to":"A","type":"blocks"}]}"#,
    )
    .unwrap();
    let mut a = App::new(Graph::from_document(doc).unwrap(), None);
    a.trace_mode = TraceMode::Both;
    let t = trace(&a);
    let g = &a.graph;
    assert_eq!(t.role(g.find("A").unwrap()), Some(Role::Focus));
    assert_eq!(t.role(g.find("B").unwrap()), Some(Role::Both));
    assert_eq!(t.role(g.find("C").unwrap()), None);
}

#[test]
fn reveal_expands_and_drills_out_to_show_every_member() {
    let mut a = app_at("S5");
    // Drill into E2: S1 (upstream) and everything downstream are now outside the view.
    a.drill.push(a.graph.find("E2").unwrap());
    a.relayout();
    a.handle_key(parse_keys("T").unwrap()[0]);
    assert_eq!(a.trace_mode, TraceMode::Both, "T turns tracing on");
    assert!(a.drill.is_empty(), "drilled out to reach S1 and M2's stories");
    let t = trace(&a);
    assert!(t.hidden(&a.graph, &a.layout).is_empty());
    for id in ["S1", "S4", "S6", "S7", "S8", "S9", "S12"] {
        assert!(a.layout.visible(a.graph.find(id).unwrap()), "{id}");
    }
    assert_eq!(a.selected, a.graph.find("S5"));
    a.handle_key(parse_keys("T").unwrap()[0]);
    assert_eq!(a.message.as_deref(), Some("whole trace already visible"));
}

#[test]
fn status_bar_and_key_cycle() {
    let mut a = App::new(common::sample_graph(), None);
    let s = snapshot(&mut a, 160, 30, &parse_keys("t").unwrap()).unwrap();
    let status = s.lines().last().unwrap();
    assert!(status.contains("trace:both ↑0 ↓4 (4 hidden, T shows)"), "{status}");
    let s = snapshot(&mut a, 160, 30, &parse_keys("t").unwrap()).unwrap();
    assert!(s.lines().last().unwrap().contains("trace:upstream ↑0 "), "{s}");
    let s = snapshot(&mut a, 160, 30, &parse_keys("tt").unwrap()).unwrap();
    assert!(!s.contains("trace:"), "fourth press turns it off");
}
