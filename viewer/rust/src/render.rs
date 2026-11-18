//! Draws a [`Layout`] into a ratatui buffer through a camera.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::widgets::Widget;

use crate::layout::{Edge, Layout, PlacedBox, Zoom, header_text};
use crate::model::{Graph, Kind, NodeId, Status};

/// (horizontal glyph, vertical glyph, colour) per link type, by index into the
/// graph's sorted link types.
const LINK_PALETTE: [(&str, &str, Color); 6] = [
    ("─", "│", Color::Red),
    ("┄", "┆", Color::Cyan),
    ("╌", "╎", Color::Magenta),
    ("━", "┃", Color::Blue),
    ("─", "│", Color::Green),
    ("┄", "┆", Color::LightYellow),
];

fn link_style(types: &[String], kind: &str) -> (&'static str, &'static str, Color) {
    let i = types.iter().position(|t| t == kind).unwrap_or(0);
    LINK_PALETTE[i % LINK_PALETTE.len()]
}

pub fn link_color(types: &[String], kind: &str) -> Color {
    link_style(types, kind).2
}

pub fn status_color(s: Status) -> Color {
    match s {
        Status::Todo => Color::Gray,
        Status::InProgress => Color::Yellow,
        Status::Done => Color::Green,
        Status::Blocked => Color::Red,
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

pub struct Canvas<'a> {
    pub graph: &'a Graph,
    pub layout: &'a Layout,
    pub camera: (i32, i32),
    pub selected: Option<NodeId>,
    pub zoom: Zoom,
}

struct Pen<'b> {
    buf: &'b mut Buffer,
    area: Rect,
    cam: (i32, i32),
}

impl Pen<'_> {
    fn cell(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        let sx = x - self.cam.0;
        let sy = y - self.cam.1;
        if sx < 0 || sy < 0 || sx >= self.area.width as i32 || sy >= self.area.height as i32 {
            return None;
        }
        self.buf
            .cell_mut((self.area.x + sx as u16, self.area.y + sy as u16))
    }

    fn put(&mut self, x: i32, y: i32, sym: &str, style: Style) {
        if let Some(c) = self.cell(x, y) {
            c.set_symbol(sym).set_style(style);
        }
    }

    fn put_fg(&mut self, x: i32, y: i32, sym: &str, fg: Color) {
        if let Some(c) = self.cell(x, y) {
            c.set_symbol(sym).set_fg(fg);
        }
    }

    fn text(&mut self, x: i32, y: i32, s: &str, max: i32, style: Style) {
        let mut tmp = [0u8; 4];
        for (i, ch) in s.chars().take(max.max(0) as usize).enumerate() {
            self.put(x + i as i32, y, ch.encode_utf8(&mut tmp), style);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
    Up,
    Down,
}

fn dir(a: (i32, i32), b: (i32, i32)) -> Dir {
    if b.0 > a.0 {
        Dir::Right
    } else if b.0 < a.0 {
        Dir::Left
    } else if b.1 > a.1 {
        Dir::Down
    } else {
        Dir::Up
    }
}

fn corner(d1: Dir, d2: Dir, h: &'static str, v: &'static str) -> &'static str {
    use Dir::*;
    match (d1, d2) {
        (Right, Down) | (Up, Left) => "┐",
        (Right, Up) | (Down, Left) => "┘",
        (Left, Down) | (Up, Right) => "┌",
        (Left, Up) | (Down, Right) => "└",
        (Left | Right, _) => h,
        (Up | Down, _) => v,
    }
}

