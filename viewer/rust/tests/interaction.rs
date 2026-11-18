//! Auto-fit zoom, the `x` close key and mouse input.

mod common;

use clio_viewer::app::{App, Mode};
use clio_viewer::keys::parse_keys;
use clio_viewer::layout::Zoom;
use clio_viewer::ui::snapshot;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

fn app() -> App {
    App::new(common::sample_graph(), None)
}

fn screen(a: &mut App, w: u16, h: u16, keys: &str) -> String {
    snapshot(a, w, h, &parse_keys(keys).unwrap()).unwrap()
}

fn sel(a: &App) -> &str {
    &a.graph.item(a.selected.unwrap()).id
}

fn find(a: &App, id: &str) -> usize {
    a.graph.find(id).unwrap()
}

fn fits(a: &App) -> bool {
    a.layout.width <= a.viewport.0 && a.layout.height <= a.viewport.1
}

#[test]
fn auto_fit_picks_most_detailed_zoom_that_fits() {
    let mut a = app();
    assert!(a.auto_fit, "on by default");
    let s = screen(&mut a, 150, 38, "");
    assert_eq!(a.zoom, Zoom::Detail);
    assert!(s.contains("zoom:detail (auto)"), "{s}");

    screen(&mut a, 150, 38, "e");
    assert_eq!(a.zoom, Zoom::Overview, "expanding everything shrinks the boxes");
    screen(&mut a, 150, 38, "E");
    assert_eq!(a.zoom, Zoom::Detail, "collapsing grows them back");

    screen(&mut a, 150, 38, "e");
    screen(&mut a, 600, 60, "");
    assert_eq!(a.zoom, Zoom::Detail, "a bigger terminal refits");
    assert!(fits(&a), "whole view shown");
    let (vw, vh) = a.viewport;
    assert_eq!(a.camera, (-(vw - a.layout.width) / 2, -(vh - a.layout.height) / 2), "and centred");
}

#[test]
fn auto_fit_shows_overflowing_views_from_the_top_left() {
    let mut a = app();
    screen(&mut a, 150, 38, "e");
    assert!(a.layout.width > 150 && a.layout.height <= 37);
    assert_eq!(a.camera.0, 0, "overflowing axis starts at the left");
    assert_eq!(a.camera.1, -(37 - a.layout.height) / 2, "fitting axis is centred");
}

#[test]
fn manual_zoom_turns_auto_fit_off_and_z_turns_it_back_on() {
    let mut a = app();
    let s = screen(&mut a, 150, 38, "-");
    assert!(!a.auto_fit && a.zoom == Zoom::Compact);
    assert!(s.contains("zoom:compact │") && s.contains("auto-fit off (z turns it back on)"), "{s}");
    screen(&mut a, 150, 38, "e");
    assert_eq!(a.zoom, Zoom::Compact, "no refit while off");
    let s = screen(&mut a, 150, 38, "z");
    assert!(a.auto_fit && a.zoom == Zoom::Overview, "{s}");
    assert!(s.contains("auto-fit on"));
}

#[test]
fn x_closes_outward_one_level_at_a_time() {
    let mut a = app();
    a.reveal(find(&a, "T1"));
    screen(&mut a, 200, 50, "x");
    assert_eq!(sel(&a), "S1");
    assert!(!a.expanded.contains(&find(&a, "S1")));
    screen(&mut a, 200, 50, "x");
    assert_eq!(sel(&a), "E1");
    screen(&mut a, 200, 50, "x");
    assert_eq!(sel(&a), "M1");
    assert!(a.expanded.is_empty());
    let s = screen(&mut a, 200, 50, "x");
    assert!(s.contains("nothing to close"), "{s}");
}

#[test]
fn x_on_an_open_box_closes_it_and_drills_out_at_the_top_of_a_view() {
    let mut a = app();
    screen(&mut a, 200, 50, "<space>x");
    assert_eq!(sel(&a), "M1");
    assert!(a.expanded.is_empty(), "the selected open box itself closes");

    screen(&mut a, 200, 50, "<enter>");
    assert_eq!((sel(&a), a.drill.len()), ("E1", 1));
    screen(&mut a, 200, 50, "x");
    assert!(a.drill.is_empty() && sel(&a) == "M1", "E1's container is the view: drill out");
}

