//! Drives the real App + ratatui rendering through a TestBackend.

mod common;

use clio_viewer::app::{App, Mode};
use clio_viewer::keys::parse_keys;
use clio_viewer::layout::Zoom;
use clio_viewer::ui::snapshot;

fn app() -> App {
    App::new(common::sample_graph(), None)
}

fn screen(app: &mut App, keys: &str) -> String {
    snapshot(app, 160, 40, &parse_keys(keys).unwrap()).unwrap()
}

fn sel(app: &App) -> &str {
    &app.graph.item(app.selected.unwrap()).id
}

#[test]
fn initial_view_shows_milestones_and_status_bar() {
    let mut a = app();
    let s = screen(&mut a, "");
    assert!(s.contains("M1 · milestone") && s.contains("M2 · milestone"), "{s}");
    assert!(!s.contains("E1"), "epics hidden until expanded");
    assert!(s.contains("Top") && s.contains("zoom:detail") && s.contains("■ blocks"));
    assert_eq!(sel(&a), "M1");
}

#[test]
fn space_expands_inline_and_collapses() {
    let mut a = app();
    let s = screen(&mut a, "<space>");
    assert!(s.contains("M1 Beta launch") && s.contains("E1 · epic") && s.contains("E2 · epic"), "{s}");
    assert!(s.contains("M2 · milestone"), "sibling stays visible beside the expanded box");
    let s = screen(&mut a, "<space>");
    assert!(!s.contains("E1"), "{s}");
}

#[test]
fn enter_drills_in_and_esc_drills_out() {
    let mut a = app();
    let s = screen(&mut a, "<enter>");
    assert_eq!(a.drill.len(), 1);
    assert!(s.contains("Top › M1"), "{s}");
    assert!(s.contains("E1 · epic") && !s.contains("M2 · milestone"), "{s}");
    assert_eq!(sel(&a), "E1");
    screen(&mut a, "l<enter>");
    assert_eq!(a.drill.len(), 2);
    assert_eq!(sel(&a), "S4");
    let s = screen(&mut a, "<esc><esc>");
    assert!(a.drill.is_empty());
    assert_eq!(sel(&a), "M1");
    assert!(s.contains("M2 · milestone"));
}

#[test]
fn arrow_navigation_moves_between_boxes() {
    let mut a = app();
    screen(&mut a, "l");
    assert_eq!(sel(&a), "M2");
    screen(&mut a, "h<space>l");
    assert_eq!(sel(&a), "E1", "moving right from an expanded box enters its children first");
    screen(&mut a, "p");
    assert_eq!(sel(&a), "M1");
}

#[test]
fn zoom_changes_detail_level() {
    let mut a = app();
    let s = screen(&mut a, "-");
    assert_eq!(a.zoom, Zoom::Compact);
    assert!(s.contains("M1 Beta launch") && !s.contains("· milestone"), "{s}");
    let s = screen(&mut a, "-");
    assert_eq!(a.zoom, Zoom::Overview);
    assert!(!s.contains("Beta launch"), "{s}");
    screen(&mut a, "++");
    assert_eq!(a.zoom, Zoom::Detail);
}

#[test]
fn relationship_panel_toggles_link_types() {
    let mut a = app();
    let s = screen(&mut a, "r");
    assert!(matches!(a.mode, Mode::Relations { .. }));
    assert!(s.contains("Relationships") && s.contains("[x] blocks"), "{s}");
    let before = a.layout.edges.len();
    let s = screen(&mut a, "<space>");
    assert!(s.contains("[ ] blocks"), "{s}");
    assert!(!a.enabled.contains("blocks"));
    assert_eq!(a.layout.edges.len(), before - 1);
    screen(&mut a, "n");
    assert!(a.layout.edges.is_empty());
    screen(&mut a, "a<esc>");
    assert_eq!(a.mode, Mode::Normal);
    assert_eq!(a.layout.edges.len(), before);
}

#[test]
fn search_reveals_nested_item() {
    let mut a = app();
    let s = screen(&mut a, "/webhook<enter>");
    assert_eq!(sel(&a), "T7");
    assert!(s.contains("T7"), "{s}");
    for id in ["M2", "E3", "S8"] {
        assert!(a.expanded.contains(&a.graph.find(id).unwrap()), "{id} expanded");
    }
    screen(&mut a, "/nothing-matches<enter>");
    assert_eq!(sel(&a), "T7");
    assert!(a.message.as_deref().unwrap_or("").contains("no match"));
}

#[test]
fn details_panel_and_help() {
    let mut a = app();
    let s = screen(&mut a, "d");
    assert!(s.contains("Details") && s.contains("Assignee") && s.contains("dana"), "{s}");
    let s = screen(&mut a, "?");
    assert!(s.contains("Keys (any key closes)"), "{s}");
    screen(&mut a, "x");
    assert_eq!(a.mode, Mode::Normal);
}

#[test]
fn expand_all_and_collapse_all() {
    let mut a = app();
    screen(&mut a, "e");
    assert_eq!(a.layout.boxes.len(), a.graph.len());
    screen(&mut a, "E");
    assert_eq!(a.layout.boxes.len(), 2);
}

#[test]
fn quit_key() {
    let mut a = app();
    screen(&mut a, "q");
    assert!(a.should_quit);
}
