//! Pure schematic layout: positions boxes and routes relationship edges in
//! world (character-cell) coordinates. No terminal code lives here.
//!
//! * Collapsed items are fixed-size boxes (size depends on [`Zoom`]).
//! * An expanded item is a container whose children are arranged in layered
//!   columns: longest-path layering over the enabled links projected onto the
//!   siblings (a link deep inside sibling A to something inside sibling B
//!   counts as A -> B), so dependencies flow left to right like a schematic.
//! * Edges are "lifted" to the nearest visible ancestor of each endpoint and
//!   merged when several links collapse onto the same pair of boxes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::model::{Graph, NodeId};

/// Discrete detail levels; terminals cannot scale glyphs, so "zoom" changes
/// how much each box shows and therefore how big it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zoom {
    #[default]
    Detail,
    Compact,
    Overview,
}

impl Zoom {
    pub fn zoom_in(self) -> Self {
        match self {
            Zoom::Overview => Zoom::Compact,
            _ => Zoom::Detail,
        }
    }

    pub fn zoom_out(self) -> Self {
        match self {
            Zoom::Detail => Zoom::Compact,
            _ => Zoom::Overview,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Zoom::Detail => "detail",
            Zoom::Compact => "compact",
            Zoom::Overview => "overview",
        }
    }

    /// (width, height) of a collapsed box, borders included.
    pub fn collapsed_size(self) -> (i32, i32) {
        match self {
            Zoom::Detail => (26, 5),
            Zoom::Compact => (20, 3),
            Zoom::Overview => (8, 3),
        }
    }

    fn col_gap(self) -> i32 {
        match self {
            Zoom::Detail => 8,
            Zoom::Compact => 6,
            Zoom::Overview => 4,
        }
    }

    /// Inner (horizontal, vertical) padding of an expanded container.
    fn pad(self) -> (i32, i32) {
        match self {
            Zoom::Detail => (2, 1),
            Zoom::Compact => (1, 1),
            Zoom::Overview => (1, 0),
        }
    }
}

const ROW_GAP: i32 = 1;
const MARGIN: i32 = 2;
const MAX_HEADER_CHARS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Bounds {
    /// First column to the right of the box.
    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    /// First row below the box.
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }

    pub fn intersects(&self, o: &Bounds) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    /// `o` lies inside this box's border (not touching it).
    pub fn strictly_contains(&self, o: &Bounds) -> bool {
        o.x > self.x && o.y > self.y && o.right() < self.right() && o.bottom() < self.bottom()
    }

    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedBox {
    pub node: NodeId,
    pub bounds: Bounds,
    /// Nesting depth relative to the current view root (top level = 0).
    pub depth: usize,
    pub expanded: bool,
    /// Index into [`Layout::boxes`] of the containing box.
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: String,
    /// Number of underlying links merged into this edge.
    pub count: usize,
    /// Orthogonal polyline; first point is just right of the source box, last
    /// point just left of the target box (where the arrowhead goes).
    pub points: Vec<(i32, i32)>,
    /// Border cell on the source / target box where the edge attaches.
    pub from_port: (i32, i32),
    pub to_port: (i32, i32),
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    /// Parents always precede their children.
    pub boxes: Vec<PlacedBox>,
    pub edges: Vec<Edge>,
    pub width: i32,
    pub height: i32,
    /// Enabled links with an endpoint outside the current (drilled-in) view.
    pub hidden_links: usize,
    by_node: HashMap<NodeId, usize>,
}

impl Layout {
    pub fn box_of(&self, n: NodeId) -> Option<&PlacedBox> {
        self.by_node.get(&n).map(|&i| &self.boxes[i])
    }

    pub fn visible(&self, n: NodeId) -> bool {
        self.by_node.contains_key(&n)
    }
}

pub struct LayoutParams<'a> {
    /// Drill-in root; `None` shows the whole forest.
    pub root: Option<NodeId>,
    pub expanded: &'a HashSet<NodeId>,
    pub zoom: Zoom,
    pub enabled: &'a HashSet<String>,
}

