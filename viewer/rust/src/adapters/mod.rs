//! Pluggable data sources. Every loader produces the same [`Document`].

pub mod file;
pub mod jira;

use anyhow::Result;

use crate::model::Document;

pub use file::FileLoader;
pub use jira::JiraLoader;

pub trait Loader {
    fn load(&self) -> Result<Document>;
    /// Short human-readable description of the source, shown in the UI.
    fn describe(&self) -> String;
}
