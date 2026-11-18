mod common;

use clio_viewer::model::{Graph, Kind, Link};

#[test]
fn sample_fixture_indexes_hierarchy_and_link_types() {
    let g = common::sample_graph();
    assert_eq!(g.roots().len(), 2);
    let m1 = g.find("M1").unwrap();
    let ids: Vec<&str> = g.children(m1).iter().map(|&c| g.item(c).id.as_str()).collect();
    assert_eq!(ids, ["E1", "E2"]);
    let t1 = g.find("T1").unwrap();
    assert_eq!(g.item(t1).kind, Kind::Subtask);
    assert!(g.is_ancestor(m1, t1));
    assert_eq!(g.link_types(), ["blocks", "duplicates", "relates_to"]);
    assert_eq!(g.descendants(m1).len(), 2 + 6 + 6);
}

#[test]
fn rejects_dangling_parent() {
    let mut doc = common::sample_doc();
    doc.items[2].parent = Some("NOPE".into());
    let err = Graph::from_document(doc).unwrap_err().to_string();
    assert!(err.contains("unknown parent"), "{err}");
}

#[test]
fn rejects_dangling_link() {
    let mut doc = common::sample_doc();
    doc.links.push(Link {
        from: "S1".into(),
        to: "GHOST".into(),
        kind: "blocks".into(),
    });
    let err = Graph::from_document(doc).unwrap_err().to_string();
    assert!(err.contains("unknown target"), "{err}");
}

#[test]
fn rejects_duplicate_ids_and_cycles() {
    let mut doc = common::sample_doc();
    let dup = doc.items[0].clone();
    doc.items.push(dup);
    assert!(Graph::from_document(doc).unwrap_err().to_string().contains("duplicate"));

    let mut doc = common::sample_doc();
    let m1 = doc.items.iter().position(|i| i.id == "M1").unwrap();
    doc.items[m1].parent = Some("E1".into()); // M1 -> E1 -> M1
    assert!(Graph::from_document(doc).unwrap_err().to_string().contains("cycle"));
}

#[test]
fn rejects_unknown_fields_and_versions() {
    let bad = r#"{"version":1,"items":[{"id":"A","kind":"epic","title":"x","colour":"red"}]}"#;
    assert!(serde_json::from_str::<clio_viewer::model::Document>(bad).is_err());
    let mut doc = common::sample_doc();
    doc.version = 2;
    assert!(Graph::from_document(doc).is_err());
}

#[test]
fn control_characters_are_replaced() {
    let mut doc = common::sample_doc();
    let it = &mut doc.items[2];
    it.title = "Hi\x1b]52;c;eA==\x07".into();
    it.assignee = Some("bob\u{9b}".into());
    it.description = Some("a\r\nb\x1b".into());
    let id = it.id.clone();
    let g = Graph::from_document(doc).unwrap();
    let got = g.item(g.find(&id).unwrap());
    assert_eq!(got.title, "Hi ]52;c;eA== ");
    assert_eq!(got.assignee.as_deref(), Some("bob "));
    assert_eq!(got.description.as_deref(), Some("a \nb "));
}
