"""Jira Cloud adapter.

:func:`map_issues` (no network) turns raw issues from
``GET /rest/api/3/search/jql`` into a Clio :class:`Document`;
:class:`JiraLoader` fetches them using only the standard library.
"""

from __future__ import annotations

import base64
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

from ..model import FORMAT_VERSION, Document, Item, Kind, Link, Status, ValidationError

FIELDS = "summary,status,issuetype,parent,assignee,issuelinks,description"


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """Refuses redirects: urllib would resend the Authorization header to
    whatever host (or plain-http URL) the Location header names."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[override]
        return None  # surfaces as HTTPError(3xx)


_OPENER = urllib.request.build_opener(_NoRedirect)


class JiraLoader:
    def __init__(self, base_url: str, email: str, token: str, jql: str) -> None:
        self.base_url = base_url
        self.email = email
        self.token = token
        self.jql = jql

    @classmethod
    def from_env(cls, jql: str) -> JiraLoader:
        """Reads ``JIRA_URL``, ``JIRA_EMAIL`` and ``JIRA_API_TOKEN``."""
        values = {}
        for k in ("JIRA_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"):
            v = os.environ.get(k)
            if not v:
                raise ValidationError(f"{k} is not set")
            values[k] = v
        return cls(values["JIRA_URL"], values["JIRA_EMAIL"], values["JIRA_API_TOKEN"], jql)

    def load(self) -> Document:
        return map_issues(self.fetch_issues())

    def describe(self) -> str:
        return f"jira: {self.jql}"

    def fetch_issues(self) -> list[dict[str, Any]]:
        if urllib.parse.urlsplit(self.base_url).scheme.lower() != "https":
            raise ValidationError("JIRA_URL must start with https://")
        endpoint = f"{self.base_url.rstrip('/')}/rest/api/3/search/jql"
        auth = base64.b64encode(f"{self.email}:{self.token}".encode()).decode()
        issues: list[dict[str, Any]] = []
        page_token: str | None = None
        for _ in range(1000):
            params = {"jql": self.jql, "fields": FIELDS, "maxResults": "100"}
            if page_token:
                params["nextPageToken"] = page_token
            req = urllib.request.Request(
                f"{endpoint}?{urllib.parse.urlencode(params)}",
                headers={"Authorization": f"Basic {auth}", "Accept": "application/json"},
            )
            try:
                with _OPENER.open(req, timeout=30) as resp:
                    page = json.load(resp)
            except urllib.error.HTTPError as e:
                raise ValidationError(f"Jira returned HTTP {e.code}: {e.reason}") from e
            except urllib.error.URLError as e:
                raise ValidationError(f"calling Jira: {e.reason}") from e
            issues.extend(page.get("issues") or [])
            page_token = page.get("nextPageToken")
            if page.get("isLast") is True or not page_token:
                return issues
        raise ValidationError("Jira pagination did not terminate after 1000 pages")


def _get(v: Any, *path: str) -> Any:
    for p in path:
        if not isinstance(v, dict):
            return None
        v = v.get(p)
    return v


def map_issues(issues: list[dict[str, Any]]) -> Document:
    """Maps raw Jira issues to a Clio document.

    Parents and links that point outside the fetched set are dropped, so the
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
        description = _adf_text(f.get("description"))
        items.append(
            Item(
                id=key,
                kind=_map_kind(f.get("issuetype") or {}),
                title=f.get("summary") or "",
                status=_map_status(f.get("status") or {}),
                parent=parent if parent in keys else None,
                assignee=_get(f, "assignee", "displayName"),
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
