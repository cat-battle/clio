//! Frame composition: canvas, status bar, details panel and popups.

use anyhow::Result;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyEvent;
use ratatui::layout::{Constraint, Layout as Split, Rect};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::{App, Mode};
use crate::render::{Canvas, color_name, link_color, role_color, status_color};
use crate::trace::{Role, Trace, TraceMode};

const DETAILS_WIDTH: u16 = 42;

pub fn draw(f: &mut Frame, app: &mut App) {
    let [main, status] = Split::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
    let canvas_area = if app.show_details {
        let [c, d] =
            Split::horizontal([Constraint::Min(10), Constraint::Length(DETAILS_WIDTH)]).areas(main);
        draw_details(f, app, d);
        c
    } else {
        main
    };

    app.set_viewport(canvas_area.width, canvas_area.height);
    let trace = app.trace();
    f.render_widget(
        Canvas {
            graph: &app.graph,
            layout: &app.layout,
            camera: app.camera,
            selected: app.selected,
            zoom: app.zoom,
            trace: trace.as_ref(),
        },
        canvas_area,
    );
    draw_status(f, app, trace.as_ref(), status);

    match &app.mode {
        Mode::Relations { cursor } => draw_relations(f, app, *cursor),
        Mode::Help => draw_help(f),
        Mode::Normal | Mode::Search { .. } => {}
    }
}

