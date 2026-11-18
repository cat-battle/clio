use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::crossterm::event::{self, Event};

use clio_viewer::adapters::{FileLoader, JiraLoader, Loader};
use clio_viewer::app::App;
use clio_viewer::keys::parse_keys;
use clio_viewer::model::Graph;
use clio_viewer::ui;

/// Schematic-style TUI for Clio task hierarchies (Milestone → Epic → Story → Subtask).
#[derive(Parser, Debug)]
#[command(name = "clio-viewer", version)]
struct Cli {
    /// Load a Clio task document (JSON, see viewer/schema).
    #[arg(long, value_name = "PATH", conflicts_with = "jira", required_unless_present = "jira")]
    file: Option<PathBuf>,

    /// Load from Jira Cloud (needs JIRA_URL, JIRA_EMAIL, JIRA_API_TOKEN and a build with `--features jira`).
    #[arg(long, requires = "jql")]
    jira: bool,

    /// JQL query selecting the issues to show (with --jira).
    #[arg(long, value_name = "QUERY")]
    jql: Option<String>,

    /// Render one frame of WIDTHxHEIGHT to stdout instead of running interactively.
    #[arg(long, value_name = "WxH")]
    snapshot: Option<String>,

    /// Keys to replay before the snapshot, e.g. "<space>j<enter>" (see src/keys.rs).
    #[arg(long, value_name = "SCRIPT", requires = "snapshot")]
    keys: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let loader: Box<dyn Loader> = if cli.jira {
        Box::new(JiraLoader::from_env(cli.jql.clone().unwrap_or_default())?)
    } else {
        Box::new(FileLoader::new(cli.file.clone().expect("clap enforces --file")))
    };
    let doc = loader
        .load()
        .with_context(|| format!("loading {}", loader.describe()))?;
    let graph = Graph::from_document(doc)?;
    let mut app = App::new(graph, Some(loader));

    if let Some(spec) = &cli.snapshot {
        let (w, h) = parse_size(spec)?;
        let keys = parse_keys(cli.keys.as_deref().unwrap_or(""))?;
        print!("{}", ui::snapshot(&mut app, w, h, &keys)?);
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        while !app.should_quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if let Event::Key(k) = event::read()? {
                app.handle_key(k);
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}

fn parse_size(spec: &str) -> Result<(u16, u16)> {
    let (w, h) = spec
        .split_once(['x', 'X'])
        .with_context(|| format!("expected WIDTHxHEIGHT, got {spec:?}"))?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}
