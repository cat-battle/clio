"""Shared data model: the on-disk :class:`Document` format and the indexed
:class:`Graph` the viewer works with. See ``viewer/schema/clio-tasks.schema.json``.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field, replace
from enum import Enum
from typing import Any

FORMAT_VERSION = 1

NodeId = int


class ValidationError(ValueError):
    """The document is malformed or internally inconsistent."""


class Kind(str, Enum):
    MILESTONE = "milestone"
    EPIC = "epic"
    STORY = "story"
    SUBTASK = "subtask"

    @property
    def label(self) -> str:
        return self.value


class Status(str, Enum):
    TODO = "todo"
    IN_PROGRESS = "in_progress"
    DONE = "done"
    BLOCKED = "blocked"

    @property
    def label(self) -> str:
        return self.value.replace("_", " ")

    @property
    def glyph(self) -> str:
        return {"todo": "○", "in_progress": "◐", "done": "●", "blocked": "✖"}[self.value]


@dataclass
class Item:
    id: str
    kind: Kind
    title: str
    status: Status = Status.TODO
    parent: str | None = None
    assignee: str | None = None
    description: str | None = None


@dataclass(frozen=True)
class Link:
    source: str
    target: str
    type: str


@dataclass
class Document:
    version: int
    items: list[Item]
    links: list[Link] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: Any) -> Document:
        """Strictly parses the JSON form (unknown keys are rejected)."""
        _expect_obj(data, "document", required={"version", "items"}, optional={"links"})
        items = [_parse_item(i, raw) for i, raw in enumerate(_expect_list(data["items"], "items"))]
        links = [_parse_link(i, raw) for i, raw in enumerate(_expect_list(data.get("links", []), "links"))]
        version = data["version"]
        if not isinstance(version, int) or isinstance(version, bool):
            raise ValidationError("version must be an integer")
        return cls(version=version, items=items, links=links)

    def to_dict(self) -> dict[str, Any]:
        items = []
        for it in self.items:
            d: dict[str, Any] = {"id": it.id, "kind": it.kind.value, "title": it.title, "status": it.status.value}
            for k in ("parent", "assignee", "description"):
                if getattr(it, k) is not None:
                    d[k] = getattr(it, k)
            items.append(d)
        links = [{"from": l.source, "to": l.target, "type": l.type} for l in self.links]
        return {"version": self.version, "items": items, "links": links}


def _expect_obj(v: Any, what: str, required: set[str], optional: set[str]) -> None:
    if not isinstance(v, dict):
        raise ValidationError(f"{what} must be an object")
    missing = required - v.keys()
    if missing:
        raise ValidationError(f"{what} is missing {sorted(missing)}")
    unknown = v.keys() - required - optional
    if unknown:
        raise ValidationError(f"{what} has unknown field(s) {sorted(unknown)}")


def _expect_list(v: Any, what: str) -> list[Any]:
    if not isinstance(v, list):
        raise ValidationError(f"{what} must be an array")
    return v


def _opt_str(d: dict[str, Any], key: str, what: str) -> str | None:
    v = d.get(key)
    if v is not None and not isinstance(v, str):
        raise ValidationError(f"{what}.{key} must be a string")
    return v


def _parse_item(i: int, raw: Any) -> Item:
    what = f"items[{i}]"
    _expect_obj(raw, what, {"id", "kind", "title"}, {"status", "parent", "assignee", "description"})
    for key in ("id", "title"):
        if not isinstance(raw[key], str):
            raise ValidationError(f"{what}.{key} must be a string")
    try:
        kind = Kind(raw["kind"])
        status = Status(raw.get("status", "todo"))
    except ValueError as e:
        raise ValidationError(f"{what}: {e}") from None
    return Item(
        id=raw["id"],
        kind=kind,
        title=raw["title"],
        status=status,
        parent=_opt_str(raw, "parent", what),
        assignee=_opt_str(raw, "assignee", what),
        description=_opt_str(raw, "description", what),
    )


def _parse_link(i: int, raw: Any) -> Link:
    what = f"links[{i}]"
    _expect_obj(raw, what, {"from", "to", "type"}, set())
    for key in ("from", "to", "type"):
        if not isinstance(raw[key], str):
            raise ValidationError(f"{what}.{key} must be a string")
    return Link(source=raw["from"], target=raw["to"], type=raw["type"])


@dataclass(frozen=True)
class ResolvedLink:
    source: NodeId
    target: NodeId
    type: str


# Unicode category Cc: C0 controls, DEL and C1 controls (the same set as
# Rust's ``char::is_control``).
_CONTROL = re.compile("[\x00-\x1f\x7f-\x9f]")
_CONTROL_BUT_NEWLINE = re.compile("[\x00-\x09\x0b-\x1f\x7f-\x9f]")


def _clean(s: str | None, keep_newlines: bool = False) -> str | None:
    if s is None:
        return None
    return (_CONTROL_BUT_NEWLINE if keep_newlines else _CONTROL).sub(" ", s)


def sanitize(doc: Document) -> Document:
    """Returns a copy of ``doc`` with every control character replaced by a
    space, so data from a file cannot send escape sequences (OSC 52
    clipboard writes, hyperlinks, ...) to the terminal. Descriptions keep
    ``\\n``. Mirrors ``sanitize`` in ``viewer/rust/src/model.rs``."""
    items = [
        replace(
            it,
            id=_clean(it.id),
            title=_clean(it.title),
            parent=_clean(it.parent),
            assignee=_clean(it.assignee),
            description=_clean(it.description, keep_newlines=True),
        )
        for it in doc.items
    ]
    links = [Link(_clean(l.source), _clean(l.target), _clean(l.type)) for l in doc.links]  # type: ignore[arg-type]
    return Document(doc.version, items, links)


class Graph:
    """Validated, indexed task forest plus typed links."""

    def __init__(self, doc: Document) -> None:
        doc = sanitize(doc)
        if doc.version != FORMAT_VERSION:
            raise ValidationError(f"unsupported format version {doc.version} (expected {FORMAT_VERSION})")
        n = len(doc.items)
        index: dict[str, NodeId] = {}
        for i, item in enumerate(doc.items):
            if not item.id:
                raise ValidationError(f"item #{i} has an empty id")
            if item.id in index:
                raise ValidationError(f"duplicate item id {item.id!r}")
            index[item.id] = i

        parent: list[NodeId | None] = [None] * n
        children: list[list[NodeId]] = [[] for _ in range(n)]
        roots: list[NodeId] = []
        for i, item in enumerate(doc.items):
            if item.parent is None:
                roots.append(i)
                continue
            pi = index.get(item.parent)
            if pi is None:
                raise ValidationError(f"item {item.id!r} has unknown parent {item.parent!r}")
            parent[i] = pi
            children[pi].append(i)
        for i, item in enumerate(doc.items):
            cur, steps = i, 0
            while parent[cur] is not None:
                cur = parent[cur]  # type: ignore[assignment]
                steps += 1
                if steps > n:
                    raise ValidationError(f"parent cycle involving item {item.id!r}")

        links: list[ResolvedLink] = []
        for l in doc.links:
            if l.source not in index:
                raise ValidationError(f"{l.type} link {l.source!r} -> {l.target!r}: unknown source")
            if l.target not in index:
                raise ValidationError(f"{l.type} link {l.source!r} -> {l.target!r}: unknown target")
            if not l.type:
                raise ValidationError(f"link {l.source!r} -> {l.target!r} has an empty type")
            links.append(ResolvedLink(index[l.source], index[l.target], l.type))

        self._items = list(doc.items)
        self._index = index
        self._parent = parent
        self._children = children
        self._roots = roots
        self._links = links
        self._link_types = sorted({l.type for l in links})

    def __len__(self) -> int:
        return len(self._items)

    def item(self, n: NodeId) -> Item:
        return self._items[n]

    def find(self, item_id: str) -> NodeId | None:
        return self._index.get(item_id)

    def parent(self, n: NodeId) -> NodeId | None:
        return self._parent[n]

    def children(self, n: NodeId) -> list[NodeId]:
        return self._children[n]

    @property
    def roots(self) -> list[NodeId]:
        return self._roots

    def top_children(self, root: NodeId | None) -> list[NodeId]:
        """Children of ``root``, or the top-level items when ``root`` is None."""
        return self._roots if root is None else self._children[root]

    @property
    def links(self) -> list[ResolvedLink]:
        return self._links

    @property
    def link_types(self) -> list[str]:
        """Distinct link types present in the data, sorted."""
        return self._link_types

    def ancestors(self, n: NodeId) -> list[NodeId]:
        """Ancestors of ``n``, nearest first."""
        out = []
        cur = self._parent[n]
        while cur is not None:
            out.append(cur)
            cur = self._parent[cur]
        return out

    def path_from_root(self, n: NodeId) -> list[NodeId]:
        path = self.ancestors(n)
        path.reverse()
        path.append(n)
        return path

    def is_ancestor(self, a: NodeId, n: NodeId) -> bool:
        """True when ``a`` is a strict ancestor of ``n``."""
        cur = self._parent[n]
        while cur is not None:
            if cur == a:
                return True
            cur = self._parent[cur]
        return False

    def descendants(self, n: NodeId) -> list[NodeId]:
        """All strict descendants of ``n``, depth first."""
        out = []
        stack = list(reversed(self._children[n]))
        while stack:
            c = stack.pop()
            out.append(c)
            stack.extend(reversed(self._children[c]))
        return out