fn click(a: &mut App, x: i32, y: i32) {
    let (cx, cy) = a.camera;
    a.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: (x - cx) as u16,
        row: (y - cy) as u16,
        modifiers: KeyModifiers::NONE,
    });
    screen(a, 200, 50, "");
}

#[test]
fn clicks_select_open_and_close_boxes() {
    let mut a = app();
    screen(&mut a, 200, 50, "");
    let m2 = a.layout.box_of(find(&a, "M2")).unwrap().bounds;
    click(&mut a, m2.x + 3, m2.y + 2);
    assert_eq!(sel(&a), "M2");
    assert!(a.expanded.is_empty(), "clicking the body only selects");

    // "[+2]" sits just left of the bottom-right corner.
    click(&mut a, m2.right() - 3, m2.bottom() - 1);
    assert!(a.expanded.contains(&find(&a, "M2")), "clicking [+N] opens");

    let e3 = a.layout.box_of(find(&a, "E3")).unwrap().bounds;
    click(&mut a, e3.x + 2, e3.y + 1);
    assert_eq!(sel(&a), "E3", "innermost box wins");

    let m2 = a.layout.box_of(find(&a, "M2")).unwrap().bounds;
    click(&mut a, m2.x + 6, m2.y);
    assert!(!a.expanded.contains(&find(&a, "M2")), "clicking an open box's title bar closes it");
    assert_eq!(sel(&a), "M2");
}

#[test]
fn wheel_pans_and_popups_ignore_the_mouse() {
    let mut a = app();
    screen(&mut a, 120, 30, "-e"); // compact, auto-fit off: too big for the screen
    let cam = a.camera;
    let wheel = |kind, modifiers| MouseEvent { kind, column: 5, row: 5, modifiers };
    a.handle_mouse(wheel(MouseEventKind::ScrollDown, KeyModifiers::NONE));
    assert_eq!(a.camera, (cam.0, cam.1 + 4));
    a.handle_mouse(wheel(MouseEventKind::ScrollDown, KeyModifiers::SHIFT));
    assert_eq!(a.camera, (cam.0 + 8, cam.1 + 4));

    a.mode = Mode::Help;
    let before = a.selected;
    a.handle_mouse(wheel(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE));
    assert_eq!((a.camera, a.selected), ((cam.0 + 8, cam.1 + 4), before));
}

#[test]
fn plus_tag_hit_area_is_exactly_the_tag() {
    let mut a = app();
    screen(&mut a, 200, 50, "");
    let m2 = a.layout.box_of(find(&a, "M2")).unwrap().bounds;
    let x1 = m2.right() - 1; // bottom-right corner; "[+2]" occupies x1-4 ..= x1-1
    click(&mut a, x1 - 5, m2.bottom() - 1);
    click(&mut a, x1, m2.bottom() - 1);
    assert!(a.expanded.is_empty(), "border cells beside the tag only select");
    click(&mut a, x1 - 4, m2.bottom() - 1);
    assert!(a.expanded.contains(&find(&a, "M2")), "the '[' opens");
}

#[test]
fn auto_fit_stretches_the_view_to_fill_the_terminal() {
    for (w, h) in [(160u16, 40u16), (250, 70)] {
        let mut a = app();
        screen(&mut a, w, h, "<space>");
        let (vw, vh) = a.viewport;
        assert!(a.layout.width <= vw && a.layout.height <= vh);
        assert!(a.layout.width * 10 >= vw * 9, "{w}x{h}: width {} of {vw}", a.layout.width);
        assert!(a.stretch.x > 0 && a.stretch.y > 0);
    }
    // Picking a zoom by hand drops the stretch.
    let mut a = app();
    screen(&mut a, 250, 70, "<space>+");
    assert_eq!((a.stretch.x, a.stretch.y), (0, 0));
}

