use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use ratatui::crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use ratatui::crossterm::execute;

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

    /// With --snapshot: also print per-cell styles (for parity tests).
    #[arg(long, requires = "snapshot")]
    styles: bool,
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
        if cli.styles {
            let (text, styles) = ui::snapshot_styles(&mut app, w, h, &keys)?;
            print!("{text}--- styles ---\n{styles}");
        } else {
            print!("{}", ui::snapshot(&mut app, w, h, &keys)?);
        }
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        execute!(std::io::stdout(), EnableMouseCapture)?;
        while !app.should_quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            match event::read()? {
                Event::Key(k) => app.handle_key(k),
                Event::Mouse(m) => app.handle_mouse(m),
                _ => {}
            }
        }
        Ok(())
    })();
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn parse_size(spec: &str) -> Result<(u16, u16)> {
    let (w, h) = spec
        .split_once(['x', 'X'])
        .with_context(|| format!("expected WIDTHxHEIGHT, got {spec:?}"))?;
    Ok((w.trim().parse()?, h.trim().parse()?))
}