/// Text shown in an expanded container's top border.
pub fn header_text(graph: &Graph, n: NodeId, zoom: Zoom) -> String {
    let item = graph.item(n);
    let s = match zoom {
        Zoom::Overview => item.id.clone(),
        _ => format!("{} {}", item.id, item.title),
    };
    s.chars().take(MAX_HEADER_CHARS).collect()
}

pub fn compute(graph: &Graph, params: &LayoutParams) -> Layout {
    let ctx = Ctx {
        graph,
        expanded: params.expanded,
        zoom: params.zoom,
        sib: sibling_edges(graph, params.enabled),
    };
    let (_, _, placed) = ctx.arrange(graph.top_children(params.root), params.root);

    let mut boxes = Vec::new();
    for (sub, x, y) in &placed {
        flatten(sub, x + MARGIN, y + MARGIN, 0, None, &mut boxes);
    }
    let by_node: HashMap<NodeId, usize> =
        boxes.iter().enumerate().map(|(i, b)| (b.node, i)).collect();
    let (edges, hidden_links) = route_edges(graph, params.enabled, &boxes, &by_node);

    let mut width = 0;
    let mut height = 0;
    for b in &boxes {
        width = width.max(b.bounds.right());
        height = height.max(b.bounds.bottom());
    }
    for e in &edges {
        for &(x, y) in &e.points {
            width = width.max(x + 1);
            height = height.max(y + 1);
        }
    }
    Layout {
        boxes,
        edges,
        width: width + MARGIN,
        height: height + MARGIN,
        hidden_links,
        by_node,
    }
}

/// Measured subtree: a box plus its children at offsets relative to it.
struct Sub {
    node: NodeId,
    w: i32,
    h: i32,
    expanded: bool,
    children: Vec<(Sub, i32, i32)>,
}

struct Ctx<'a> {
    graph: &'a Graph,
    expanded: &'a HashSet<NodeId>,
    zoom: Zoom,
    /// Per container (None = forest top level): sibling-level edges.
    sib: HashMap<Option<NodeId>, BTreeSet<(NodeId, NodeId)>>,
}

impl Ctx<'_> {
    fn sub(&self, n: NodeId) -> Sub {
        let kids = self.graph.children(n);
        let (min_w, min_h) = self.zoom.collapsed_size();
        if !self.expanded.contains(&n) || kids.is_empty() {
            return Sub {
                node: n,
                w: min_w,
                h: min_h,
                expanded: false,
                children: Vec::new(),
            };
        }
        let (cw, ch, placed) = self.arrange(kids, Some(n));
        let (px, py) = self.zoom.pad();
        // border + "─ " + header + " " + room for the "[-]" marker
        let header_w = header_text(self.graph, n, self.zoom).chars().count() as i32 + 9;
        Sub {
            node: n,
            w: (cw + 2 * px + 2).max(header_w).max(min_w),
            h: ch + 2 * py + 2,
            expanded: true,
            children: placed
                .into_iter()
                .map(|(s, x, y)| (s, x + 1 + px, y + 1 + py))
                .collect(),
        }
    }

    /// Lays out `nodes` (siblings) in layered columns. Returns the content
    /// size and each subtree's offset.
    fn arrange(&self, nodes: &[NodeId], container: Option<NodeId>) -> (i32, i32, Vec<(Sub, i32, i32)>) {
        if nodes.is_empty() {
            return (0, 0, Vec::new());
        }
        let subs: Vec<Sub> = nodes.iter().map(|&c| self.sub(c)).collect();
        let pos: HashMap<NodeId, usize> = nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let edges: Vec<(usize, usize)> = self
            .sib
            .get(&container)
            .into_iter()
            .flatten()
            .filter_map(|(a, b)| Some((*pos.get(a)?, *pos.get(b)?)))
            .collect();
        let layers = layer_assignment(nodes.len(), &edges);
        let ncols = layers.iter().max().map_or(0, |m| m + 1);
        let mut cols: Vec<Vec<usize>> = vec![Vec::new(); ncols];
        for (i, &l) in layers.iter().enumerate() {
            cols[l].push(i);
        }

        let col_h: Vec<i32> = cols
            .iter()
            .map(|c| c.iter().map(|&i| subs[i].h).sum::<i32>() + ROW_GAP * (c.len() as i32 - 1))
            .collect();
        let total_h = col_h.iter().copied().max().unwrap_or(0);
        let gap = self.zoom.col_gap();
        let mut offsets = vec![(0, 0); nodes.len()];
        let mut x = 0;
        for (ci, col) in cols.iter().enumerate() {
            let cw = col.iter().map(|&i| subs[i].w).max().unwrap_or(0);
            let mut y = (total_h - col_h[ci]) / 2;
            for &i in col {
                offsets[i] = (x, y);
                y += subs[i].h + ROW_GAP;
            }
            x += cw + gap;
        }
        let total_w = x - gap;
        let placed = subs
            .into_iter()
            .zip(offsets)
            .map(|(s, (x, y))| (s, x, y))
            .collect();
        (total_w, total_h, placed)
    }
}

