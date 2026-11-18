//! Data sources. The viewer is offline: documents come from local JSON files.

pub mod file;

use anyhow::Result;

use crate::model::Document;

pub use file::FileLoader;

pub trait Loader {
    fn load(&self) -> Result<Document>;
    /// Short human-readable description of the source, shown in the UI.
    fn describe(&self) -> String;
}
