//! Colour themes.

mod common;

use clio_viewer::app::App;
use clio_viewer::keys::parse_keys;
use clio_viewer::model::Status;
use clio_viewer::theme::{CLASSIC, MOCHA, Theme};
use clio_viewer::ui::snapshot_styles;
use ratatui::style::Color;

fn styles(theme: &'static Theme, keys: &str) -> (String, String) {
    let mut a = App::new(common::sample_graph(), None);
    a.theme = theme;
    snapshot_styles(&mut a, 160, 40, &parse_keys(keys).unwrap()).unwrap()
}

#[test]
fn resolve_prefers_flag_then_env_then_truecolor_detection() {
    assert_eq!(Theme::resolve(Some("classic"), Some("mocha"), Some("truecolor")), Ok(&CLASSIC));
    assert_eq!(Theme::resolve(None, Some("Classic"), Some("truecolor")), Ok(&CLASSIC));
    assert_eq!(Theme::resolve(None, Some(""), Some("24bit")), Ok(&MOCHA), "empty env is ignored");
    assert_eq!(Theme::resolve(None, None, Some("truecolor")), Ok(&MOCHA));
    assert_eq!(Theme::resolve(None, None, None), Ok(&CLASSIC), "no true colour: 16-colour theme");
    let err = Theme::resolve(Some("latte"), None, None).unwrap_err();
    assert!(err.contains("mocha, classic"), "{err}");
}

#[test]
fn mocha_is_the_clik_catppuccin_palette() {
    assert_eq!(MOCHA.status(Status::Blocked), Color::Rgb(0xf3, 0x8b, 0xa8));
    assert_eq!(MOCHA.status(Status::Done), Color::Rgb(0xa6, 0xe3, 0xa1));
    assert_eq!(MOCHA.selected_bg, Color::Rgb(0x89, 0xb4, 0xfa));
    let (_, styles) = styles(&MOCHA, "");
    assert!(styles.contains("#11111b/#89b4fa/1"), "selected box: crust on blue");
    assert!(styles.contains("#f38ba8/#181825/0"), "blocks wires in red, on the mocha background");
    assert!(!styles.contains("/-/"), "every canvas cell has a background");
    assert!(!styles.contains("grey") && !styles.contains("/blue/"), "no ANSI colours left");
}

#[test]
fn classic_keeps_the_ansi_look() {
    let (_, styles) = styles(&CLASSIC, "");
    assert!(styles.contains("white/blue/1") && styles.contains("red/-/0"));
    assert!(!styles.contains('#'));
}

#[test]
fn trace_dims_with_the_theme_faint_colour() {
    let (_, styles) = styles(&MOCHA, "<space>t");
    assert!(styles.contains("#585b70"), "out-of-trace boxes use surface2");
    assert!(styles.contains("#f5c2e7"), "downstream boxes use pink");
}

#[test]
fn shift_c_cycles_themes() {
    let mut a = App::new(common::sample_graph(), None);
    assert_eq!(a.theme, &MOCHA);
    let (text, _) = snapshot_styles(&mut a, 160, 40, &parse_keys("C").unwrap()).unwrap();
    assert_eq!(a.theme, &CLASSIC);
    assert!(text.contains("theme: classic"));
    snapshot_styles(&mut a, 160, 40, &parse_keys("C").unwrap()).unwrap();
    assert_eq!(a.theme, &MOCHA);
}

#[test]
fn mocha_paints_the_background_and_fills_cards() {
    let (_, mocha) = styles(&MOCHA, "<space>");
    assert!(mocha.contains("-/#181825/0"), "canvas background (mantle)");
    assert!(mocha.contains("-/#313244/0"), "closed boxes are filled (surface0)");
    let (_, classic) = styles(&CLASSIC, "<space>");
    assert!(!classic.contains('#'), "classic leaves the terminal background alone");
}