/// Longest-path layering. Cycles are broken by ignoring DFS back edges.
fn layer_assignment(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    fn dfs(u: usize, adj: &[Vec<usize>], state: &mut [u8], dag: &mut [Vec<usize>], order: &mut Vec<usize>) {
        state[u] = 1;
        for &v in &adj[u] {
            match state[v] {
                0 => {
                    dag[u].push(v);
                    dfs(v, adj, state, dag, order);
                }
                2 => dag[u].push(v),
                _ => {} // back edge: part of a cycle, ignored for layering
            }
        }
        state[u] = 2;
        order.push(u);
    }

    let mut adj = vec![Vec::new(); n];
    for &(a, b) in edges {
        if a != b {
            adj[a].push(b);
        }
    }
    let mut state = vec![0u8; n];
    let mut dag = vec![Vec::new(); n];
    let mut order = Vec::with_capacity(n);
    for s in 0..n {
        if state[s] == 0 {
            dfs(s, &adj, &mut state, &mut dag, &mut order);
        }
    }
    order.reverse();
    let mut layer = vec![0; n];
    for &u in &order {
        for &v in &dag[u] {
            layer[v] = layer[v].max(layer[u] + 1);
        }
    }
    layer
}

fn flatten(sub: &Sub, x: i32, y: i32, depth: usize, parent: Option<usize>, out: &mut Vec<PlacedBox>) {
    let idx = out.len();
    out.push(PlacedBox {
        node: sub.node,
        bounds: Bounds { x, y, w: sub.w, h: sub.h },
        depth,
        expanded: sub.expanded,
        parent,
    });
    for (c, cx, cy) in &sub.children {
        flatten(c, x + cx, y + cy, depth + 1, Some(idx), out);
    }
}

/// Projects every enabled link onto the pair of siblings directly below the
/// endpoints' lowest common ancestor.
fn sibling_edges(g: &Graph, enabled: &HashSet<String>) -> HashMap<Option<NodeId>, BTreeSet<(NodeId, NodeId)>> {
    let mut map: HashMap<Option<NodeId>, BTreeSet<(NodeId, NodeId)>> = HashMap::new();
    for l in g.links() {
        if !enabled.contains(&l.kind) {
            continue;
        }
        let pa = g.path_from_root(l.from);
        let pb = g.path_from_root(l.to);
        let common = pa.iter().zip(&pb).take_while(|(a, b)| a == b).count();
        if common == pa.len() || common == pb.len() {
            continue; // same item, or one contains the other
        }
        let container = common.checked_sub(1).map(|i| pa[i]);
        map.entry(container).or_default().insert((pa[common], pb[common]));
    }
    map
}

