# Clio viewer

A terminal viewer for Clio task hierarchies (**Milestone → Epic → Story → Subtask**)
that behaves like an RTL/schematic viewer:

- Each item is a **box**; open it **inline** (it grows in place and shows its
  children as nested boxes) or **drill in** (it becomes the whole view, with a
  breadcrumb back out).
- Relationships (`blocks`, `relates_to`, `duplicates`, … — whatever types the data
  contains) are drawn as **wires** between boxes. Each type can be toggled at runtime.
- Wires are **lifted** like hierarchical nets: a link to something inside a
  collapsed box is drawn to that box, and parallel links merge into one wire
  labelled `×N`.
- Siblings are laid out in **layered columns** so dependencies flow left → right;
  wires are routed orthogonally around unrelated boxes.
- **Zoom** switches between detail / compact / overview box sizes.

```
  ╔═ M1 Beta launch ════════════════════════════════════════════[-]╗
  ║                             ┌─────────────────────────────────────────┐
  ║  ┏━━━━━━━━━━━━━━━━━━━━━━━━┓ │      ┏━━━━━━━━━━━━━━━━━━━━━━━━┓  ║      │ ╔════════════════════════╗
  ║  ┃E1 · epic               ├─┘      ┃E2 · epic               ├─────┐  ┌╌▶┤M2 · milestone          ║
  ║  ┃Auth & accounts         ├╌┐ ┌───▶┤Task board              ├╌╌╌╌┐└────▶┤GA release              ║
  ║  ┃◐ in progress       @lee├───┘    ┃○ todo              @sam┃  ║ └╌╌╌╌╌▶┤○ todo             @dana║
  ║  ┗━━━━━━━━━━━━━━━━━━━━[+3]┛ ╎      ┗━━━━━━━━━━━━━━━━━━━━[+3]┛  ║     ╎  ╚════════════════════[+2]╝
  ║                             └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
  ╚════════════════════════════════════════════════════════════════╝
```

Border style shows the kind (═ milestone, ━ epic, ╭╮ story, ┌┐ subtask); border
colour shows status (grey todo, yellow in progress, green done, red blocked).

## Layout of this folder

| Path | What |
|---|---|
| `schema/clio-tasks.schema.json` | Canonical data format shared by every implementation |
| `fixtures/sample.json` | Sample hierarchy (2 milestones, 4 epics, 12 stories, 8 subtasks, 3 link types) |
| `fixtures/jira_search_sample.json` | Recorded Jira search response, used by the adapter tests |
| `rust/` | Rust + [ratatui](https://ratatui.rs) implementation |
| `python/` | Python + [Textual](https://textual.textualize.io) implementation |

## Data format

A flat list of items with parent references, plus typed links:

```json
{
  "version": 1,
  "items": [
    { "id": "M1", "kind": "milestone", "title": "Beta launch", "status": "in_progress", "assignee": "dana" },
    { "id": "E1", "kind": "epic", "title": "Auth & accounts", "parent": "M1" }
  ],
  "links": [
    { "from": "S1", "to": "S3", "type": "blocks" }
  ]
}
```

`kind` ∈ milestone | epic | story | subtask; `status` ∈ todo | in_progress | done | blocked
(default todo). Link `type` is free-form snake_case. Loaders reject duplicate ids,
unknown parents or link endpoints, and parent cycles.

## Rust implementation

```sh
cd viewer/rust
cargo run -- --file ../fixtures/sample.json
```

### Jira

The Jira adapter calls `GET /rest/api/3/search/jql`. It maps issue types to kinds (by name, falling back to hierarchy level),
`parent` to parent, `issuelinks` to links (using the outward name, e.g. `relates_to`),
and status category to status. It needs the `jira` feature:

```sh
export JIRA_URL=https://your-site.atlassian.net JIRA_EMAIL=you@example.com JIRA_API_TOKEN=...
cargo run --features jira -- --jira --jql 'project = CLIO'
```

### Headless snapshots

`--snapshot WxH` renders one frame to stdout instead of running interactively;
`--keys` replays keystrokes first (`<space>`, `<enter>`, `<esc>`, `<S-left>`, …):

```sh
cargo run -- --file ../fixtures/sample.json --snapshot 160x40 --keys 'e-'
```

### Tests

```sh
cargo test --all-features
```

These cover model validation, layout geometry (no sibling overlap, children inside parents,
edge lifting, edge routing around boxes, filters, drill-in scope), Jira mapping against the
recorded fixture, and UI behaviour through ratatui's `TestBackend`.

## Python implementation

Same features, same keys, and **the same frames**. Layout and rendering are a direct port
of the Rust code, and `tests/test_parity.py` checks that both viewers produce byte-identical
snapshots. Popups are native Textual screens: the help overlay, the relationship filters
(a `SelectionList`) and the search prompt (an `Input`). The details panel is a docked widget.

```sh
cd viewer/python
python3 -m venv .venv && .venv/bin/pip install -e '.[test]'
.venv/bin/clio-viewer --file ../fixtures/sample.json
```

Jira needs no extra dependencies here (it uses the standard library's HTTP client):

```sh
.venv/bin/clio-viewer --jira --jql 'project = CLIO'   # same JIRA_* env vars as above
```

`--snapshot WxH` and `--keys` work as in Rust (normal-mode keys only; popups are
interactive).

### Tests

```sh
.venv/bin/python -m pytest
```

The suite covers model validation, layout geometry and routing, Jira mapping and pagination
(with a mocked HTTP client), view state, Textual UI behaviour through `Pilot`, and the Rust↔Python
parity check. The parity check runs only when the Rust binary has been built (`cargo build`
in `viewer/rust`) or `CLIO_RUST_BIN` is set.

## Keys

| Key | Action |
|---|---|
| arrows / `hjkl` | move selection (→/↓ on an open box steps into it) |
| Shift+arrows / `HJKL` | pan |
| `+` / `-` | zoom in / out |
| space | expand / collapse in place |
| enter | drill into box |
| esc / backspace | drill out |
| `e` / `E` | expand all / collapse all (within the current view) |
| `p` | select parent |
| `c` | center on selection |
| `r` | relationship filters (space toggle, `a` all, `n` none) |
| `d` | details panel |
| `/`, `n` | search id/title, next match |
| `R` | reload from the data source |
| `?` | help |
| `q` | quit |
