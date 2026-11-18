//! Shared data model: the on-disk [`Document`] format and the indexed [`Graph`]
//! the viewer works with. See `viewer/schema/clio-tasks.schema.json`.

use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Milestone,
    Epic,
    Story,
    Subtask,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Milestone => "milestone",
            Kind::Epic => "epic",
            Kind::Story => "story",
            Kind::Subtask => "subtask",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Todo,
    InProgress,
    Done,
    Blocked,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Todo => "todo",
            Status::InProgress => "in progress",
            Status::Done => "done",
            Status::Blocked => "blocked",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Status::Todo => "○",
            Status::InProgress => "◐",
            Status::Done => "●",
            Status::Blocked => "✖",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u32,
    pub items: Vec<Item>,
    #[serde(default)]
    pub links: Vec<Link>,
}

/// Index of an item inside a [`Graph`].
pub type NodeId = usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLink {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: String,
}

/// Validated, indexed task forest plus typed links.
#[derive(Debug, Clone)]
pub struct Graph {
    items: Vec<Item>,
    index: HashMap<String, NodeId>,
    parent: Vec<Option<NodeId>>,
    children: Vec<Vec<NodeId>>,
    roots: Vec<NodeId>,
    links: Vec<ResolvedLink>,
    link_types: Vec<String>,
}

/// Replaces every control character (Unicode Cc: C0, DEL, C1) with a space,
/// so data from a file cannot send escape sequences (OSC 52 clipboard
/// writes, hyperlinks, ...) to the terminal. Descriptions keep `\n`.
/// Mirrors `sanitize` in `viewer/python/clio_viewer/model.py`.
pub fn sanitize(doc: &mut Document) {
    fn clean(s: &mut String, keep_newlines: bool) {
        if s.chars().any(|c| c.is_control()) {
            *s = s
                .chars()
                .map(|c| {
                    if c.is_control() && !(keep_newlines && c == '\n') {
                        ' '
                    } else {
                        c
                    }
                })
                .collect();
        }
    }
    for it in &mut doc.items {
        clean(&mut it.id, false);
        clean(&mut it.title, false);
        for s in [&mut it.parent, &mut it.assignee].into_iter().flatten() {
            clean(s, false);
        }
        if let Some(d) = &mut it.description {
            clean(d, true);
        }
    }
    for l in &mut doc.links {
        clean(&mut l.from, false);
        clean(&mut l.to, false);
        clean(&mut l.kind, false);
    }
}

impl Graph {
    /// Builds a graph, checking that ids are unique, every parent and link endpoint
    /// exists, and the parent relation has no cycles. Text is [`sanitize`]d first.
    pub fn from_document(mut doc: Document) -> Result<Self> {
        sanitize(&mut doc);
        if doc.version != FORMAT_VERSION {
            bail!(
                "unsupported format version {} (expected {FORMAT_VERSION})",
                doc.version
            );
        }
        let n = doc.items.len();
        let mut index = HashMap::with_capacity(n);
        for (i, item) in doc.items.iter().enumerate() {
            if item.id.is_empty() {
                bail!("item #{i} has an empty id");
            }
            if index.insert(item.id.clone(), i).is_some() {
                bail!("duplicate item id {:?}", item.id);
            }
        }

        let mut parent = vec![None; n];
        let mut children = vec![Vec::new(); n];
        let mut roots = Vec::new();
        for (i, item) in doc.items.iter().enumerate() {
            match &item.parent {
                None => roots.push(i),
                Some(p) => {
                    let Some(&pi) = index.get(p) else {
                        bail!("item {:?} has unknown parent {:?}", item.id, p);
                    };
                    parent[i] = Some(pi);
                    children[pi].push(i);
                }
            }
        }
        for (i, item) in doc.items.iter().enumerate() {
            let mut cur = i;
            let mut steps = 0;
            while let Some(p) = parent[cur] {
                cur = p;
                steps += 1;
                if steps > n {
                    bail!("parent cycle involving item {:?}", item.id);
                }
            }
        }

        let mut links = Vec::with_capacity(doc.links.len());
        let mut types = BTreeSet::new();
        for l in &doc.links {
            let Some(&from) = index.get(&l.from) else {
                bail!("{} link {:?} -> {:?}: unknown source", l.kind, l.from, l.to);
            };
            let Some(&to) = index.get(&l.to) else {
                bail!("{} link {:?} -> {:?}: unknown target", l.kind, l.from, l.to);
            };
            if l.kind.is_empty() {
                bail!("link {:?} -> {:?} has an empty type", l.from, l.to);
            }
            types.insert(l.kind.clone());
            links.push(ResolvedLink {
                from,
                to,
                kind: l.kind.clone(),
            });
        }

        Ok(Self {
            items: doc.items,
            index,
            parent,
            children,
            roots,
            links,
            link_types: types.into_iter().collect(),
        })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn item(&self, n: NodeId) -> &Item {
        &self.items[n]
    }

    pub fn find(&self, id: &str) -> Option<NodeId> {
        self.index.get(id).copied()
    }

    pub fn parent(&self, n: NodeId) -> Option<NodeId> {
        self.parent[n]
    }

    pub fn children(&self, n: NodeId) -> &[NodeId] {
        &self.children[n]
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Children of `root`, or the top-level items when `root` is `None`.
    pub fn top_children(&self, root: Option<NodeId>) -> &[NodeId] {
        match root {
            Some(r) => &self.children[r],
            None => &self.roots,
        }
    }

    pub fn links(&self) -> &[ResolvedLink] {
        &self.links
    }

    /// Distinct link types present in the data, sorted.
    pub fn link_types(&self) -> &[String] {
        &self.link_types
    }

    /// Ancestors of `n`, nearest first.
    pub fn ancestors(&self, n: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.parent[n];
        while let Some(p) = cur {
            out.push(p);
            cur = self.parent[p];
        }
        out
    }

    /// Path from the top-level ancestor down to and including `n`.
    pub fn path_from_root(&self, n: NodeId) -> Vec<NodeId> {
        let mut path = self.ancestors(n);
        path.reverse();
        path.push(n);
        path
    }

    /// True when `a` is a strict ancestor of `n`.
    pub fn is_ancestor(&self, a: NodeId, n: NodeId) -> bool {
        let mut cur = self.parent[n];
        while let Some(p) = cur {
            if p == a {
                return true;
            }
            cur = self.parent[p];
        }
        false
    }

    /// All strict descendants of `n`, depth first.
    pub fn descendants(&self, n: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children[n].iter().rev().copied().collect();
        while let Some(c) = stack.pop() {
            out.push(c);
            stack.extend(self.children[c].iter().rev().copied());
        }
        out
    }
}