fn route_edges(
    g: &Graph,
    enabled: &HashSet<String>,
    boxes: &[PlacedBox],
    by_node: &HashMap<NodeId, usize>,
) -> (Vec<Edge>, usize) {
    let rep = |n: NodeId| -> Option<NodeId> {
        let mut cur = Some(n);
        while let Some(c) = cur {
            if by_node.contains_key(&c) {
                return Some(c);
            }
            cur = g.parent(c);
        }
        None
    };

    let mut agg: BTreeMap<(NodeId, NodeId, String), usize> = BTreeMap::new();
    let mut hidden = 0;
    for l in g.links() {
        if !enabled.contains(&l.kind) {
            continue;
        }
        match (rep(l.from), rep(l.to)) {
            (Some(a), Some(b)) => {
                if a != b && !g.is_ancestor(a, b) && !g.is_ancestor(b, a) {
                    *agg.entry((a, b, l.kind.clone())).or_insert(0) += 1;
                }
            }
            (Some(_), None) | (None, Some(_)) => hidden += 1,
            (None, None) => {}
        }
    }
    let keys: Vec<((NodeId, NodeId, String), usize)> = agg.into_iter().collect();
    let bx = |n: NodeId| &boxes[by_node[&n]].bounds;

    // Spread ports along the box sides so parallel edges stay distinguishable.
    let mut outgoing: BTreeMap<NodeId, Vec<usize>> = BTreeMap::new();
    let mut incoming: BTreeMap<NodeId, Vec<usize>> = BTreeMap::new();
    for (i, ((a, b, _), _)) in keys.iter().enumerate() {
        outgoing.entry(*a).or_default().push(i);
        incoming.entry(*b).or_default().push(i);
    }
    let mut from_y = vec![0; keys.len()];
    let mut to_y = vec![0; keys.len()];
    for (n, list) in &mut outgoing {
        list.sort_by_key(|&i| (bx(keys[i].0.1).center().1, i));
        assign_ports(bx(*n), list, &mut from_y);
    }
    for (n, list) in &mut incoming {
        list.sort_by_key(|&i| (bx(keys[i].0.0).center().1, i));
        assign_ports(bx(*n), list, &mut to_y);
    }

    // Vertical "lanes": sources sharing a right edge get distinct channels.
    let mut lanes: BTreeMap<i32, Vec<NodeId>> = BTreeMap::new();
    for ((a, _, _), _) in &keys {
        let list = lanes.entry(bx(*a).right()).or_default();
        if !list.contains(a) {
            list.push(*a);
        }
    }

    let mut router = Router::new(boxes);
    let edges = keys
        .into_iter()
        .enumerate()
        .map(|(i, ((a, b, kind), count))| {
            let (ab, bb) = (bx(a), bx(b));
            let s = (ab.right(), from_y[i]);
            let e = (bb.x - 1, to_y[i]);
            let lane = &lanes[&s.0];
            let li = lane.iter().position(|&n| n == a).unwrap_or(0) as i32;
            let preferred_mx = s.0 + (e.0 - s.0) * (li + 1) / (lane.len() as i32 + 1);
            let obstacles: Vec<(Bounds, i64)> = boxes
                .iter()
                .filter(|o| !g.is_ancestor(o.node, a) && !g.is_ancestor(o.node, b))
                .map(|o| (o.bounds, if o.expanded { CROSS_CONTAINER } else { CROSS_BOX }))
                .collect();
            let points = router.route(s, e, preferred_mx, &obstacles);
            Edge {
                from: a,
                to: b,
                kind,
                count,
                points: simplify(points),
                from_port: (ab.right() - 1, s.1),
                to_port: (bb.x, e.1),
            }
        })
        .collect();
    (edges, hidden)
}

/// Cost per cell of running a wire through an unrelated leaf box / container.
const CROSS_BOX: i64 = 60;
const CROSS_CONTAINER: i64 = 6;
/// Cost per cell of overlapping a previously routed wire.
const OVERLAP: i64 = 4;
const BEND: i64 = 2;
/// Extra cost of a 5-segment detour over a 3-segment route.
const DETOUR: i64 = 6;

/// Greedy orthogonal router: for each edge, evaluates a family of 3-segment
/// (one vertical channel) and 5-segment (detour along a free row) routes and
/// keeps the cheapest, accounting for boxes crossed and wires already placed.
struct Router {
    used: HashSet<(i32, i32)>,
    min_y: i32,
    max_y: i32,
}

impl Router {
    fn new(boxes: &[PlacedBox]) -> Self {
        let min_y = boxes.iter().map(|b| b.bounds.y).min().unwrap_or(0);
        let max_y = boxes.iter().map(|b| b.bounds.bottom()).max().unwrap_or(0);
        Self {
            used: HashSet::new(),
            min_y: min_y - 1,
            max_y,
        }
    }

