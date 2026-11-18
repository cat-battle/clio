mod common;

use clio_viewer::adapters::jira::map_issues;
use clio_viewer::model::{Graph, Kind, Status};

#[test]
fn maps_recorded_jira_search_response() {
    let raw = std::fs::read_to_string(common::fixture("jira_search_sample.json")).unwrap();
    let page: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let doc = map_issues(page["issues"].as_array().unwrap()).unwrap();
    let g = Graph::from_document(doc).expect("mapped document validates");

    let item = |k: &str| g.item(g.find(k).unwrap());
    assert_eq!(item("CLIO-1").kind, Kind::Milestone);
    assert_eq!(item("CLIO-2").kind, Kind::Epic);
    assert_eq!(item("CLIO-3").kind, Kind::Story);
    assert_eq!(item("CLIO-4").kind, Kind::Story); // "Task" at hierarchy level 0
    assert_eq!(item("CLIO-5").kind, Kind::Subtask);

    assert_eq!(item("CLIO-1").status, Status::InProgress);
    assert_eq!(item("CLIO-2").status, Status::Todo);
    assert_eq!(item("CLIO-3").status, Status::Done);
    assert_eq!(item("CLIO-4").status, Status::Blocked);

    assert_eq!(item("CLIO-2").parent.as_deref(), Some("CLIO-1"));
    assert_eq!(item("CLIO-5").parent.as_deref(), Some("CLIO-4"));
    assert_eq!(item("CLIO-6").parent, None, "parent outside the result set is dropped");

    assert_eq!(item("CLIO-1").assignee.as_deref(), Some("Dana"));
    assert_eq!(
        item("CLIO-1").description.as_deref(),
        Some("First public beta.\nAccounts and board.")
    );

    // Each link appears on both issues but is mapped once; the link to
    // CLIO-99 (not fetched) is dropped.
    let mut links: Vec<(String, String, String)> = g
        .links()
        .iter()
        .map(|l| (g.item(l.from).id.clone(), g.item(l.to).id.clone(), l.kind.clone()))
        .collect();
    links.sort();
    assert_eq!(
        links,
        vec![
            ("CLIO-3".into(), "CLIO-4".into(), "blocks".into()),
            ("CLIO-5".into(), "CLIO-4".into(), "duplicates".into()),
        ]
    );
}
