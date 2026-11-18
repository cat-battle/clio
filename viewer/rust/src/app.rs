//! Viewer state and input handling. Rendering lives in [`crate::ui`].

use std::collections::HashSet;

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::adapters::Loader;
use crate::layout::{self, Layout, LayoutParams, Zoom};
use crate::model::{Graph, NodeId};
use crate::trace::{Trace, TraceMode};

const PAN_X: i32 = 8;
const PAN_Y: i32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Help,
    Relations { cursor: usize },
    Search { query: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

pub struct App {
    pub graph: Graph,
    source: Option<Box<dyn Loader>>,
    pub expanded: HashSet<NodeId>,
    /// Drill-in stack; the last entry is the current view root.
    pub drill: Vec<NodeId>,
    pub zoom: Zoom,
    pub enabled: HashSet<String>,
    pub selected: Option<NodeId>,
    pub camera: (i32, i32),
    pub viewport: (i32, i32),
    pub layout: Layout,
    pub mode: Mode,
    pub show_details: bool,
    pub trace_mode: TraceMode,
    /// Pick the most detailed zoom level at which the whole view fits.
    pub auto_fit: bool,
    pub message: Option<String>,
    pub last_search: Option<String>,
    pub should_quit: bool,
    follow_pending: bool,
    center_pending: bool,
    fit_pending: bool,
}

impl App {
    pub fn new(graph: Graph, source: Option<Box<dyn Loader>>) -> Self {
        let enabled = graph.link_types().iter().cloned().collect();
        let mut app = Self {
            graph,
            source,
            expanded: HashSet::new(),
            drill: Vec::new(),
            zoom: Zoom::default(),
            enabled,
            selected: None,
            camera: (0, 0),
            viewport: (80, 24),
            layout: Layout::default(),
            mode: Mode::Normal,
            show_details: false,
            trace_mode: TraceMode::Off,
            auto_fit: true,
            message: None,
            last_search: None,
            should_quit: false,
            follow_pending: true,
            center_pending: false,
            fit_pending: true,
        };
        app.relayout();
        app
    }

    pub fn source_description(&self) -> Option<String> {
        self.source.as_ref().map(|s| s.describe())
    }

    pub fn root(&self) -> Option<NodeId> {
        self.drill.last().copied()
    }

    pub fn relayout(&mut self) {
        self.layout = layout::compute(
            &self.graph,
            &LayoutParams {
                root: self.root(),
                expanded: &self.expanded,
                zoom: self.zoom,
                enabled: &self.enabled,
            },
        );
        self.fix_selection();
        self.follow_pending = true;
        self.fit_pending = true;
    }

    /// Moves the selection to its nearest visible ancestor (or the first box)
    /// when it is not visible in the current layout.
    fn fix_selection(&mut self) {
        if !self.selected.is_some_and(|s| self.layout.visible(s)) {
            self.selected = self
                .selected
                .and_then(|s| {
                    self.graph
                        .ancestors(s)
                        .into_iter()
                        .find(|&a| self.layout.visible(a))
                })
                .or_else(|| self.layout.boxes.first().map(|b| b.node));
        }
    }

    fn layout_at(&self, zoom: Zoom) -> Layout {
        layout::compute(
            &self.graph,
            &LayoutParams {
                root: self.root(),
                expanded: &self.expanded,
                zoom,
                enabled: &self.enabled,
            },
        )
    }

    fn fits(&self, l: &Layout) -> bool {
        l.width <= self.viewport.0 && l.height <= self.viewport.1
    }

    /// Auto-fit: switches to the most detailed zoom level at which the whole
    /// view fits the viewport (overview if nothing fits). When it fits, the
    /// camera shows all of it (see `set_viewport`).
    fn fit(&mut self) {
        let old = self.zoom;
        for z in [Zoom::Detail, Zoom::Compact, Zoom::Overview] {
            let l = if z == self.zoom {
                std::mem::take(&mut self.layout)
            } else {
                self.layout_at(z)
            };
            if self.fits(&l) || z == Zoom::Overview {
                self.zoom = z;
                self.layout = l;
                break;
            }
            if z == old {
                self.layout = l; // keep it if we break out later at old zoom
            }
        }
        self.fix_selection();
        if self.zoom != old {
            // Start from the top-left and scroll only as far as the
            // selection needs.
            self.camera = (0, 0);
            self.follow_pending = true;
        }
    }

    /// Called by the UI with the canvas size before each frame; applies any
    /// pending camera adjustment.
    pub fn set_viewport(&mut self, w: u16, h: u16) {
        let size = (w as i32, h as i32);
        let resized = size != self.viewport;
        self.viewport = size;
        if self.auto_fit && (std::mem::take(&mut self.fit_pending) || resized) {
            self.fit();
        }
        if std::mem::take(&mut self.center_pending) {
            self.follow_pending = false;
            self.center_on_selected();
        } else if std::mem::take(&mut self.follow_pending) {
            self.ensure_selected_visible();
        }
        // With auto-fit, an axis on which the whole view fits never scrolls.
        if self.auto_fit {
            if self.layout.width <= self.viewport.0 {
                self.camera.0 = 0;
            }
            if self.layout.height <= self.viewport.1 {
                self.camera.1 = 0;
            }
        }
    }

    fn clamp_camera(&mut self) {
        let (vw, vh) = self.viewport;
        let max_x = (self.layout.width - vw / 2).max(-vw / 2);
        let max_y = (self.layout.height - vh / 2).max(-vh / 2);
        self.camera.0 = self.camera.0.clamp(-vw / 2, max_x);
        self.camera.1 = self.camera.1.clamp(-vh / 2, max_y);
    }

    pub fn center_on_selected(&mut self) {
        if let Some(b) = self.selected.and_then(|s| self.layout.box_of(s)) {
            let (cx, cy) = b.bounds.center();
            self.camera = (cx - self.viewport.0 / 2, cy - self.viewport.1 / 2);
            self.clamp_camera();
        }
    }

    fn ensure_selected_visible(&mut self) {
        let Some(b) = self.selected.and_then(|s| self.layout.box_of(s)) else {
            return;
        };
        let r = b.bounds;
        let (vw, vh) = self.viewport;
        let (cx, cy) = &mut self.camera;
        if r.w + 2 > vw || r.x - 1 < *cx {
            *cx = r.x - 1;
        } else if r.right() + 1 > *cx + vw {
            *cx = r.right() + 1 - vw;
        }
        if r.h + 2 > vh || r.y - 1 < *cy {
            *cy = r.y - 1;
        } else if r.bottom() + 1 > *cy + vh {
            *cy = r.bottom() + 1 - vh;
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        match self.mode {
            Mode::Normal => {
                self.message = None;
                self.key_normal(key);
            }
            Mode::Help => self.mode = Mode::Normal,
            Mode::Relations { .. } => self.key_relations(key),
            Mode::Search { .. } => self.key_search(key),
        }
    }

    fn key_normal(&mut self, key: KeyEvent) {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left if shift => self.pan(-PAN_X, 0),
            KeyCode::Right if shift => self.pan(PAN_X, 0),
            KeyCode::Up if shift => self.pan(0, -PAN_Y),
            KeyCode::Down if shift => self.pan(0, PAN_Y),
            KeyCode::Char('H') => self.pan(-PAN_X, 0),
            KeyCode::Char('L') => self.pan(PAN_X, 0),
            KeyCode::Char('K') => self.pan(0, -PAN_Y),
            KeyCode::Char('J') => self.pan(0, PAN_Y),
            KeyCode::Left | KeyCode::Char('h') => self.move_selection(Dir::Left),
            KeyCode::Right | KeyCode::Char('l') => self.move_selection(Dir::Right),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(Dir::Up),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(Dir::Down),
            KeyCode::Char('+') | KeyCode::Char('=') => self.manual_zoom(self.zoom.zoom_in()),
            KeyCode::Char('-') | KeyCode::Char('_') => self.manual_zoom(self.zoom.zoom_out()),
            KeyCode::Char('z') => self.toggle_auto_fit(),
            KeyCode::Char('x') => self.close_enclosing(),
            KeyCode::Char(' ') => self.toggle_expand(),
            KeyCode::Enter => self.drill_in(),
            KeyCode::Esc | KeyCode::Backspace => self.drill_out(),
            KeyCode::Char('e') => self.expand_all(),
            KeyCode::Char('E') => self.collapse_all(),
            KeyCode::Char('p') => self.select_parent(),
            KeyCode::Char('c') => self.center_pending = true,
            KeyCode::Char('r') => self.mode = Mode::Relations { cursor: 0 },
            KeyCode::Char('R') => self.reload(),
            KeyCode::Char('d') => self.show_details = !self.show_details,
            KeyCode::Char('t') => self.trace_mode = self.trace_mode.next(),
            KeyCode::Char('T') => self.reveal_trace(),
            KeyCode::Char('/') => {
                self.mode = Mode::Search {
                    query: String::new(),
                }
            }
            KeyCode::Char('n') => match self.last_search.clone() {
                Some(q) => self.search(&q),
                None => self.message = Some("no previous search".into()),
            },
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    fn key_relations(&mut self, key: KeyEvent) {
        let types = self.graph.link_types().to_vec();
        let Mode::Relations { cursor } = &mut self.mode else {
            return;
        };
        let mut changed = false;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => *cursor = cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if *cursor + 1 < types.len() {
                    *cursor += 1;
                }
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(t) = types.get(*cursor) {
                    if !self.enabled.remove(t) {
                        self.enabled.insert(t.clone());
                    }
                    changed = true;
                }
            }
            KeyCode::Char('a') => {
                self.enabled = types.iter().cloned().collect();
                changed = true;
            }
            KeyCode::Char('n') => {
                self.enabled.clear();
                changed = true;
            }
            KeyCode::Esc | KeyCode::Char('r') | KeyCode::Char('q') => self.mode = Mode::Normal,
            _ => {}
        }
        if changed {
            self.relayout();
        }
    }

    fn key_search(&mut self, key: KeyEvent) {
        let Mode::Search { query } = &mut self.mode else {
            return;
        };
        match key.code {
            KeyCode::Char(c) => query.push(c),
            KeyCode::Backspace => {
                query.pop();
            }
            KeyCode::Enter => {
                let q = std::mem::take(query);
                self.mode = Mode::Normal;
                if !q.is_empty() {
                    self.last_search = Some(q.clone());
                    self.search(&q);
                }
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            _ => {}
        }
    }

    pub fn pan(&mut self, dx: i32, dy: i32) {
        self.camera.0 += dx;
        self.camera.1 += dy;
        self.clamp_camera();
    }

    /// `+`/`-`: choosing a zoom level by hand turns auto-fit off.
    pub fn manual_zoom(&mut self, z: Zoom) {
        if self.auto_fit {
            self.auto_fit = false;
            self.message = Some("auto-fit off (z turns it back on)".into());
        }
        self.set_zoom(z);
    }

    pub fn toggle_auto_fit(&mut self) {
        self.auto_fit = !self.auto_fit;
        self.fit_pending = self.auto_fit;
        self.message = Some(format!("auto-fit {}", if self.auto_fit { "on" } else { "off" }));
    }

    /// `x`: closes the selected box if it is open, otherwise the box that
    /// contains it (selecting that box), so repeated presses close outward.
    /// At the top of a drilled-in view it drills out instead.
    pub fn close_enclosing(&mut self) {
        let Some(s) = self.selected else { return };
        if self.expanded.contains(&s) {
            self.expanded.remove(&s);
            self.relayout();
            return;
        }
        match self.graph.parent(s) {
            Some(p) if self.layout.visible(p) => {
                self.expanded.remove(&p);
                self.selected = Some(p);
                self.relayout();
            }
            Some(p) if self.root() == Some(p) => self.drill_out(),
            _ => self.message = Some("nothing to close".into()),
        }
    }

    /// Mouse input: left click (see [`App::click`]) and wheel panning
    /// (Shift+wheel pans sideways). Ignored outside the canvas and while a
    /// popup is open.
    pub fn handle_mouse(&mut self, m: MouseEvent) {
        let (sx, sy) = (m.column as i32, m.row as i32);
        if self.mode != Mode::Normal || sx >= self.viewport.0 || sy >= self.viewport.1 {
            return;
        }
        let shift = m.modifiers.contains(KeyModifiers::SHIFT);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.message = None;
                self.click(sx, sy);
            }
            MouseEventKind::ScrollUp if shift => self.pan(-PAN_X, 0),
            MouseEventKind::ScrollDown if shift => self.pan(PAN_X, 0),
            MouseEventKind::ScrollUp => self.pan(0, -PAN_Y),
            MouseEventKind::ScrollDown => self.pan(0, PAN_Y),
            MouseEventKind::ScrollLeft => self.pan(-PAN_X, 0),
            MouseEventKind::ScrollRight => self.pan(PAN_X, 0),
            _ => {}
        }
    }

    /// Mouse click at canvas cell (`sx`, `sy`): selects the innermost box
    /// there. Clicking an open box's top border (its title or `[-]`) closes
    /// it; clicking a closed box's `[+N]` tag opens it. Returns false when
    /// nothing was hit.
    pub fn click(&mut self, sx: i32, sy: i32) -> bool {
        let (x, y) = (sx + self.camera.0, sy + self.camera.1);
        let Some(b) = self.layout.boxes.iter().rev().find(|b| b.bounds.contains_point(x, y)) else {
            return false;
        };
        let (n, r, expanded) = (b.node, b.bounds, b.expanded);
        self.selected = Some(n);
        let kids = self.graph.children(n).len();
        if expanded && y == r.y {
            self.expanded.remove(&n);
            self.relayout();
        } else if !expanded && kids > 0 && y == r.bottom() - 1 {
            let tag = if self.zoom == Zoom::Overview { 1 } else { format!("[+{kids}]").chars().count() as i32 };
            let x1 = r.right() - 1;
            if x >= x1 - tag && x < x1 {
                self.expanded.insert(n);
                self.relayout();
            }
        }
        true
    }

    pub fn set_zoom(&mut self, z: Zoom) {
        if z != self.zoom {
            self.zoom = z;
            self.relayout();
            self.center_pending = true;
        }
    }

    /// Moves the selection to the nearest visible box in direction `d`
    /// (ignoring the boxes that contain the current selection).
    pub fn move_selection(&mut self, d: Dir) {
        let Some(sel) = self.selected else {
            self.selected = self.layout.boxes.first().map(|b| b.node);
            return;
        };
        let Some(sb) = self.layout.box_of(sel) else {
            return;
        };
        // Right/Down on an open container steps into it (top-left child first).
        if sb.expanded && matches!(d, Dir::Right | Dir::Down) {
            let first = self
                .layout
                .boxes
                .iter()
                .filter(|b| self.graph.parent(b.node) == Some(sel))
                .min_by_key(|b| (b.bounds.x, b.bounds.y));
            if let Some(b) = first {
                self.selected = Some(b.node);
                self.follow_pending = true;
                return;
            }
        }
        let (sx, sy) = sb.bounds.center();
        let mut best: Option<(i32, NodeId)> = None;
        for b in &self.layout.boxes {
            if b.node == sel || self.graph.is_ancestor(b.node, sel) {
                continue;
            }
            let (bx, by) = b.bounds.center();
            // Cells are roughly twice as tall as wide.
            let (dx, dy) = (bx - sx, (by - sy) * 2);
            let (along, across) = match d {
                Dir::Right => (dx, dy.abs()),
                Dir::Left => (-dx, dy.abs()),
                Dir::Down => (dy, dx.abs()),
                Dir::Up => (-dy, dx.abs()),
            };
            if along <= 0 {
                continue;
            }
            let score = along + 2 * across;
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, b.node));
            }
        }
        if let Some((_, n)) = best {
            self.selected = Some(n);
            self.follow_pending = true;
        }
    }

    pub fn select_parent(&mut self) {
        let Some(p) = self.selected.and_then(|s| self.graph.parent(s)) else {
            return;
        };
        if self.layout.visible(p) {
            self.selected = Some(p);
            self.follow_pending = true;
        }
    }

    pub fn toggle_expand(&mut self) {
        let Some(s) = self.selected else { return };
        if self.graph.children(s).is_empty() {
            self.message = Some(format!("{} has no children", self.graph.item(s).id));
            return;
        }
        if !self.expanded.remove(&s) {
            self.expanded.insert(s);
        }
        self.relayout();
    }

    pub fn drill_in(&mut self) {
        let Some(s) = self.selected else { return };
        if self.graph.children(s).is_empty() {
            self.message = Some(format!("{} has no children", self.graph.item(s).id));
            return;
        }
        self.drill.push(s);
        self.selected = self.graph.children(s).first().copied();
        self.camera = (0, 0);
        self.relayout();
    }

    pub fn drill_out(&mut self) {
        if let Some(n) = self.drill.pop() {
            self.selected = Some(n);
            self.relayout();
            self.center_pending = true;
        }
    }

    fn scope(&self) -> Vec<NodeId> {
        match self.root() {
            Some(r) => self.graph.descendants(r),
            None => (0..self.graph.len()).collect(),
        }
    }

    pub fn expand_all(&mut self) {
        for n in self.scope() {
            if !self.graph.children(n).is_empty() {
                self.expanded.insert(n);
            }
        }
        self.relayout();
    }

    pub fn collapse_all(&mut self) {
        for n in self.scope() {
            self.expanded.remove(&n);
        }
        self.relayout();
    }

    /// Selects the next item (after the current selection) whose id or title
    /// contains `query`, expanding/drilling out as needed to make it visible.
    pub fn search(&mut self, query: &str) {
        let q = query.to_lowercase();
        let n = self.graph.len();
        let start = self.selected.map_or(0, |s| s + 1);
        let hit = (0..n).map(|k| (start + k) % n).find(|&i| {
            let it = self.graph.item(i);
            it.id.to_lowercase().contains(&q) || it.title.to_lowercase().contains(&q)
        });
        match hit {
            Some(i) => self.reveal(i),
            None => self.message = Some(format!("no match for {query:?}")),
        }
    }

    pub fn reveal(&mut self, n: NodeId) {
        while let Some(&r) = self.drill.last() {
            if self.graph.is_ancestor(r, n) {
                break;
            }
            self.drill.pop();
        }
        let root = self.root();
        for a in self.graph.ancestors(n) {
            if Some(a) == root {
                break;
            }
            self.expanded.insert(a);
        }
        self.selected = Some(n);
        self.relayout();
        self.center_pending = true;
    }

    /// The dependency cone of the selection, when tracing is on.
    pub fn trace(&self) -> Option<Trace> {
        if self.trace_mode == TraceMode::Off {
            return None;
        }
        let s = self.selected?;
        Some(Trace::compute(&self.graph, &self.layout, s, self.trace_mode, &self.enabled))
    }

    /// Expands (and drills out) just enough to give every traced item its own
    /// box.
    pub fn reveal_trace(&mut self) {
        if self.trace_mode == TraceMode::Off {
            self.trace_mode = TraceMode::Both;
        }
        let Some(t) = self.trace() else { return };
        let hidden = t.hidden(&self.graph, &self.layout);
        if hidden.is_empty() {
            self.message = Some("whole trace already visible".into());
            return;
        }
        let g = &self.graph;
        let mut members = Trace::outermost(g, &t.upstream);
        members.extend(Trace::outermost(g, &t.downstream));
        members.extend(self.selected);
        while let Some(r) = self.root() {
            if members.iter().all(|&n| g.is_ancestor(r, n)) {
                break;
            }
            self.drill.pop();
        }
        let root = self.root();
        for &n in &members {
            for a in g.ancestors(n) {
                if Some(a) == root {
                    break;
                }
                self.expanded.insert(a);
            }
        }
        self.relayout();
        self.message = Some(format!("revealed {} traced items", hidden.len()));
    }

    /// Re-reads the data source, keeping expansion, drill path, selection and
    /// relationship filters (matched by item id / link type).
    pub fn reload(&mut self) {
        let Some(src) = &self.source else {
            self.message = Some("no source to reload".into());
            return;
        };
        let graph = match src.load().and_then(Graph::from_document) {
            Ok(g) => g,
            Err(e) => {
                self.message = Some(format!("reload failed: {e:#}"));
                return;
            }
        };
        let id = |n: NodeId| self.graph.item(n).id.clone();
        let expanded: Vec<String> = self.expanded.iter().map(|&n| id(n)).collect();
        let drill: Vec<String> = self.drill.iter().map(|&n| id(n)).collect();
        let selected = self.selected.map(id);
        let old_types: HashSet<&String> = self.graph.link_types().iter().collect();
        let enabled: HashSet<String> = graph
            .link_types()
            .iter()
            .filter(|t| self.enabled.contains(*t) || !old_types.contains(t))
            .cloned()
            .collect();

        self.graph = graph;
        self.enabled = enabled;
        self.expanded = expanded.iter().filter_map(|i| self.graph.find(i)).collect();
        self.drill = drill.iter().map_while(|i| self.graph.find(i)).collect();
        self.selected = selected.and_then(|i| self.graph.find(&i));
        self.relayout();
        self.message = Some(format!("reloaded {} items", self.graph.len()));
    }
}
