# Clio viewer

A terminal viewer for Clio task hierarchies (**Milestone → Epic → Story → Subtask**)
that behaves like an RTL/schematic viewer. It comes in two implementations, Rust
(ratatui) and Python (Textual), with the same features, the same keys and
identical screens.

- Each item is a **box**. Open it **inline** (it grows in place and shows its
  children as nested boxes) or **drill in** (it becomes the whole view, with a
  breadcrumb back out).
- Relationships (`blocks`, `relates_to`, `duplicates`, …, whatever types the data
  contains) are drawn as **wires** between boxes. Each type can be toggled at runtime.
- Wires are **lifted** like hierarchical nets: a link to something inside a
  collapsed box is drawn to that box, and parallel links merge into one wire
  labelled `×N`.
- Siblings are laid out in **layered columns** so dependencies flow left → right.
  Wires are routed orthogonally around unrelated boxes and **never leave the box
  that contains both of their ends**.
- **Zoom** switches between detail / compact / overview box sizes. **Auto-fit**
  (on by default) picks the most detailed size at which the whole view fits, then
  **stretches the layout to fill the terminal** (wider boxes, wrapped titles,
  description previews, wider spacing) and refits when the window is resized.
- **Dependency tracing** (`t`) lights up everything the selected box depends on
  and everything that depends on it, and greys out the rest, like fan-in/fan-out
  cone tracing in an RTL viewer.
- **Mouse:** click to select, click `[+N]` / an open box's title bar to open or
  close it, double right-click to close the open box under the pointer, wheel to pan.
- **Offline:** reads one local JSON file; no network access, no Jira or other
  service connections.
- **Colour themes:** Catppuccin Mocha (the palette clik uses, with its own
  background and card-filled boxes) or the classic 16 terminal colours.

```
   ╔═ M1 Beta launch ════════════════════════════════════════════════[-]╗
   ║                               ┌───────────────────────────────────────────┐
   ║  ┏━━━━━━━━━━━━━━━━━━━━━━━━━━┓ │      ┏━━━━━━━━━━━━━━━━━━━━━━━━━━┓  ║      │ ╔══════════════════════════╗
   ║  ┃E1 · epic                 ┃ │      ┃E2 · epic                 ┃  ║      │ ║M2 · milestone            ║
   ║  ┃Auth & accounts           ├─┘      ┃Task board                ┃  ║      └▶┤GA release                ║
   ║  ┃                          ├╌┐ ┌───▶┤                          ├─────┐   ┌▶┤General availability:     ║
   ║  ┃                          ├───┘    ┃                          ├╌╌╌╌┐└────▶┤billing and production    ║
   ║  ┃                          ┃ ╎      ┃                          ┃  ║ └╌╌╌╌╌▶┤observability.            ║
   ║  ┃◐ in progress         @lee┃ ╎      ┃○ todo                @sam┃  ║      ╎ ║○ todo               @dana║
   ║  ┗━━━━━━━━━━━━━━━━━━━━━━[+3]┛ ╎      ┗━━━━━━━━━━━━━━━━━━━━━━[+3]┛  ║      ╎ ╚══════════════════════[+2]╝
   ║                               └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
   ╚════════════════════════════════════════════════════════════════════╝
 Top  │ zoom:detail (auto) │ ■ blocks ■ duplicates ■ relates_to  │ ?: help
```

Border style shows the kind (═ milestone, ━ epic, ╭╮ story, ┌┐ subtask); border
colour shows status (grey todo, yellow in progress, green done, red blocked).
`[+N]` marks a closed box with N children, `[-]` an open one.

For the full user guide (reading the screen, every key, tracing, organising your
data, exporting from a database, troubleshooting) see
**[docs/guide.md](docs/guide.md)**.

## Quick start

The helper scripts in `scripts/` check prerequisites, build or install a viewer,
run it and run the tests:

```sh
scripts/viewer.sh install                     # Linux / macOS: both viewers
scripts/viewer.sh run rust                    # opens the sample plan
scripts/viewer.sh run python --file my-plan.json
scripts/viewer.sh test                        # both test suites
```

```powershell
scripts\viewer.ps1 install                    # Windows, PowerShell 7
scripts\viewer.ps1 run rust
scripts\viewer.ps1 run python --file my-plan.json
scripts\viewer.ps1 test
```

