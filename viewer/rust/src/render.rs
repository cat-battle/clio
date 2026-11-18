//! Draws a [`Layout`] into a ratatui buffer through a camera.

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::widgets::Widget;

use crate::layout::{Edge, Layout, PlacedBox, Zoom, header_text};
use crate::model::{Graph, Kind, NodeId};
use crate::theme::Theme;
use crate::trace::Trace;

/// Stable colour names for `--styles` dumps (they match the Python port's
/// colour strings so the two can be compared).
pub fn color_name(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Reset => "-".into(),
        Color::Red => "red".into(),
        Color::Cyan => "cyan".into(),
        Color::Magenta => "magenta".into(),
        Color::Blue => "blue".into(),
        Color::Green => "green".into(),
        Color::Yellow => "yellow".into(),
        Color::White => "white".into(),
        Color::Gray => "grey70".into(),
        Color::DarkGray => "grey50".into(),
        Color::LightYellow => "bright_yellow".into(),
        Color::LightCyan => "bright_cyan".into(),
        Color::LightMagenta => "bright_magenta".into(),
        _ => "?".into(),
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

/// Greedy word wrap of `text` into at most `max_lines` lines of `width`
/// characters. Words longer than a line are split; whatever does not fit is
/// put on the last line and truncated with `…`. With `max_lines == 1` this is
/// [`truncate`] of the whitespace-normalised text.
pub fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut out = Vec::new();
    if width == 0 || max_lines == 0 {
        return out;
    }
    let mut words: std::collections::VecDeque<String> = text.split_whitespace().map(str::to_string).collect();
    while let Some(first) = words.pop_front() {
        if out.len() + 1 == max_lines {
            let rest: Vec<String> = std::iter::once(first).chain(words.drain(..)).collect();
            out.push(truncate(&rest.join(" "), width));
            break;
        }
        let n = first.chars().count();
        if n > width {
            out.push(first.chars().take(width).collect());
            words.push_front(first.chars().skip(width).collect());
            continue;
        }
        let mut line = first;
        let mut len = n;
        while let Some(next) = words.front() {
            let m = next.chars().count();
            if len + 1 + m > width {
                break;
            }
            line.push(' ');
            line.push_str(next);
            len += 1 + m;
            words.pop_front();
        }
        out.push(line);
    }
    out
}

pub struct Canvas<'a> {
    pub graph: &'a Graph,
    pub layout: &'a Layout,
    pub camera: (i32, i32),
    pub selected: Option<NodeId>,
    pub zoom: Zoom,
    /// When tracing, boxes and edges outside the cone are dimmed.
    pub trace: Option<&'a Trace>,
    pub theme: &'a Theme,
}

