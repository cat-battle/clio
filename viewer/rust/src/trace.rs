//! Dependency-cone tracing (RTL "fan-in / fan-out" for tasks).
//!
//! The *focus* is the selected item plus its descendants. Following the enabled
//! link types forwards gives everything **downstream** of it (what depends on
//! it); following them backwards gives everything **upstream** (what it
//! depends on). Reaching an item pulls in its descendants too, because a link
//! to an epic applies to the work inside it, and their links are followed in
//! turn.
//!
//! The result is mapped onto the current layout the same way edges are lifted:
//! a member hidden inside a collapsed box marks that box.

use std::collections::{HashMap, HashSet};

use crate::layout::Layout;
use crate::model::{Graph, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceMode {
    #[default]
    Off,
    Both,
    Upstream,
    Downstream,
}

impl TraceMode {
    pub fn next(self) -> Self {
        match self {
            TraceMode::Off => TraceMode::Both,
            TraceMode::Both => TraceMode::Upstream,
            TraceMode::Upstream => TraceMode::Downstream,
            TraceMode::Downstream => TraceMode::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TraceMode::Off => "off",
            TraceMode::Both => "both",
            TraceMode::Upstream => "upstream",
            TraceMode::Downstream => "downstream",
        }
    }

    fn up(self) -> bool {
        matches!(self, TraceMode::Both | TraceMode::Upstream)
    }

    fn down(self) -> bool {
        matches!(self, TraceMode::Both | TraceMode::Downstream)
    }
}

/// How a visible box relates to the traced focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Focus,
    Upstream,
    Downstream,
    /// Both upstream and downstream (the focus sits on a cycle with it).
    Both,
}

#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub focus: HashSet<NodeId>,
    pub upstream: HashSet<NodeId>,
    pub downstream: HashSet<NodeId>,
    /// Role of each visible box that holds (or hides) a traced item.
    pub roles: HashMap<NodeId, Role>,
    /// Drawn edges `(from box, to box, link type)` that carry a traced link.
    pub edges: HashSet<(NodeId, NodeId, String)>,
}

impl Trace {
    /// Traces `mode` from `selected` over the `enabled` link types and maps the
    /// result onto `layout`.
    pub fn compute(
        graph: &Graph,
        layout: &Layout,
        selected: NodeId,
        mode: TraceMode,
        enabled: &HashSet<String>,
    ) -> Self {
        let mut focus: HashSet<NodeId> = graph.descendants(selected).into_iter().collect();
        focus.insert(selected);
        let links: Vec<(NodeId, NodeId, &str)> = graph
            .links()
            .iter()
            .filter(|l| enabled.contains(&l.kind))
            .map(|l| (l.from, l.to, l.kind.as_str()))
            .collect();

        let mut t = Trace {
            focus,
            ..Default::default()
        };
        let mut traced: Vec<(NodeId, NodeId, &str)> = Vec::new();
        if mode.down() {
            t.downstream = closure(graph, &t.focus, &links, false);
            let reach = |n: &NodeId| t.focus.contains(n) || t.downstream.contains(n);
            traced.extend(links.iter().filter(|l| reach(&l.0) && t.downstream.contains(&l.1)));
        }
        if mode.up() {
            t.upstream = closure(graph, &t.focus, &links, true);
            let reach = |n: &NodeId| t.focus.contains(n) || t.upstream.contains(n);
            traced.extend(links.iter().filter(|l| reach(&l.1) && t.upstream.contains(&l.0)));
        }

        let rep = |n: NodeId| visible_rep(graph, layout, n);
        for (set, role) in [(&t.upstream, Role::Upstream), (&t.downstream, Role::Downstream)] {
            for &n in set {
                if let Some(b) = rep(n) {
                    let r = t.roles.entry(b).or_insert(role);
                    if *r != role && *r != Role::Focus {
                        *r = Role::Both;
                    }
                }
            }
        }
        for &n in &t.focus {
            if let Some(b) = rep(n) {
                t.roles.insert(b, Role::Focus);
            }
        }
        for (a, b, kind) in traced {
            if let (Some(ra), Some(rb)) = (rep(a), rep(b))
                && ra != rb
            {
                t.edges.insert((ra, rb, kind.to_string()));
            }
        }
        t
    }

    pub fn role(&self, n: NodeId) -> Option<Role> {
        self.roles.get(&n).copied()
    }

    pub fn edge_traced(&self, from: NodeId, to: NodeId, kind: &str) -> bool {
        self.edges.contains(&(from, to, kind.to_string()))
    }

    /// Outermost members of `set`: items whose parent is not also a member.
    /// These are what the counts and "reveal" refer to (an epic pulled in with
    /// its stories counts once).
    pub fn outermost(graph: &Graph, set: &HashSet<NodeId>) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = set
            .iter()
            .copied()
            .filter(|&n| graph.parent(n).is_none_or(|p| !set.contains(&p)))
            .collect();
        v.sort_unstable();
        v
    }

    /// Outermost upstream and downstream members that have no box of their
    /// own in `layout` (inside a collapsed box or outside the drilled-in view).
    pub fn hidden(&self, graph: &Graph, layout: &Layout) -> Vec<NodeId> {
        let mut v: Vec<NodeId> = Self::outermost(graph, &self.upstream)
            .into_iter()
            .chain(Self::outermost(graph, &self.downstream))
            .filter(|&n| !layout.visible(n))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

/// Items reachable from `start` along `links` (backwards when `reverse`),
/// each reached item contributing its descendants. `start` itself is excluded.
fn closure(
    graph: &Graph,
    start: &HashSet<NodeId>,
    links: &[(NodeId, NodeId, &str)],
    reverse: bool,
) -> HashSet<NodeId> {
    let mut adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for &(a, b, _) in links {
        let (s, t) = if reverse { (b, a) } else { (a, b) };
        adj.entry(s).or_default().push(t);
    }
    let mut out = HashSet::new();
    let mut stack: Vec<NodeId> = start.iter().copied().collect();
    while let Some(u) = stack.pop() {
        for &v in adj.get(&u).map_or(&[][..], |v| v.as_slice()) {
            if start.contains(&v) || out.contains(&v) {
                continue;
            }
            for w in std::iter::once(v).chain(graph.descendants(v)) {
                if !start.contains(&w) && out.insert(w) {
                    stack.push(w);
                }
            }
        }
    }
    out
}

/// The box an item is drawn as: itself if visible, else its nearest visible
/// ancestor (None when it lies outside the current view).
pub fn visible_rep(graph: &Graph, layout: &Layout, n: NodeId) -> Option<NodeId> {
    if layout.visible(n) {
        return Some(n);
    }
    graph.ancestors(n).into_iter().find(|&a| layout.visible(a))
}
