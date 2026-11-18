import json

import pytest

from clio_viewer.adapters import FileLoader
from clio_viewer.jira_import import convert, main, map_issues, snake_case
from clio_viewer.model import Graph, Kind, Status, ValidationError

from .conftest import FIXTURES

PAGE = FIXTURES / "jira_search_sample.json"


def load_page():
    return json.loads(PAGE.read_text())


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


def test_pages_are_merged_and_bare_lists_accepted(tmp_path):
    issues = load_page()["issues"]
    a = tmp_path / "a.json"
    b = tmp_path / "b.json"
    a.write_text(json.dumps({"issues": issues[:3], "nextPageToken": "x"}))
    b.write_text(json.dumps(issues[2:]))  # bare list, CLIO-3 repeated
    doc = convert([a, b])
    assert [it.id for it in doc.items] == [i["key"] for i in issues]
    # With every page present, the cross-page parent and links resolve.
    assert next(it for it in doc.items if it.id == "CLIO-5").parent == "CLIO-4"
    assert len(doc.links) == 2


def test_cli_writes_a_plan_the_viewer_loads(tmp_path, capsys):
    out = tmp_path / "plan.json"
    assert main([str(PAGE), "-o", str(out)]) == 0
    assert "wrote 6 items and 2 links" in capsys.readouterr().err
    g = Graph(FileLoader(out).load())
    assert len(g) == 6


def test_output_is_sanitised(tmp_path, capsys):
    issues = load_page()["issues"]
    issues[0]["fields"]["summary"] = "Hi\x1b]52;c;eA==\x07"
    src = tmp_path / "page.json"
    src.write_text(json.dumps(issues))
    assert main([str(src)]) == 0
    plan = json.loads(capsys.readouterr().out)
    assert plan["items"][0]["title"] == "Hi ]52;c;eA== "


@pytest.mark.parametrize("content", ['{"total": 3}', "42", "[{\"fields\": {}}]", "not json"])
def test_rejects_malformed_input(tmp_path, content):
    src = tmp_path / "bad.json"
    src.write_text(content)
    with pytest.raises(ValidationError):
        convert([src])