| Command | What it does |
|---|---|
| `install [rust\|python\|all]` | Checks for `cargo` / Python 3.10+ and says how to install them if missing, builds the Rust viewer (release) and creates `python/.venv`. |
| `run rust\|python [viewer options]` | Runs a viewer, installing it first if needed (and rebuilding the Rust viewer if its source changed). Without `--file` it opens the sample plan. Options go straight to the viewer; relative paths resolve from your current directory. |
| `test [rust\|python\|all]` | `cargo test`, and `pytest` including the Rust/Python parity tests. |

Prerequisites: a Rust toolchain for the Rust viewer, Python 3.10+ for the Python
viewer. `CLIO_PYTHON` picks a specific interpreter.

**Windows notes:**
- Use PowerShell 7 (`pwsh`) and Windows Terminal. The viewers detect Windows
  Terminal and use full colour (mocha theme), and mouse support and box-drawing
  characters work there.
- The script suggests `winget` commands for Rust (which also needs the Visual
  Studio C++ Build Tools) and Python.
- If the execution policy blocks the script, run it as
  `pwsh -ExecutionPolicy Bypass -File scripts\viewer.ps1 …` or use
  `Unblock-File scripts\viewer.ps1`.
- `viewer.ps1` has been reviewed but not yet run on Windows; please report problems.