    fn seg_cost(&self, p: (i32, i32), q: (i32, i32), obstacles: &[(Bounds, i64)]) -> i64 {
        let (x0, x1) = (p.0.min(q.0), p.0.max(q.0));
        let (y0, y1) = (p.1.min(q.1), p.1.max(q.1));
        let mut cost = ((x1 - x0) + (y1 - y0)) as i64;
        for (b, weight) in obstacles {
            let w = (x1 + 1).min(b.right()) - x0.max(b.x);
            let h = (y1 + 1).min(b.bottom()) - y0.max(b.y);
            if w > 0 && h > 0 {
                cost += (w * h) as i64 * weight;
            }
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                if self.used.contains(&(x, y)) {
                    cost += OVERLAP;
                }
            }
        }
        cost
    }

    fn path_cost(&self, pts: &[(i32, i32)], obstacles: &[(Bounds, i64)]) -> i64 {
        let segs: i64 = pts.windows(2).map(|w| self.seg_cost(w[0], w[1], obstacles)).sum();
        segs + BEND * (pts.len() as i64 - 2)
    }

    fn route(
        &mut self,
        s: (i32, i32),
        e: (i32, i32),
        preferred_mx: i32,
        obstacles: &[(Bounds, i64)],
    ) -> Vec<(i32, i32)> {
        let mut best: Option<(i64, Vec<(i32, i32)>)> = None;
        let mut consider = |cost: i64, pts: Vec<(i32, i32)>| {
            if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                best = Some((cost, pts));
            }
        };
        if e.0 >= s.0 {
            for mx in s.0..=e.0 {
                let pts = vec![s, (mx, s.1), (mx, e.1), e];
                // ×8 so the preferred-lane tie-breaker never outweighs a real cost
                let cost = self.path_cost(&pts, obstacles) * 8 + (mx - preferred_mx).abs() as i64;
                consider(cost, pts);
            }
        }
        for dx1 in 1..=3 {
            for dx2 in 1..=3 {
                let (x1, x2) = (s.0 + dx1, e.0 - dx2);
                for yf in self.min_y..=self.max_y {
                    let pts = vec![s, (x1, s.1), (x1, yf), (x2, yf), (x2, e.1), e];
                    let cost = (self.path_cost(&pts, obstacles) + DETOUR) * 8;
                    consider(cost, pts);
                }
            }
        }
        let pts = simplify(best.map(|(_, p)| p).unwrap_or_else(|| vec![s, e]));
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            for x in p.0.min(q.0)..=p.0.max(q.0) {
                for y in p.1.min(q.1)..=p.1.max(q.1) {
                    self.used.insert((x, y));
                }
            }
        }
        pts
    }
}

fn assign_ports(b: &Bounds, list: &[usize], ys: &mut [i32]) {
    let rows = (b.h - 2).max(1);
    let k = list.len() as i32;
    for (j, &i) in list.iter().enumerate() {
        let j = j as i32;
        ys[i] = if k <= rows {
            b.y + 1 + (rows - k) / 2 + j
        } else {
            b.y + 1 + j * rows / k
        };
    }
}

/// Drops repeated and collinear points.
fn simplify(points: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    let mut out: Vec<(i32, i32)> = Vec::with_capacity(points.len());
    for p in points {
        if out.last() == Some(&p) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            if (a.0 == b.0 && b.0 == p.0) || (a.1 == b.1 && b.1 == p.1) {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layering_orders_chain_and_breaks_cycles() {
        assert_eq!(layer_assignment(3, &[(0, 1), (1, 2)]), vec![0, 1, 2]);
        assert_eq!(layer_assignment(3, &[(2, 1), (1, 0)]), vec![2, 1, 0]);
        let l = layer_assignment(2, &[(0, 1), (1, 0)]);
        assert_eq!(l, vec![0, 1]);
    }

    #[test]
    fn simplify_removes_degenerate_points() {
        let p = simplify(vec![(0, 0), (3, 0), (3, 0), (6, 0)]);
        assert_eq!(p, vec![(0, 0), (6, 0)]);
        let p = simplify(vec![(0, 0), (2, 0), (2, 4), (5, 4)]);
        assert_eq!(p, vec![(0, 0), (2, 0), (2, 4), (5, 4)]);
    }
}
