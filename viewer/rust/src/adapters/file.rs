use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use super::Loader;
use crate::model::Document;

/// Loads a Clio task document from a local JSON file.
pub struct FileLoader {
    pub path: PathBuf,
}

impl FileLoader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Loader for FileLoader {
    fn load(&self) -> Result<Document> {
        let text = fs::read_to_string(&self.path)
            .with_context(|| format!("reading {}", self.path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", self.path.display()))
    }

    fn describe(&self) -> String {
        self.path.display().to_string()
    }
}