To build by hand instead, see [Rust implementation](#rust-implementation) and
[Python implementation](#python-implementation) below.

## Colour themes

| Theme | Looks like |
|---|---|
| `mocha` | The Catppuccin Mocha palette that clik uses. The viewer paints its own background (mantle `#181825`), fills closed boxes like cards (surface0 `#313244`) and gives the status bar a panel colour. The Python version also switches Textual's own widgets to its `catppuccin-mocha` theme. |
| `classic` | The 16 standard terminal colours on the terminal's own background. |

The theme comes from `--theme NAME`, else `CLIO_THEME`, else **mocha** when the
terminal has true colour (`COLORTERM=truecolor`/`24bit`, or Windows Terminal) and
**classic** otherwise. Press `C` to switch while running. All colours live in one
table per language (`rust/src/theme.rs`, `python/clio_viewer/theme.py`).

## Dependency tracing

Press `t` to trace the selected box. Press it again to cycle through
**both → upstream → downstream → off**.

- **Upstream** (teal; light cyan in classic) is everything the selection depends
  on, found by following links backwards.
- **Downstream** (pink; light magenta in classic) is everything that depends on it.
- A box that is both upstream and downstream (a cycle) is peach (light yellow in
  classic).
- The wires on the path keep their colour. Every other box and wire turns grey.
- The trace follows the selection as you move it, so you can sweep across the
  schematic and watch each item's cone.

Rules:

- **Only enabled link types are followed.** Use `r` to narrow the trace.
  `relates_to` has no real direction, so leave only `blocks` on to see true
  blocking chains.
- **Containers trace their contents.** Tracing an epic follows the links of all
  its stories and subtasks.
- **Reaching an item includes its children**, because a link to an epic applies
  to the work inside it.
- **Collapsed boxes light up for hidden items.** If a traced item is hidden
  inside a collapsed box, that box lights up instead, just as wires are lifted.

The status bar shows `trace:both ↑2 ↓5 (3 hidden, T shows)`, counting each
traced item once (an epic and its stories count as one). `T` expands boxes, and
drills out if needed, so every traced item gets its own box.

## Keys

| Key | Action |
|---|---|
| arrows / `hjkl` | move selection (→/↓ on an open box steps into it) |
| Shift+arrows / `HJKL` | pan |
| `+` / `-` | zoom in / out (turns auto-fit off and returns to base box sizes) |
| `z` | auto-fit zoom on / off |
| space | expand / collapse in place |
| `x` | close the open box, or the box around the selection (repeat to close outward); at the top of a drilled-in view, drill out |
| enter | drill into box |
| esc / backspace | drill out |
| `e` / `E` | expand all / collapse all (within the current view) |
| `p` | select parent |
| `c` | center on selection |
| `r` | relationship filters (space toggle, `a` all, `n` none) |
| `d` | details panel |
| `t` | dependency trace: both / upstream / downstream / off |
| `T` | expand so every traced item is visible |
| `/`, `n` | search id/title, next match |
| `R` | reload from the data source (keeps open boxes, selection and filters) |
| `C` | colour theme: mocha / classic |
| `?` | help |
| `q` | quit |

| Mouse | Action |
|---|---|
| click | select the innermost box under the pointer |
| click `[+N]` | open a closed box |
| click an open box's title bar or `[-]` | close it |
| double right-click | close the innermost open box under the pointer (on a closed box: the box around it; on empty canvas: like `x`) |
| wheel / Shift+wheel | pan up-down / sideways |

The viewer captures the mouse while it runs, so selecting text in the terminal
usually needs Shift held down.

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

`kind` ∈ milestone | epic | story | subtask; `status` ∈ todo | in_progress | done |
blocked (default todo). Link `type` is free-form snake_case; point links from the
prerequisite to the dependent (`A blocks B`). Loaders reject duplicate ids, unknown
parents or link endpoints, parent cycles and unknown fields. The formal schema is
`schema/clio-tasks.schema.json`; [docs/guide.md](docs/guide.md#7-organising-your-data)
has the full field reference, validation rules, data-organisation guidelines and a
SQLite schema + export script.

## Layout of this folder

| Path | What |
|---|---|
| `docs/guide.md` | **User guide**: using the viewer, organising your data, exporting from a database, troubleshooting |
| `scripts/` | `viewer.sh` (Linux/macOS) and `viewer.ps1` (Windows, PowerShell 7): install, run and test both viewers |
| `schema/clio-tasks.schema.json` | Canonical data format shared by both implementations |
| `fixtures/sample.json` | Sample hierarchy (2 milestones, 4 epics, 12 stories, 8 subtasks, 3 link types) |
| `rust/` | Rust + [ratatui](https://ratatui.rs) implementation |
| `python/` | Python + [Textual](https://textual.textualize.io) implementation |

## Command-line options

Both viewers take the same options:

| Option | Meaning |
|---|---|
| `--file PATH` | the Clio JSON file to show (required) |
| `--theme NAME` | `mocha` or `classic` |
| `--snapshot WxH` | print one frame of width W and height H as text instead of running interactively |
| `--keys SCRIPT` | with `--snapshot`: replay keys first (`<space>`, `<enter>`, `<esc>`, `<S-left>`, …) |
| `--styles` | with `--snapshot`: also print each canvas row's cell colours after a `--- styles ---` line, as run-length `N*fg/bg/bold` tokens (the parity test compares them) |

```sh
scripts/viewer.sh run rust --snapshot 160x40 --keys '<space>t' --theme classic
```

## Rust implementation

```sh
cd viewer/rust
cargo run --release -- --file ../fixtures/sample.json
```

### Security

The viewer is fully offline: neither implementation has any networking code, and
the Rust build pulls in no HTTP or TLS crates. Every text field read from the
file has its control characters replaced with spaces (descriptions keep
newlines), so task data can't send escape sequences such as OSC 52 clipboard
writes to your terminal. `TEXTUAL_*` environment variables can change how the
Python viewer behaves.

### Tests

```sh
cargo test
```

The tests cover:
- model validation;
- layout geometry: no sibling overlap, children inside parents, stretch metrics;
- edge lifting and routing: routes avoid boxes and stay inside their common
  container;
- filters and drill-in scope;
- dependency tracing;
- auto-fit and fill-the-terminal;
- the `x` key and mouse input, including double right-click timing;
- themes and backgrounds;
- UI behaviour through ratatui's `TestBackend`.

## Python implementation

Same features, same keys and **the same frames**. Layout and rendering are a direct
port of the Rust code, and `tests/test_parity.py` checks that both viewers produce
byte-identical frames (characters and cell colours) for 25 key scripts × 4 terminal
sizes × both themes. Popups are native Textual screens: the help overlay, the
relationship filters (a `SelectionList`) and the search prompt (an `Input`). The
details panel is a docked widget.

```sh
cd viewer/python
python3 -m venv .venv && .venv/bin/pip install -e '.[test]'
.venv/bin/clio-viewer --file ../fixtures/sample.json
```

Python 3.10 or newer.

`--snapshot`, `--keys` and `--styles` work as in Rust. Only normal-mode keys are
supported, because popups are interactive.

### Tests

```sh
.venv/bin/python -m pytest
```

The suite covers the same areas as the Rust tests, plus:
- Textual UI behaviour through `Pilot`: keys, clicks and right-clicks, popups,
  theme switching;
- the Rust↔Python parity check.

The parity check runs only when the Rust debug binary has been built
(`cargo build` in `viewer/rust`, which `scripts/viewer.sh test` does for you) or
`CLIO_RUST_BIN` points at one.