fn draw_status(f: &mut Frame, app: &App, trace: Option<&Trace>, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = Vec::new();
    if let Mode::Search { query } = &app.mode {
        spans.push(Span::styled("/", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(query.clone()));
        spans.push(Span::styled("█", Style::default().fg(Color::Yellow)));
        spans.push(Span::styled("   enter: find · esc: cancel", dim));
        f.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    spans.push(Span::styled(" Top", Style::default().add_modifier(Modifier::BOLD)));
    for &n in &app.drill {
        spans.push(Span::styled(" › ", dim));
        spans.push(Span::styled(
            app.graph.item(n).id.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(format!("  │ zoom:{} │", app.zoom.label()), dim));
    for t in app.graph.link_types() {
        if app.enabled.contains(t) {
            let c = link_color(app.graph.link_types(), t);
            spans.push(Span::styled(format!(" ■ {t}"), Style::default().fg(c)));
        } else {
            spans.push(Span::styled(format!(" □ {t}"), dim));
        }
    }
    if app.layout.hidden_links > 0 {
        spans.push(Span::styled(
            format!("  ({} links leave this view)", app.layout.hidden_links),
            dim,
        ));
    }
    if let Some(t) = trace {
        let g = &app.graph;
        spans.push(Span::styled(format!("  │ trace:{}", app.trace_mode.label()), dim));
        let color = |r| Style::default().fg(role_color(r).unwrap_or(Color::Reset));
        if app.trace_mode != TraceMode::Downstream {
            let n = Trace::outermost(g, &t.upstream).len();
            spans.push(Span::styled(format!(" ↑{n}"), color(Role::Upstream)));
        }
        if app.trace_mode != TraceMode::Upstream {
            let n = Trace::outermost(g, &t.downstream).len();
            spans.push(Span::styled(format!(" ↓{n}"), color(Role::Downstream)));
        }
        let hidden = t.hidden(g, &app.layout).len();
        if hidden > 0 {
            spans.push(Span::styled(format!(" ({hidden} hidden, T shows)"), dim));
        }
    }
    match &app.message {
        Some(m) => spans.push(Span::styled(format!("  │ {m}"), Style::default().fg(Color::Yellow))),
        None => spans.push(Span::styled("  │ ?: help", dim)),
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_details(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered().title(" Details ");
    let Some(n) = app.selected else {
        f.render_widget(Paragraph::new("nothing selected").block(block), area);
        return;
    };
    let g = &app.graph;
    let it = g.item(n);
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(it.id.clone(), bold),
            Span::styled(format!(" · {}", it.kind.label()), dim),
        ]),
        Line::from(it.title.clone()),
        Line::from(""),
        Line::from(vec![
            Span::styled("Status   ", dim),
            Span::styled(
                format!("{} {}", it.status.glyph(), it.status.label()),
                Style::default().fg(status_color(it.status)),
            ),
        ]),
        Line::from(vec![
            Span::styled("Assignee ", dim),
            Span::raw(it.assignee.clone().unwrap_or_else(|| "—".into())),
        ]),
    ];
    let path: Vec<String> = g.path_from_root(n).iter().map(|&p| g.item(p).id.clone()).collect();
    lines.push(Line::from(vec![Span::styled("Path     ", dim), Span::raw(path.join(" › "))]));
    lines.push(Line::from(vec![
        Span::styled("Children ", dim),
        Span::raw(g.children(n).len().to_string()),
    ]));
    if let Some(d) = &it.description {
        lines.push(Line::from(""));
        lines.extend(d.lines().map(|l| Line::from(l.to_string())));
    }
    let outgoing: Vec<_> = g.links().iter().filter(|l| l.from == n).collect();
    let incoming: Vec<_> = g.links().iter().filter(|l| l.to == n).collect();
    for (title, list, arrow) in [("Links out", &outgoing, "→"), ("Links in", &incoming, "←")] {
        if list.is_empty() {
            continue;
        }
        lines.push(Line::from(""));
        lines.push(Line::styled(title, bold));
        for l in list.iter() {
            let other = if l.from == n { l.to } else { l.from };
            let c = link_color(g.link_types(), &l.kind);
            lines.push(Line::from(vec![
                Span::styled(format!(" {} {arrow} ", l.kind), Style::default().fg(c)),
                Span::raw(format!("{} {}", g.item(other).id, g.item(other).title)),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines).block(block).wrap(Wrap { trim: false }), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_relations(f: &mut Frame, app: &App, cursor: usize) {
    let types = app.graph.link_types();
    let mut lines: Vec<Line> = Vec::new();
    if types.is_empty() {
        lines.push(Line::styled("(no relationships in this data)", Style::default().fg(Color::DarkGray)));
    }
    for (i, t) in types.iter().enumerate() {
        let on = app.enabled.contains(t);
        let count = app.graph.links().iter().filter(|l| &l.kind == t).count();
        let mut style = Style::default().fg(link_color(types, t));
        if i == cursor {
            style = style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::styled(
            format!(" [{}] {t:<20} {count:>4} ", if on { "x" } else { " " }),
            style,
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled(
        "space toggle · a all · n none · esc close",
        Style::default().fg(Color::DarkGray),
    ));
    let area = centered(f.area(), 46, lines.len() as u16 + 2);
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Relationships ")),
        area,
    );
}

pub const HELP: &[(&str, &str)] = &[
    ("arrows / hjkl", "move selection (→/↓ on open box: enter)"),
    ("Shift+arrows / HJKL", "pan the camera"),
    ("+ / -", "zoom in / out (detail level)"),
    ("space", "expand / collapse box in place"),
    ("enter", "drill into box"),
    ("esc / backspace", "drill out"),
    ("e / E", "expand all / collapse all"),
    ("p", "select parent box"),
    ("c", "center on selection"),
    ("r", "relationship filters"),
    ("d", "details panel"),
    ("t", "trace deps: both / upstream / downstream / off"),
    ("T", "expand to show the whole trace"),
    ("/ , n", "search id/title, next match"),
    ("R", "reload data source"),
    ("q", "quit"),
];

fn draw_help(f: &mut Frame) {
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!(" {k:<22}"), Style::default().fg(Color::Yellow)),
                Span::raw(*v),
            ])
        })
        .collect();
    let area = centered(f.area(), 60, lines.len() as u16 + 2);
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Keys (any key closes) ")),
        area,
    );
}

/// Renders the app headlessly at `w`×`h` after replaying `keys`, returning
/// the screen as plain text. Used by `--snapshot` and the UI tests.
pub fn snapshot(app: &mut App, w: u16, h: u16, keys: &[KeyEvent]) -> Result<String> {
    Ok(render_headless(app, w, h, keys)?.0)
}

/// Like [`snapshot`], plus one line per canvas row (all rows but the status
/// bar) describing cell styles as run-length `N*fg/bg/b` tokens. Blank cells
/// only report their background.
pub fn snapshot_styles(app: &mut App, w: u16, h: u16, keys: &[KeyEvent]) -> Result<(String, String)> {
    render_headless(app, w, h, keys)
}

fn render_headless(app: &mut App, w: u16, h: u16, keys: &[KeyEvent]) -> Result<(String, String)> {
    let mut terminal = Terminal::new(TestBackend::new(w, h))?;
    terminal.draw(|f| draw(f, app))?;
    for &k in keys {
        app.handle_key(k);
        terminal.draw(|f| draw(f, app))?;
    }
    let buf = terminal.backend().buffer();
    let mut out = String::with_capacity((w as usize + 1) * h as usize);
    for y in 0..h {
        for x in 0..w {
            out.push_str(buf.cell((x, y)).map_or(" ", |c| c.symbol()));
        }
        out.push('\n');
    }
    Ok((out, style_dump(buf, w, h.saturating_sub(1))))
}

fn style_dump(buf: &Buffer, w: u16, rows: u16) -> String {
    let mut out = String::new();
    for y in 0..rows {
        let mut runs: Vec<(String, usize)> = Vec::new();
        for x in 0..w {
            let Some(c) = buf.cell((x, y)) else { continue };
            let blank = c.symbol() == " ";
            let tok = format!(
                "{}/{}/{}",
                if blank { "-" } else { color_name(c.fg) },
                color_name(c.bg),
                u8::from(!blank && c.modifier.contains(Modifier::BOLD)),
            );
            match runs.last_mut() {
                Some((t, n)) if *t == tok => *n += 1,
                _ => runs.push((tok, 1)),
            }
        }
        let line: Vec<String> = runs.iter().map(|(t, n)| format!("{n}*{t}")).collect();
        out.push_str(&line.join(" "));
        out.push('\n');
    }
    out
}
