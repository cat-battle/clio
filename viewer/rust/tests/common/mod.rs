#![allow(dead_code)]

use clio_viewer::adapters::{FileLoader, Loader};
use clio_viewer::model::{Document, Graph};

pub fn fixture(name: &str) -> String {
    format!("{}/../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

pub fn sample_doc() -> Document {
    FileLoader::new(fixture("sample.json")).load().expect("sample fixture loads")
}

pub fn sample_graph() -> Graph {
    Graph::from_document(sample_doc()).expect("sample fixture is valid")
}
