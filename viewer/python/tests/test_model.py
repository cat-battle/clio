import json

import pytest

from clio_viewer.adapters import FileLoader
from clio_viewer.model import Document, Graph, Kind, Link, ValidationError

from .conftest import sample_doc


def test_sample_fixture_indexes_hierarchy_and_link_types(graph):
    assert len(graph.roots) == 2
    m1 = graph.find("M1")
    assert [graph.item(c).id for c in graph.children(m1)] == ["E1", "E2"]
    t1 = graph.find("T1")
    assert graph.item(t1).kind is Kind.SUBTASK
    assert graph.is_ancestor(m1, t1)
    assert graph.link_types == ["blocks", "duplicates", "relates_to"]
    assert len(graph.descendants(m1)) == 2 + 6 + 6


def test_rejects_dangling_parent():
    doc = sample_doc()
    doc.items[2].parent = "NOPE"
    with pytest.raises(ValidationError, match="unknown parent"):
        Graph(doc)


def test_rejects_dangling_link():
    doc = sample_doc()
    doc.links.append(Link("S1", "GHOST", "blocks"))
    with pytest.raises(ValidationError, match="unknown target"):
        Graph(doc)


def test_rejects_duplicate_ids_and_cycles():
    doc = sample_doc()
    doc.items.append(doc.items[0])
    with pytest.raises(ValidationError, match="duplicate"):
        Graph(doc)
    doc = sample_doc()
    next(i for i in doc.items if i.id == "M1").parent = "E1"  # M1 -> E1 -> M1
    with pytest.raises(ValidationError, match="cycle"):
        Graph(doc)


def test_rejects_unknown_fields_bad_enums_and_versions():
    bad = {"version": 1, "items": [{"id": "A", "kind": "epic", "title": "x", "colour": "red"}]}
    with pytest.raises(ValidationError, match="unknown field"):
        Document.from_dict(bad)
    with pytest.raises(ValidationError):
        Document.from_dict({"version": 1, "items": [{"id": "A", "kind": "saga", "title": "x"}]})
    doc = sample_doc()
    doc.version = 2
    with pytest.raises(ValidationError, match="version"):
        Graph(doc)


def test_file_loader_errors_are_validation_errors(tmp_path):
    with pytest.raises(ValidationError, match="reading"):
        FileLoader(tmp_path / "missing.json").load()
    p = tmp_path / "broken.json"
    p.write_text("{not json")
    with pytest.raises(ValidationError, match="parsing"):
        FileLoader(p).load()


def test_round_trip_to_dict():
    doc = sample_doc()
    again = Document.from_dict(json.loads(json.dumps(doc.to_dict())))
    assert again == doc