fn screen_xy(a: &App, x: i32, y: i32) -> (i32, i32) {
    (x - a.camera.0, y - a.camera.1)
}

#[test]
fn double_right_click_closes_the_open_box_under_the_pointer() {
    use std::time::{Duration, Instant};
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);

    // Inside open E1 (its empty interior): E1 closes and is selected.
    let mut a = app();
    a.reveal(find(&a, "T1")); // opens M1, E1, S1
    a.selected = a.graph.find("M2");
    screen(&mut a, 200, 50, "");
    let e1 = a.layout.box_of(find(&a, "E1")).unwrap().bounds;
    let (sx, sy) = screen_xy(&a, e1.x + 1, e1.bottom() - 2);
    assert!(!a.right_press(sx, sy, ms(0)), "a single right-click does nothing");
    assert!(a.expanded.contains(&find(&a, "E1")));
    assert!(a.right_press(sx + 1, sy, ms(300)), "second press nearby and in time");
    assert!(!a.expanded.contains(&find(&a, "E1")));
    assert_eq!(sel(&a), "E1");
    assert!(a.expanded.contains(&find(&a, "M1")), "only the innermost open box closes");

    // On a closed box inside an open one: the box around it closes.
    let mut a = app();
    a.reveal(find(&a, "S3")); // opens M1, E1
    screen(&mut a, 200, 50, "");
    let s3 = a.layout.box_of(find(&a, "S3")).unwrap().bounds;
    let (sx, sy) = screen_xy(&a, s3.x + 3, s3.y + 1);
    a.right_press(sx, sy, ms(0));
    a.right_press(sx, sy, ms(100));
    assert!(!a.expanded.contains(&find(&a, "E1")) && sel(&a) == "E1");
}

#[test]
fn double_right_click_needs_two_quick_presses_in_one_place() {
    use std::time::{Duration, Instant};
    let t0 = Instant::now();
    let ms = |n| t0 + Duration::from_millis(n);
    let mut a = app();
    screen(&mut a, 200, 50, "<space>");
    let m1 = a.layout.box_of(find(&a, "M1")).unwrap().bounds;
    let (sx, sy) = screen_xy(&a, m1.x + 1, m1.y + 1);

    assert!(!a.right_press(sx, sy, ms(0)));
    assert!(!a.right_press(sx, sy, ms(500)), "too slow: starts a new pair");
    assert!(!a.right_press(sx + 5, sy, ms(600)), "too far away");
    assert!(a.expanded.contains(&find(&a, "M1")));
    assert!(a.right_press(sx + 5, sy, ms(700)));
    assert!(a.expanded.is_empty());
    assert!(!a.right_press(sx + 5, sy, ms(800)), "a third press starts over");
}

#[test]
fn double_right_click_on_empty_canvas_acts_like_x() {
    use std::time::{Duration, Instant};
    let t0 = Instant::now();
    let mut a = app();
    a.reveal(find(&a, "T1"));
    screen(&mut a, 200, 50, "");
    // Bottom-right corner of the canvas: no box there.
    a.right_press(199, 48, t0);
    a.right_press(199, 48, t0 + Duration::from_millis(50));
    assert_eq!(sel(&a), "S1", "T1's box (S1) closed, as `x` would");
    assert!(!a.expanded.contains(&find(&a, "S1")));
}

#[test]
fn right_presses_arrive_through_handle_mouse() {
    let mut a = app();
    screen(&mut a, 200, 50, "<space>");
    let m1 = a.layout.box_of(find(&a, "M1")).unwrap().bounds;
    let (sx, sy) = screen_xy(&a, m1.x + 1, m1.y + 1);
    let press = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: sx as u16,
        row: sy as u16,
        modifiers: KeyModifiers::NONE,
    };
    a.handle_mouse(press);
    a.handle_mouse(press);
    assert!(a.expanded.is_empty(), "two immediate right-presses close M1");
}