struct Pen<'b> {
    buf: &'b mut Buffer,
    area: Rect,
    cam: (i32, i32),
    /// Background for styles that don't set one.
    bg: Color,
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
        let bg = self.bg;
        if let Some(c) = self.cell(x, y) {
            c.reset();
            c.set_symbol(sym).set_style(style);
            if style.bg.is_none() {
                c.set_bg(bg);
            }
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
        let role = self.trace.map(|t| t.role(b.node));
        let dimmed = role == Some(None);
        // Inside the trace, everything outside the cone is painted grey.
        let th = self.theme;
        let paint = |s: Style| if dimmed { Style::default().fg(th.faint) } else { s };
        let mut bstyle = Style::default().fg(th.status(item.status));
        if let Some(c) = role.flatten().and_then(|r| th.role(r)) {
            bstyle = Style::default().fg(c).add_modifier(Modifier::BOLD);
        }
        bstyle = paint(bstyle);
        if selected {
            bstyle = Style::default()
                .fg(th.selected_fg)
                .bg(th.selected_bg)
                .add_modifier(Modifier::BOLD);
        }
        let r = b.bounds;
        let (x0, y0, x1, y1) = (r.x, r.y, r.right() - 1, r.bottom() - 1);

        // Closed boxes are filled like cards; open containers (and boxes
        // greyed out by a trace) sit on the canvas background.
        let outer = th.background;
        let fill = if b.expanded || dimmed { outer } else { th.card };
        pen.bg = outer;
        let blank = Style::default().bg(fill);
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
        let bold = paint(Style::default().fg(th.text).add_modifier(Modifier::BOLD));
        let title_style = if selected { bstyle } else { bold };
        if b.expanded {
            let head = format!(" {} ", header_text(self.graph, b.node, self.zoom));
            pen.text(x0 + 2, y0, &head, inner - 5, title_style);
            pen.text(x1 - 3, y0, "[-]", 3, bstyle);
            return;
        }

        let kids = self.graph.children(b.node).len();
        let dim = Style::default().fg(if dimmed { th.faint } else { th.subtle });
        let plain = paint(Style::default().fg(th.body));
        pen.bg = fill;
        let rows = r.h - 2; // text rows inside the border
        let w = inner.max(0) as usize;
        match self.zoom {
            Zoom::Detail => {
                // id · kind / title (wrapped) / description (if room) / status
                let id = truncate(&item.id, inner as usize - 1);
                pen.text(x0 + 1, y0 + 1, &id, inner, bold);
                let kind = format!(" · {}", item.kind.label());
                let used = id.chars().count() as i32;
                pen.text(x0 + 1 + used, y0 + 1, &kind, inner - used, dim);
                let middle = (rows - 2).max(0) as usize;
                let mut lines = wrap(&item.title, w, middle);
                let title_lines = lines.len();
                if let Some(d) = &item.description {
                    lines.extend(wrap(d, w, middle - title_lines));
                }
                for (i, line) in lines.iter().enumerate() {
                    let style = if i < title_lines { plain } else { dim };
                    pen.text(x0 + 1, y0 + 2 + i as i32, line, inner, style);
                }
                let st = format!("{} {}", item.status.glyph(), item.status.label());
                let st_style = paint(Style::default().fg(th.status(item.status)));
                pen.text(x0 + 1, y1 - 1, &st, inner, st_style);
                if let Some(a) = &item.assignee {
                    let used = st.chars().count() as i32 + 1;
                    let who = truncate(&format!("@{a}"), (inner - used).max(0) as usize);
                    pen.text(x1 - who.chars().count() as i32, y1 - 1, &who, inner - used, dim);
                }
            }
            Zoom::Compact => {
                let text = format!("{} {}", item.id, item.title);
                for (i, line) in wrap(&text, w, rows.max(0) as usize).iter().enumerate() {
                    pen.text(x0 + 1, y0 + 1 + i as i32, line, inner, title_style);
                }
            }
            Zoom::Overview => {
                pen.text(x0 + 1, y0 + 1, &truncate(&item.id, inner as usize), inner, title_style);
            }
        }
        pen.bg = outer;
        if kids > 0 {
            let tag = match self.zoom {
                Zoom::Overview => "+".to_string(),
                _ => format!("[+{kids}]"),
            };
            let len = tag.chars().count() as i32;
            pen.text(x1 - len, y1, &tag, len, bstyle);
        }
    }

    /// Colour of an edge: its link type's, or grey when tracing and the edge
    /// is not part of the cone.
    fn edge_color(&self, e: &Edge) -> Color {
        match self.trace {
            Some(t) if !t.edge_traced(e.from, e.to, &e.kind) => self.theme.faint,
            _ => self.theme.link_color(self.graph.link_types(), &e.kind),
        }
    }

    fn draw_edge(&self, pen: &mut Pen, e: &Edge) {
        let (h, v, _) = self.theme.link(self.graph.link_types(), &e.kind);
        let color = self.edge_color(e);
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
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.reset();
                    c.set_bg(self.theme.background);
                }
            }
        }
        let mut pen = Pen {
            buf,
            area,
            cam: self.camera,
            bg: self.theme.background,
        };
        // Containers first, then wires, then leaf boxes on top of any wire that
        // happens to cross them, then the port marks on the borders.
        for b in self.layout.boxes.iter().filter(|b| b.expanded) {
            self.draw_box(&mut pen, b);
        }
        // Traced edges go last so they win where wires overlap.
        let (lit, grey): (Vec<&Edge>, Vec<&Edge>) = self
            .layout
            .edges
            .iter()
            .partition(|e| self.trace.is_none_or(|t| t.edge_traced(e.from, e.to, &e.kind)));
        for &e in grey.iter().chain(&lit) {
            self.draw_edge(&mut pen, e);
        }
        for b in self.layout.boxes.iter().filter(|b| !b.expanded) {
            self.draw_box(&mut pen, b);
        }
        for &e in grey.iter().chain(&lit) {
            let color = self.edge_color(e);
            pen.put_fg(e.from_port.0, e.from_port.1, "├", color);
            pen.put_fg(e.to_port.0, e.to_port.1, "┤", color);
        }
    }
}
