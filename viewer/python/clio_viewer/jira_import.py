"""Offline converter from saved Jira search results to a Clio plan file.

The viewer never talks to Jira. Save the issues however you like (the REST
search API, an export, a script) and convert them::

    clio-jira-import page1.json page2.json -o plan.json
    clio-viewer --file plan.json

Each input file is either a Jira search response (``{"issues": [...], ...}``,
as returned by ``GET /rest/api/3/search/jql``) or a bare list of issues. Each
issue needs ``key`` and ``fields`` with ``summary``, ``status``, ``issuetype``,
``parent``, ``assignee``, ``issuelinks`` and ``description`` (missing fields
fall back to defaults). This module does no network I/O.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

from .model import FORMAT_VERSION, Document, Graph, Item, Kind, Link, Status, ValidationError, sanitize


def load_issues(paths: list[Path]) -> list[dict[str, Any]]:
    """Reads saved search pages (or bare issue lists) and concatenates their
    issues. An issue key seen twice keeps its first occurrence."""
    issues: list[dict[str, Any]] = []
    seen: set[str] = set()
    for p in paths:
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as e:
            raise ValidationError(f"{p}: {e}") from e
        page = data.get("issues") if isinstance(data, dict) else data
        if not isinstance(page, list):
            raise ValidationError(f"{p}: expected a Jira search response with an 'issues' list, or a list of issues")
        for issue in page:
            key = issue.get("key") if isinstance(issue, dict) else None
            if not isinstance(key, str):
                raise ValidationError(f"{p}: Jira issue without a key")
            if key not in seen:
                seen.add(key)
                issues.append(issue)
    return issues


def _get(v: Any, *path: str) -> Any:
    for p in path:
        if not isinstance(v, dict):
            return None
        v = v.get(p)
    return v


def map_issues(issues: list[dict[str, Any]]) -> Document:
    """Maps raw Jira issues to a Clio document.

    Parents and links that point outside the given issues are dropped, so the
    result always passes :class:`~clio_viewer.model.Graph` validation.
    """
    keys = {k for i in issues if isinstance(k := i.get("key"), str)}
    items: list[Item] = []
    links: list[Link] = []
    seen: set[tuple[str, str, str]] = set()
    for issue in issues:
        key = issue.get("key")
        if not isinstance(key, str):
            raise ValidationError("Jira issue without a key")
        f = issue.get("fields") or {}
        parent = _get(f, "parent", "key")
        assignee = _get(f, "assignee", "displayName")
        description = _adf_text(f.get("description"))
        items.append(
            Item(
                id=key,
                kind=_map_kind(f.get("issuetype") or {}),
                title=str(f.get("summary") or ""),
                status=_map_status(f.get("status") or {}),
                parent=parent if parent in keys else None,
                assignee=assignee if isinstance(assignee, str) else None,
                description=description or None,
            )
        )
        for l in f.get("issuelinks") or []:
            if out := _get(l, "outwardIssue", "key"):
                src, dst = key, out
            elif inw := _get(l, "inwardIssue", "key"):
                src, dst = inw, key
            else:
                continue
            if src not in keys or dst not in keys:
                continue
            ty = l.get("type") or {}
            name = snake_case(ty.get("outward") or ty.get("name") or "relates to")
            if (src, dst, name) not in seen:
                seen.add((src, dst, name))
                links.append(Link(src, dst, name))
    return Document(version=FORMAT_VERSION, items=items, links=links)


def _map_kind(issuetype: dict[str, Any]) -> Kind:
    name = (issuetype.get("name") or "").lower()
    if name in ("milestone", "initiative"):
        return Kind.MILESTONE
    if name == "epic":
        return Kind.EPIC
    if "sub" in name:
        return Kind.SUBTASK
    level = issuetype.get("hierarchyLevel")
    if isinstance(level, int):
        if level < 0:
            return Kind.SUBTASK
        if level == 1:
            return Kind.EPIC
        if level >= 2:
            return Kind.MILESTONE
    return Kind.STORY


def _map_status(status: dict[str, Any]) -> Status:
    if "block" in (status.get("name") or "").lower():
        return Status.BLOCKED
    return {"done": Status.DONE, "indeterminate": Status.IN_PROGRESS}.get(
        _get(status, "statusCategory", "key"), Status.TODO
    )


def snake_case(s: str) -> str:
    """``"is blocked by"`` -> ``"is_blocked_by"``"""
    return re.sub(r"[^0-9a-z]+", "_", s.strip().lower()).strip("_")


def _adf_text(v: Any) -> str:
    """Flattens an Atlassian Document Format value (or a plain string) to text."""
    if isinstance(v, str):
        return v.strip()
    out: list[str] = []

    def walk(node: Any) -> None:
        if not isinstance(node, dict):
            return
        if isinstance(node.get("text"), str):
            out.append(node["text"])
        content = node.get("content")
        if isinstance(content, list):
            for c in content:
                walk(c)
            if node.get("type") in ("paragraph", "heading"):
                out.append("\n")

    walk(v)
    return "".join(out).strip()


def convert(paths: list[Path]) -> Document:
    """Saved Jira pages -> a validated, sanitised Clio document."""
    doc = sanitize(map_issues(load_issues(paths)))
    Graph(doc)  # raises ValidationError if the result would not load
    return doc


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(
        prog="clio-jira-import",
        description="Convert saved Jira search results (JSON) into a Clio plan file. Works offline.",
    )
    p.add_argument("inputs", nargs="+", type=Path, metavar="PAGE.json", help="saved Jira search response(s) or issue list(s)")
    p.add_argument("-o", "--output", type=Path, metavar="PLAN.json", help="where to write the plan (default: stdout)")
    args = p.parse_args(argv)
    try:
        doc = convert(args.inputs)
    except ValidationError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    text = json.dumps(doc.to_dict(), indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
        print(f"wrote {len(doc.items)} items and {len(doc.links)} links to {args.output}", file=sys.stderr)
    else:
        if hasattr(sys.stdout, "reconfigure"):
            sys.stdout.reconfigure(encoding="utf-8")
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
