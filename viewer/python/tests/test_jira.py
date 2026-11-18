import json
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer
from unittest import mock

import pytest

from clio_viewer.adapters import jira
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
    with mock.patch.object(jira._OPENER, "open", fake_urlopen):
        doc = loader.load()
    assert len(seen_urls) == 2
    assert seen_urls[0].startswith("https://x.atlassian.net/rest/api/3/search/jql?")
    assert "nextPageToken=tok" in seen_urls[1]
    assert len(doc.items) == 6


@pytest.mark.parametrize("url", ["http://x.atlassian.net", "file:///etc/passwd", "x.atlassian.net"])
def test_fetch_requires_https(url):
    loader = JiraLoader(url, "me@example.com", "secret", "project = CLIO")
    with mock.patch.object(jira._OPENER, "open", side_effect=AssertionError("no request expected")):
        with pytest.raises(ValidationError, match="must start with https://"):
            loader.fetch_issues()


def _serve(handler_cls):
    server = HTTPServer(("127.0.0.1", 0), handler_cls)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def test_redirects_are_not_followed():
    hits = []

    class Target(BaseHTTPRequestHandler):
        def do_GET(self):
            hits.append(self.headers.get("Authorization"))
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"{}")

        def log_message(self, *a):
            pass

    target = _serve(Target)
    target_url = f"http://localhost:{target.server_address[1]}/stolen"

    class Redirector(BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(302)
            self.send_header("Location", target_url)
            self.end_headers()

        def log_message(self, *a):
            pass

    redirector = _serve(Redirector)
    try:
        req = urllib.request.Request(
            f"http://127.0.0.1:{redirector.server_address[1]}/rest/api/3/search/jql",
            headers={"Authorization": "Basic c2VjcmV0"},
        )
        with pytest.raises(urllib.error.HTTPError) as err:
            jira._OPENER.open(req, timeout=5)
        assert err.value.code == 302
        err.value.close()
        assert hits == []
    finally:
        redirector.shutdown()
        target.shutdown()
        redirector.server_close()
        target.server_close()
