import json
from unittest import mock

import pytest

from clio_viewer.adapters.jira import JiraLoader, map_issues, snake_case
from clio_viewer.model import Graph, Kind, Status, ValidationError

from .conftest import FIXTURES


def load_page():
    return json.loads((FIXTURES / "jira_search_sample.json").read_text())


def test_maps_recorded_jira_search_response():
    g = Graph(map_issues(load_page()["issues"]))
    item = lambda k: g.item(g.find(k))  # noqa: E731
    assert [item(f"CLIO-{i}").kind for i in range(1, 6)] == [
        Kind.MILESTONE, Kind.EPIC, Kind.STORY, Kind.STORY, Kind.SUBTASK,
    ]
    assert [item(f"CLIO-{i}").status for i in range(1, 5)] == [
        Status.IN_PROGRESS, Status.TODO, Status.DONE, Status.BLOCKED,
    ]
    assert item("CLIO-2").parent == "CLIO-1"
    assert item("CLIO-5").parent == "CLIO-4"
    assert item("CLIO-6").parent is None, "parent outside the result set is dropped"
    assert item("CLIO-1").assignee == "Dana"
    assert item("CLIO-1").description == "First public beta.\nAccounts and board."
    links = sorted((g.item(l.source).id, g.item(l.target).id, l.type) for l in g.links)
    assert links == [("CLIO-3", "CLIO-4", "blocks"), ("CLIO-5", "CLIO-4", "duplicates")]


def test_snake_case():
    assert snake_case("relates to") == "relates_to"
    assert snake_case("Is Blocked By") == "is_blocked_by"
    assert snake_case("  clones -> ") == "clones"


def test_from_env_requires_credentials(monkeypatch):
    for k in ("JIRA_URL", "JIRA_EMAIL", "JIRA_API_TOKEN"):
        monkeypatch.delenv(k, raising=False)
    with pytest.raises(ValidationError, match="JIRA_URL is not set"):
        JiraLoader.from_env("project = X")


def test_fetch_paginates_with_next_page_token():
    page = load_page()
    first = {"issues": page["issues"][:3], "nextPageToken": "tok", "isLast": False}
    second = {"issues": page["issues"][3:], "isLast": True}
    seen_urls = []

    class Resp:
        def __init__(self, body):
            self.body = json.dumps(body).encode()

        def read(self, *a):
            return self.body

        def __enter__(self):
            return self

        def __exit__(self, *a):
            return False

    def fake_urlopen(req, timeout):
        seen_urls.append(req.full_url)
        assert req.headers["Authorization"].startswith("Basic ")
        return Resp(first if len(seen_urls) == 1 else second)

    loader = JiraLoader("https://x.atlassian.net/", "me@example.com", "secret", "project = CLIO")
    with mock.patch("urllib.request.urlopen", fake_urlopen):
        doc = loader.load()
    assert len(seen_urls) == 2
    assert seen_urls[0].startswith("https://x.atlassian.net/rest/api/3/search/jql?")
    assert "nextPageToken=tok" in seen_urls[1]
    assert len(doc.items) == 6
