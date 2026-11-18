//! Colour themes. Every colour the viewer draws comes from a [`Theme`], so a
//! new palette is one more constant here (mirrored in the Python port's
//! `clio_viewer/theme.py`).
//!
//! * **mocha** (default): the Catppuccin Mocha true-colour palette, the same
//!   one the clik project uses.
//! * **classic**: the 16 standard ANSI colours, for terminals without true
//!   colour.

use ratatui::style::Color;

use crate::model::Status;
use crate::trace::Role;

/// (horizontal glyph, vertical glyph, colour) of one relationship type.
pub type LinkStyle = (&'static str, &'static str, Color);

#[derive(Debug, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    /// Ids and container titles (bold).
    pub text: Color,
    /// Box titles.
    pub body: Color,
    /// Secondary text: kinds, assignees, descriptions, status-bar separators.
    pub subtle: Color,
    /// Everything outside a dependency trace.
    pub faint: Color,
    /// Messages, the search prompt and key names in help.
    pub accent: Color,
    /// Border/status colour by [`Status`]: todo, in progress, done, blocked.
    pub status: [Color; 4],
    /// Relationship types in sorted order; repeats after six.
    pub links: [LinkStyle; 6],
    pub upstream: Color,
    pub downstream: Color,
    pub both: Color,
    pub selected_fg: Color,
    pub selected_bg: Color,
    /// Canvas and panel background (`Reset` = the terminal's own).
    pub background: Color,
    /// Fill of collapsed boxes ("cards").
    pub card: Color,
    /// Status bar background.
    pub bar: Color,
}

const fn hex(rgb: u32) -> Color {
    Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Catppuccin Mocha (https://catppuccin.com/palette), as in clik's theme.py.
pub const MOCHA: Theme = Theme {
    name: "mocha",
    text: hex(0xcdd6f4),    // text
    body: hex(0xbac2de),    // subtext1
    subtle: hex(0x7f849c),  // overlay1
    faint: hex(0x585b70),   // surface2
    accent: hex(0xf9e2af),  // yellow
    status: [
        hex(0x9399b2), // overlay2
        hex(0xf9e2af), // yellow
        hex(0xa6e3a1), // green
        hex(0xf38ba8), // red
    ],
    links: [
        ("─", "│", hex(0xf38ba8)), // red
        ("┄", "┆", hex(0x89dceb)), // sky
        ("╌", "╎", hex(0xcba6f7)), // mauve
        ("━", "┃", hex(0x89b4fa)), // blue
        ("─", "│", hex(0xa6e3a1)), // green
        ("┄", "┆", hex(0xfab387)), // peach
    ],
    upstream: hex(0x94e2d5),    // teal
    downstream: hex(0xf5c2e7),  // pink
    both: hex(0xfab387),        // peach
    selected_fg: hex(0x11111b), // crust
    selected_bg: hex(0x89b4fa), // blue
    // Same as Textual's built-in catppuccin-mocha theme (used by the Python port).
    background: hex(0x181825), // mantle
    card: hex(0x313244),       // surface0
    bar: hex(0x313244),        // surface0
};

/// The original 16-colour look.
pub const CLASSIC: Theme = Theme {
    name: "classic",
    text: Color::White,
    body: Color::Reset,
    subtle: Color::DarkGray,
    faint: Color::DarkGray,
    accent: Color::Yellow,
    status: [Color::Gray, Color::Yellow, Color::Green, Color::Red],
    links: [
        ("─", "│", Color::Red),
        ("┄", "┆", Color::Cyan),
        ("╌", "╎", Color::Magenta),
        ("━", "┃", Color::Blue),
        ("─", "│", Color::Green),
        ("┄", "┆", Color::LightYellow),
    ],
    upstream: Color::LightCyan,
    downstream: Color::LightMagenta,
    both: Color::LightYellow,
    selected_fg: Color::White,
    selected_bg: Color::Blue,
    background: Color::Reset,
    card: Color::Reset,
    bar: Color::Reset,
};

pub const THEMES: [&Theme; 2] = [&MOCHA, &CLASSIC];

impl Theme {
    pub fn by_name(name: &str) -> Option<&'static Theme> {
        THEMES.into_iter().find(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// `--theme` if given, else `$CLIO_THEME`, else mocha on true-colour
    /// terminals (`$COLORTERM` = truecolor / 24bit) and classic elsewhere.
    pub fn resolve(
        flag: Option<&str>,
        env_theme: Option<&str>,
        colorterm: Option<&str>,
    ) -> Result<&'static Theme, String> {
        if let Some(name) = flag.or(env_theme).filter(|n| !n.is_empty()) {
            return Self::by_name(name).ok_or_else(|| {
                let names: Vec<&str> = THEMES.iter().map(|t| t.name).collect();
                format!("unknown theme {name:?} (choose from: {})", names.join(", "))
            });
        }
        let truecolor = colorterm.is_some_and(|c| matches!(c.to_ascii_lowercase().as_str(), "truecolor" | "24bit"));
        Ok(if truecolor { &MOCHA } else { &CLASSIC })
    }

    /// The theme after this one (for the `C` key).
    pub fn next(&self) -> &'static Theme {
        let i = THEMES.iter().position(|t| *t == self).unwrap_or(0);
        THEMES[(i + 1) % THEMES.len()]
    }

    pub fn status(&self, s: Status) -> Color {
        self.status[match s {
            Status::Todo => 0,
            Status::InProgress => 1,
            Status::Done => 2,
            Status::Blocked => 3,
        }]
    }

    /// Style of `kind`, by its index in the graph's sorted link types.
    pub fn link(&self, types: &[String], kind: &str) -> LinkStyle {
        let i = types.iter().position(|t| t == kind).unwrap_or(0);
        self.links[i % self.links.len()]
    }

    pub fn link_color(&self, types: &[String], kind: &str) -> Color {
        self.link(types, kind).2
    }

    /// Border colour of a traced box (None for the focus: keeps its own).
    pub fn role(&self, r: Role) -> Option<Color> {
        match r {
            Role::Focus => None,
            Role::Upstream => Some(self.upstream),
            Role::Downstream => Some(self.downstream),
            Role::Both => Some(self.both),
        }
    }
}