impl Canvas<'_> {
    fn draw_box(&self, pen: &mut Pen, b: &PlacedBox) {
        let item = self.graph.item(b.node);
        let set = match item.kind {
            Kind::Milestone => border::DOUBLE,
            Kind::Epic => border::THICK,
            Kind::Story => border::ROUNDED,
            Kind::Subtask => border::PLAIN,
        };
        let selected = self.selected == Some(b.node);
        let mut bstyle = Style::default().fg(status_color(item.status));
        if selected {
            bstyle = Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD);
        }
        let r = b.bounds;
        let (x0, y0, x1, y1) = (r.x, r.y, r.right() - 1, r.bottom() - 1);

        let blank = Style::default();
        for y in y0 + 1..y1 {
            for x in x0 + 1..x1 {
                pen.put(x, y, " ", blank);
            }
        }
        for x in x0 + 1..x1 {
            pen.put(x, y0, set.horizontal_top, bstyle);
            pen.put(x, y1, set.horizontal_bottom, bstyle);
        }
        for y in y0 + 1..y1 {
            pen.put(x0, y, set.vertical_left, bstyle);
            pen.put(x1, y, set.vertical_right, bstyle);
        }
        pen.put(x0, y0, set.top_left, bstyle);
        pen.put(x1, y0, set.top_right, bstyle);
        pen.put(x0, y1, set.bottom_left, bstyle);
        pen.put(x1, y1, set.bottom_right, bstyle);

        let inner = r.w - 2;
        let bold = Style::default().fg(Color::White).add_modifier(Modifier::BOLD);
        let title_style = if selected { bstyle } else { bold };
        if b.expanded {
            let head = format!(" {} ", header_text(self.graph, b.node, self.zoom));
            pen.text(x0 + 2, y0, &head, inner - 5, title_style);
            pen.text(x1 - 3, y0, "[-]", 3, bstyle);
            return;
        }

        let kids = self.graph.children(b.node).len();
        let dim = Style::default().fg(Color::DarkGray);
        match self.zoom {
            Zoom::Detail => {
                let id = truncate(&item.id, inner as usize - 1);
                pen.text(x0 + 1, y0 + 1, &id, inner, bold);
                let kind = format!(" · {}", item.kind.label());
                let used = id.chars().count() as i32;
                pen.text(x0 + 1 + used, y0 + 1, &kind, inner - used, dim);
                pen.text(x0 + 1, y0 + 2, &truncate(&item.title, inner as usize), inner, Style::default());
                let st = format!("{} {}", item.status.glyph(), item.status.label());
                let st_style = Style::default().fg(status_color(item.status));
                pen.text(x0 + 1, y0 + 3, &st, inner, st_style);
                if let Some(a) = &item.assignee {
                    let used = st.chars().count() as i32 + 1;
                    let who = truncate(&format!("@{a}"), (inner - used).max(0) as usize);
                    pen.text(x1 - who.chars().count() as i32, y0 + 3, &who, inner - used, dim);
                }
            }
            Zoom::Compact => {
                let s = truncate(&format!("{} {}", item.id, item.title), inner as usize);
                pen.text(x0 + 1, y0 + 1, &s, inner, title_style);
            }
            Zoom::Overview => {
                pen.text(x0 + 1, y0 + 1, &truncate(&item.id, inner as usize), inner, title_style);
            }
        }
        if kids > 0 {
            let tag = match self.zoom {
                Zoom::Overview => "+".to_string(),
                _ => format!("[+{kids}]"),
            };
            let len = tag.chars().count() as i32;
            pen.text(x1 - len, y1, &tag, len, bstyle);
        }
    }

    fn draw_edge(&self, pen: &mut Pen, e: &Edge) {
        let (h, v, color) = link_style(self.graph.link_types(), &e.kind);
        let pts = &e.points;
        if pts.is_empty() {
            return;
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.1 == b.1 {
                for x in a.0.min(b.0)..=a.0.max(b.0) {
                    pen.put_fg(x, a.1, h, color);
                }
            } else {
                for y in a.1.min(b.1)..=a.1.max(b.1) {
                    pen.put_fg(a.0, y, v, color);
                }
            }
        }
        for i in 1..pts.len().saturating_sub(1) {
            let c = corner(dir(pts[i - 1], pts[i]), dir(pts[i], pts[i + 1]), h, v);
            pen.put_fg(pts[i].0, pts[i].1, c, color);
        }
        let last = pts[pts.len() - 1];
        let arrow = if pts.len() >= 2 {
            match dir(pts[pts.len() - 2], last) {
                Dir::Right => "▶",
                Dir::Left => "◀",
                Dir::Down => "▼",
                Dir::Up => "▲",
            }
        } else {
            "▶"
        };
        pen.put_fg(last.0, last.1, arrow, color);
        if e.count > 1 {
            let label = format!("×{}", e.count);
            let style = Style::default().fg(color).add_modifier(Modifier::BOLD);
            pen.text(pts[0].0, pts[0].1, &label, 4, style);
        }
    }
}

impl Widget for Canvas<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut pen = Pen {
            buf,
            area,
            cam: self.camera,
        };
        // Containers first, then wires, then leaf boxes on top of any wire that
        // happens to cross them, then the port marks on the borders.
        for b in self.layout.boxes.iter().filter(|b| b.expanded) {
            self.draw_box(&mut pen, b);
        }
        for e in &self.layout.edges {
            self.draw_edge(&mut pen, e);
        }
        for b in self.layout.boxes.iter().filter(|b| !b.expanded) {
            self.draw_box(&mut pen, b);
        }
        for e in &self.layout.edges {
            let color = link_color(self.graph.link_types(), &e.kind);
            pen.put_fg(e.from_port.0, e.from_port.1, "├", color);
            pen.put_fg(e.to_port.0, e.to_port.1, "┤", color);
        }
    }
}
