# Clio viewer guide

This guide explains how to use the Clio viewer and how to organise your task
data so the viewer can show it well. It covers both implementations: Rust
(`viewer/rust`) and Python (`viewer/python`). They have the same features, the
same keys, and draw identical screens.

- [1. What the viewer is for](#1-what-the-viewer-is-for)
- [2. Getting started](#2-getting-started)
- [3. Reading the screen](#3-reading-the-screen)
- [4. Moving around](#4-moving-around)
- [5. Tracing dependencies](#5-tracing-dependencies)
- [6. Everyday recipes](#6-everyday-recipes)
- [7. Organising your data](#7-organising-your-data)
- [8. Keeping your tasks in a database](#8-keeping-your-tasks-in-a-database)
- [9. Loading from Jira](#9-loading-from-jira)
- [10. Troubleshooting](#10-troubleshooting)

---

## 1. What the viewer is for

The viewer shows a project plan the way an RTL/schematic viewer shows a chip
design:

| Schematic viewer | Clio viewer |
|---|---|
| module / instance | a task item: **milestone → epic → story → subtask** |
| open an instance to see what is inside | expand a box to see its children |
| nets / wires | relationships between items: `blocks`, `relates_to`, … |
| fan-in / fan-out cone trace | dependency trace: what an item waits on, and what waits on it |

The viewer is read-only. It never changes your data. You edit tasks wherever
they live (a JSON file, a database you export from, or Jira) and press `R` in
the viewer to reload.

---

## 2. Getting started

### Rust

```sh
cd viewer/rust
cargo run --release -- --file ../fixtures/sample.json
```

### Python (3.10 or newer)

```sh
cd viewer/python
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/clio-viewer --file ../fixtures/sample.json
```

`viewer/fixtures/sample.json` is a small example project with 2 milestones and
26 items in total. Open it and work through sections 3–5 with it.

### Command-line options (both versions)

| Option | Meaning |
|---|---|
| `--file PATH` | load a Clio JSON file (see [section 7](#7-organising-your-data)) |
| `--jira --jql QUERY` | load from Jira Cloud instead (see [section 9](#9-loading-from-jira)) |
| `--snapshot WxH` | print one screen of width W and height H as text, then exit (no interactive UI) |
| `--keys SCRIPT` | with `--snapshot`: press these keys first, e.g. `'e-'` or `'<space>j<enter>'` |
| `--theme NAME` | colour theme: `mocha` or `classic` (see [Colour themes](#colour-themes)) |
| `--styles` | with `--snapshot`: also print the colour of every cell (used by tests) |

`--snapshot` is useful for pasting a diagram into a chat, a ticket or a review.
It is also a quick way to check that a file is valid: if the file is broken,
the viewer prints an error and exits with status 1.

---

## 3. Reading the screen

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
 Top  │ zoom:detail │ ■ blocks ■ duplicates ■ relates_to  │ ?: help
```

### Boxes

The **border style** shows the item's kind:

| Border | Kind |
|---|---|
| `╔═╗` double | milestone |
| `┏━┓` heavy | epic |
| `╭─╮` rounded | story |
| `┌─┐` plain | subtask |

The **border colour** shows its status (the same colour names in both themes,
pastel shades in mocha):

| Colour | Glyph | Status |
|---|---|---|
| grey | `○` | todo |
| yellow | `◐` | in progress |
| green | `●` | done |
| red | `✖` | blocked |

The **selected box** is drawn dark on blue (classic: white on blue).

### Colour themes

The viewer has two colour themes:

| Theme | Looks like | Needs |
|---|---|---|
| `mocha` | The [Catppuccin Mocha](https://catppuccin.com/palette) pastel palette, the same one clik uses | A true-colour terminal (most modern terminals) |
| `classic` | The 16 standard terminal colours | Any terminal |

The theme is chosen in this order:

1. `--theme NAME`
2. the `CLIO_THEME` environment variable
3. otherwise **mocha** if `COLORTERM` is `truecolor` or `24bit`, and classic
   if not.

Press **`C`** to switch themes while the viewer runs.

Every colour comes from one table per language (`rust/src/theme.rs`,
`python/clio_viewer/theme.py`). Adding a theme means adding one more entry in
each.

A **closed box** shows a `[+N]` tag in its bottom-right corner when it has `N`
children. At the overview zoom level the tag is just `+`.

An **open box** (container) shows its id and title in the top border, with `[-]`
at the right end. Its children are drawn inside it.

What a closed box shows depends on the zoom level:

| Zoom | Base box size | Shows |
|---|---|---|
| detail | 26×5 | `id · kind`, title, status, `@assignee` |
| compact | 20×3 | `id title` |
| overview | 8×3 | `id` |

With auto-fit on, boxes grow beyond their base size to fill the terminal
(see [Auto-fit zoom](#auto-fit-zoom)):

- **Wider** boxes, up to twice the base width, show more of the title.
- **Taller** boxes wrap the title over several lines. At detail zoom they also
  show the start of the description (in grey) below it.

### Wires

- A wire leaves the **right side** of the item at its `from` end (marked `├`).
- It enters the **left side** of the item at its `to` end (marked `┤`), with an
  arrowhead `▶`.
- Each relationship type has its own colour and line style. The status bar
  shows which colour belongs to which type.
- **Wires stay inside their box.** A wire between two items never leaves the
  innermost open box that contains both of them. For example, a link between
  two stories in epic E1 stays inside E1, and a link from a story in E1 to one
  in E2 stays inside their milestone. Only links whose ends have no common box
  (different milestones) run across the top level.
- Wires are routed around closed boxes. They may cross an open container they
  are not connected to, when that is the only way through.
- **Lifting:** when an end of a link is hidden inside a closed box, the wire is
  drawn to that box instead. Opening the box shows exactly which child the link
  belongs to.
- **Merging:** when several links of one type collapse onto the same pair of
  boxes, they become one wire labelled `×N` at its start.
- **Left-to-right order:** boxes inside a container are arranged in columns so
  that links mostly point left → right. Prerequisites are on the left and the
  work that depends on them is on the right. If the links form a cycle, the
  cycle is broken for layout purposes only.
- **No wires between nested items:** a link between an item and its own parent
  or child is not drawn, because one box already sits inside the other.

### Status bar

```
 Top › M1 │ zoom:detail (auto) │ ■ blocks □ duplicates ■ relates_to  (3 links leave this view) │ trace:both ↑2 ↓5 │ ?: help
```

From left to right:

1. **Breadcrumb**, shown when you have drilled into a box.
2. **Zoom level.** `(auto)` means auto-fit is choosing it (see below).
3. **Relationship types.** `■` means shown and `□` means hidden.
4. **Links leaving the view**, when you have drilled in and some links connect
   to items outside the current view.
5. **Trace summary**, when tracing is on.
6. **Messages** such as search results.

---

## 4. Moving around

Press `?` at any time to show the key list.

| Key | Action |
|---|---|
| arrows / `h j k l` | Move the selection to the nearest box in that direction. →/↓ on an open box steps into its first child. |
| Shift+arrows / `H J K L` | Pan the view without changing the selection. |
| `+` / `-` | Zoom in / out (detail ↔ compact ↔ overview). Turns auto-fit off. |
| `z` | Turn auto-fit zoom back on (or off). |
| space | Open / close the selected box in place. |
| `x` | Close the selected box if it is open, otherwise the box around it. Press again to keep closing outward. |
| enter | Drill into the selected box: it becomes the whole view. |
| esc / backspace | Drill back out one level. |
| `e` / `E` | Open all / close all boxes in the current view. |
| `p` | Select the parent of the selected box. |
| `c` | Centre the view on the selection. |
| `r` | Relationship filters. |
| `d` | Details panel. |
| `/` then `n` | Search, then jump to the next match. |
| `t` / `T` | Dependency trace / show the whole trace (see section 5). |
| `R` | Reload the data source. |
| `C` | Switch colour theme (mocha / classic). |
| `q` | Quit. |

### Mouse

| Action | Effect |
|---|---|
| Click a box | Select it. Inside open boxes, the innermost box under the pointer is selected. |
| Click `[+N]` on a closed box | Open it. |
| Click the top border of an open box (its title or `[-]`) | Close it. |
| Wheel | Pan up / down. |
| Shift+wheel (or a sideways wheel) | Pan left / right. |

While the viewer runs it captures the mouse, so selecting text in the terminal
usually needs Shift held down (this varies by terminal).

### Auto-fit zoom

Auto-fit is on when the viewer starts. Whenever you open or close boxes, drill
in or out, change filters, or resize the terminal, the viewer picks the **most
detailed zoom level at which the whole view fits on screen**:

- If the view fits at *detail*, it is shown at detail.
- Otherwise it tries *compact*, then *overview*.
- If even overview is too big, the view stays at overview. It starts from the
  top-left, scrolls only as far as needed to keep the selection visible, and
  you pan the rest.

Auto-fit then **stretches the layout to use the whole terminal**, separately
for each direction in which it fits:

- **Across:** boxes widen first (up to twice their width), then the gaps
  between columns grow.
- **Down:** boxes gain text rows first, then the gaps between stacked boxes
  grow.
- Space that can't be used is split evenly on both sides, so the drawing sits
  in the middle of the window.

A bigger terminal therefore shows bigger boxes with more text. Resizing the
window refits immediately.

The status bar shows `zoom:compact (auto)` while auto-fit is in charge.
Pressing `+` or `-` means you want a particular size, so it turns auto-fit off
and goes back to that zoom level's base box sizes until you press `z`.

### Closing boxes

Pick whichever is closest to hand:

- **`x`** works from anywhere inside a box. On an open box it closes that box.
  On anything else it closes the box around the selection and selects it, so
  pressing `x` repeatedly closes one level at a time. At the top of a
  drilled-in view, `x` drills back out.
- **Click** the top border of an open box.
- **Space** on the open box itself (`p` selects the parent first).
- **`E`** closes everything in the current view.

### Opening a box in place vs drilling in

- **Space** opens a box where it is. You keep the surrounding context, and wires
  to the rest of the plan stay visible. Use it to look one level deeper.
- **Enter** makes the box the whole view and shows a breadcrumb such as
  `Top › M1 › E2`. Use it when a container is too large to read in place. Links
  to items outside the view are counted in the status bar rather than drawn.
  Esc goes back.

### Relationship filters (`r`)

`r` opens a checklist of every relationship type found in your data, with a
count for each.

- Move with the arrow keys (`j`/`k` also work in the Rust version).
- Space toggles the type under the cursor.
- `a` turns all types on; `n` turns all of them off.
- Esc closes the list.

Changes apply immediately. The filters also control what the dependency trace
follows.

### Search (`/`)

Type part of an item's id or title and press enter.

- Matching is case-insensitive and searches all items, including ones hidden
  inside closed boxes.
- The viewer opens or drills out as far as needed, selects the match and
  centres it.
- `n` jumps to the next match.

### Details (`d`)

`d` opens a side panel for the selected item. It shows:

- id, kind, title, status and assignee;
- the path from the top of the hierarchy, and the number of children;
- the description;
- every link in and out of the item.

### Reload (`R`)

`R` re-reads the file (or re-runs the Jira query). The viewer keeps your open
boxes, drill path, selection and filters, matching items by id. Any new
relationship types are switched on. If the new data is invalid, the viewer
keeps showing the old data and puts the error in the status bar.

---

## 5. Tracing dependencies

Press **`t`** on any box.

- **Upstream** items, which the selection depends on, turn **teal** (classic:
  light cyan).
- **Downstream** items, which depend on the selection, turn **pink** (classic:
  light magenta).
- Items that are both (a dependency cycle) turn **peach** (classic: light
  yellow).
- Every other box and wire turns grey. The wires on the traced paths keep their
  colour.

Press `t` again to cycle through **both → upstream only → downstream only →
off**. The trace follows the selection, so you can move around with the arrow
keys and watch each item's dependencies change.

The status bar shows the size of the trace, for example
`trace:both ↑2 ↓5 (3 hidden, T shows)`. Press **`T`** to open boxes, and drill
out if needed, until every traced item has its own visible box.

### Rules

The trace follows these rules:

1. **It follows only the enabled relationship types.** Use `r` to choose them.
   `blocks` is a true dependency. `relates_to` usually is not, and following it
   can pull in half the plan. For a clean "what is actually blocking this"
   view, turn on only `blocks`.
2. **A container includes its contents.** Tracing an epic follows the links of
   all its stories and subtasks.
3. **Reaching an item includes its children.** If S1 blocks epic E2, then
   everything inside E2 is downstream of S1, and the links from E2's children
   are followed too.
4. **Hidden items light up their box.** A traced item inside a closed box
   lights up that box, in the same way wires are lifted.
5. **Counts are per outermost item.** An epic and all of its stories in the
   trace count as one.

### Example (sample data)

1. Search for `S5` (Board UI): press `/S5` then enter.
2. Press `t`.

Upstream are **S4** (Board API) and **S1** (Login page), which both block S5.
Downstream are **S6**, **S7 → S8 → S9** and, through `relates_to`, **S12**.
The status bar shows `↑2 ↓5`.

---

## 6. Everyday recipes

| Question | How |
|---|---|
| What is blocking milestone M2 from starting? | Select M2, press `t` twice (`trace:upstream`), then `T`. |
| What slips if S4 slips? | Select S4, press `t` three times (`trace:downstream`), then `T`. |
| What is blocking this, ignoring "related" noise? | `r`, turn everything off except `blocks`, close the list, then `t`. |
| Show me one epic in depth. | Select it, press enter, then `e` to open everything inside it. |
| Big picture of the whole plan. | `e` (open all). Auto-fit shrinks the boxes to fit; pan with `HJKL` or the wheel if it is still too big. |
| Paste a diagram into a ticket. | `clio-viewer --file plan.json --snapshot 140x40 --keys 'e-'` |

---

## 7. Organising your data

The viewer reads one JSON document. The format is described formally by the
JSON Schema in `viewer/schema/clio-tasks.schema.json`, and summarised here.

### The smallest valid file

```json
{
  "version": 1,
  "items": [
    { "id": "M1", "kind": "milestone", "title": "Beta launch" },
    { "id": "E1", "kind": "epic", "title": "Accounts", "parent": "M1" },
    { "id": "S1", "kind": "story", "title": "Login page", "parent": "E1", "status": "done" },
    { "id": "S2", "kind": "story", "title": "Password reset", "parent": "E1" }
  ],
  "links": [
    { "from": "S1", "to": "S2", "type": "blocks" }
  ]
}
```

### Document

| Field | Required | Meaning |
|---|---|---|
| `version` | yes | Always `1`. |
| `items` | yes | Every task, as a **flat list**. Nesting is expressed with `parent`, not by putting items inside each other. |
| `links` | no | Relationships between items. Defaults to none. |

### Items

| Field | Required | Values | Meaning |
|---|---|---|---|
| `id` | yes | non-empty string, unique | The item's key. Links and parents refer to it, and reload matches items by it. |
| `kind` | yes | `milestone`, `epic`, `story`, `subtask` | Sets the border style. |
| `title` | yes | string | Short, single-line name. |
| `status` | no | `todo` (default), `in_progress`, `done`, `blocked` | Sets the border colour. |
| `parent` | no | id of another item, or `null` | The containing item. Leave it out for top-level items. |
| `assignee` | no | string or `null` | Shown as `@name` at detail zoom. |
| `description` | no | string or `null` | Shown in the details panel. Line breaks are kept. |

### Links

| Field | Required | Meaning |
|---|---|---|
| `from` | yes | id of the item where the arrow starts |
| `to` | yes | id of the item the arrow points to |
| `type` | yes | Any non-empty string. Use `snake_case`, e.g. `blocks`, `relates_to`, `duplicates`. |

Relationship types are not a fixed list. Every distinct `type` in the file
appears in the `r` filter list and gets its own colour. The six colours
repeat if a file has more than six types.

### Validation: what makes a file fail to load

The viewer checks the whole file before showing anything, and refuses to load
it if any of these are true:

- `version` is not `1`.
- An item is missing `id`, `kind` or `title`, or has an empty `id`.
- Two items have the same `id`.
- `kind` or `status` is not one of the listed values. They are case-sensitive:
  `in_progress`, not `In Progress`.
- A `parent` or a link's `from`/`to` names an id that is not in the file.
- Parents form a loop (A's parent is B and B's parent is A).
- A link has an empty `type`.
- **Any field not listed above is present**, on the document, an item or a
  link. This is deliberate, so typos such as `"parnet"` don't fail silently.
  Put extra information such as priority or estimates into `description`.

### Guidelines for data that reads well

**Hierarchy**

- Use the kinds top-down: milestone → epic → story → subtask. The viewer does
  not enforce this (a story directly under a milestone loads fine), but box
  styles are only meaningful if kinds are used consistently.
- Keep items with no parent for the top level: usually milestones. Items with
  no parent are drawn at the top level of the view.
- Keep ids **stable and short**. Ids appear on every box (at overview zoom,
  ids longer than 6 characters are cut off), and reload uses them to keep your view
  state. Project-style keys (`CLIO-42`) or prefixes (`M1`, `E3`, `S12`) work
  well.
- **Order matters a little.** Top-level items and siblings keep the order they
  have in the file within each layout column. Put items in the order you want
  to read them.
- Keep titles short. About 24 characters fit in a box at detail zoom at its
  base size (more on a big terminal, where boxes grow and titles wrap), and
  longer titles are cut off with `…`. Put the rest in `description`. The
  first words of the description appear in boxes that have room.

**Links**

- **Point links from the prerequisite to the dependent.** `{"from": "A", "to":
  "B", "type": "blocks"}` means *A blocks B*, so A must finish first. The
  layout then flows left → right in time order, and "upstream" in the trace
  means "has to happen before". If your source says *B depends on A*, swap the
  ends when exporting, and call the type `blocks`.
- **Link the most specific items you know about.** If you know subtask T5
  blocks story S5, link T5 → S5 rather than epic E2 → S5. When E2 is closed,
  the wire is lifted to E2 anyway, and opening E2 shows the exact source.
- **Don't link an item to its own parent or child.** That relationship is
  already shown by nesting, so the link is not drawn.
- **Keep the number of types small.** Two or three types (`blocks`,
  `relates_to`, `duplicates`) are easy to read. A dozen is not.
- Undirected relationships such as `relates_to` still need a `from` and a `to`.
  Pick either direction, and add each pair **only once**: repeated links are
  merged into a `×2` wire.
- Cycles are allowed. They are highlighted when traced, but they usually
  mean a planning problem.

**Status**

- Use `blocked` only for items that are actually stuck. The trace already
  shows what is waiting on what, so you don't need to mark every downstream
  item as blocked.

### Editing the file by hand

The JSON file is fine for hand editing in small projects:

1. Keep the viewer open.
2. Edit and save the file.
3. Press `R` in the viewer.

If you made a mistake, the viewer keeps the previous version on screen and
shows the error in the status bar, so you can fix it and press `R` again.

Any JSON Schema (draft 2020-12) validator can check a file against
`viewer/schema/clio-tasks.schema.json`, e.g. in an editor or in CI. The
viewer's own checks (above) also catch unknown parents, unknown link ends and
parent loops, which a schema cannot express.

---

## 8. Keeping your tasks in a database

The viewer has no database driver. It reads the JSON format above. For anything
bigger than a hand-edited file, keep the tasks in a database and **export to
JSON**. Then press `R` in the viewer whenever you want to see the latest
export.

The tables below map one-to-one onto the JSON format. This example uses SQLite,
but the same shape works in Postgres or MySQL.

### Schema

```sql
CREATE TABLE items (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind IN ('milestone', 'epic', 'story', 'subtask')),
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'todo'
                CHECK (status IN ('todo', 'in_progress', 'done', 'blocked')),
    parent      TEXT REFERENCES items (id),
    assignee    TEXT,
    description TEXT,
    position    INTEGER NOT NULL DEFAULT 0   -- order among siblings
);

CREATE TABLE links (
    from_id TEXT NOT NULL REFERENCES items (id),
    to_id   TEXT NOT NULL REFERENCES items (id),
    type    TEXT NOT NULL CHECK (type <> ''),
    PRIMARY KEY (from_id, to_id, type),
    CHECK (from_id <> to_id)
);
```

The constraints stop most invalid data from being stored:

- `CHECK` limits `kind` and `status` to the values the viewer accepts.
- `REFERENCES` stops dangling parents and links. In SQLite this only works with
  `PRAGMA foreign_keys = ON`.
- The primary key on `links` stops the same link being stored twice.

One rule is not covered: the database won't stop parent loops. The viewer
reports those when it loads the export.

Create the database with the `sqlite3` command-line tool, or with Python's
built-in `sqlite3` module:

```sh
sqlite3 tasks.db < schema.sql
```

### Adding data

```sql
PRAGMA foreign_keys = ON;

INSERT INTO items (id, kind, title, status, parent, assignee, position) VALUES
  ('M1', 'milestone', 'Beta launch',    'in_progress', NULL, 'dana', 0),
  ('E1', 'epic',      'Accounts',       'in_progress', 'M1', 'lee',  0),
  ('S1', 'story',     'Login page',     'done',        'E1', 'lee',  0),
  ('S2', 'story',     'Password reset', 'todo',        'E1', 'ana',  1);

INSERT INTO links (from_id, to_id, type) VALUES
  ('S1', 'S2', 'blocks');             -- S1 must finish before S2
```

Insert parents before their children, so the foreign keys are satisfied.

### Exporting

Save this as `export_clio.py`. It needs only Python's standard library:

```python
#!/usr/bin/env python3
"""Export a Clio task database (SQLite) to the viewer's JSON format.

usage: export_clio.py tasks.db tasks.json
"""
import json
import os
import sqlite3
import sys

db_path, out_path = sys.argv[1], sys.argv[2]
db = sqlite3.connect(db_path)
db.row_factory = sqlite3.Row

items = []
for r in db.execute(
    "SELECT id, kind, title, status, parent, assignee, description "
    "FROM items ORDER BY position, rowid"
):
    # Leave out empty optional fields; the viewer treats missing as "none".
    items.append({k: r[k] for k in r.keys() if r[k] is not None})

links = [
    {"from": r["from_id"], "to": r["to_id"], "type": r["type"]}
    for r in db.execute("SELECT from_id, to_id, type FROM links ORDER BY rowid")
]

# Write to a temp file and rename, so a viewer reloading with R never reads
# a half-written file.
tmp = out_path + ".tmp"
with open(tmp, "w") as f:
    json.dump({"version": 1, "items": items, "links": links}, f, indent=2)
os.replace(tmp, out_path)
print(f"wrote {len(items)} items and {len(links)} links to {out_path}")
```

Then export and open the result:

```sh
python3 export_clio.py tasks.db tasks.json
clio-viewer --file tasks.json        # later: re-export, then press R in the viewer
```

The export writes **only** the columns the format allows. The viewer rejects
unknown fields, so add new columns to the database as you like but don't put
them in the export. Fold anything you want visible into `description`.

### Adapting an existing database

If your tasks already live in another schema, you don't need to copy them into
the tables above. Write the two `SELECT`s so they return the expected columns:

- **Kind.** Map your issue types onto the four kinds with a `CASE` expression,
  e.g. `CASE type WHEN 'Initiative' THEN 'milestone' WHEN 'Task' THEN 'story'
  ... END`.
- **Status.** Map your workflow states onto the four statuses the same way.
  Anything not finished and not stuck is `todo` or `in_progress`.
- **Parent.** The parent column must be an id that is also exported. If you
  export a subset, e.g. one project, set the parent to `NULL` when the parent
  is outside the subset, and drop links whose other end is outside it.
  Otherwise the file won't load.
- **Link direction.** Flip "depends on" style links so they run from the
  prerequisite to the dependent (see [Links](#links)).

---

## 9. Loading from Jira

Both versions can read directly from Jira Cloud:

- **Python:** works out of the box.
- **Rust:** must be built with the `jira` feature:
  `cargo run --release --features jira -- --jira --jql '…'`.

```sh
export JIRA_URL=https://your-company.atlassian.net
export JIRA_EMAIL=you@your-company.com
export JIRA_API_TOKEN=…            # https://id.atlassian.com/manage-profile/security/api-tokens
clio-viewer --jira --jql 'project = CLIO ORDER BY rank'
```

The JQL decides which issues are loaded. Issues are mapped like this:

| Clio field | Taken from Jira |
|---|---|
| `id` | issue key (`CLIO-42`) |
| `kind` | Issue type name: `Milestone`/`Initiative` → milestone, `Epic` → epic, names containing "sub" → subtask. Otherwise it uses the hierarchy level: 2 or more → milestone, 1 → epic, below 0 → subtask, anything else → story. |
| `status` | `blocked` if the status name contains "block". Otherwise the status category: Done → `done`, In Progress → `in_progress`, anything else → `todo`. |
| `parent` | the issue's parent, **if the parent is also in the results**. Otherwise the issue becomes top-level. |
| `links` | Issue links, in their outward direction (`CLIO-1 blocks CLIO-2`). The type name is converted to snake_case (`relates to` → `relates_to`). Links to issues outside the results are dropped. |
| `assignee`, `description` | display name, and the description as plain text |

Make the JQL include the parents you want to see. For example,
`project = CLIO` loads milestones, epics and their children together. A query
for only stories shows them all as top-level boxes.

This has been tested against recorded Jira responses only, not a live Jira
site.

---

## 10. Troubleshooting

| Message | Cause and fix |
|---|---|
| `item "S3" has unknown parent "E9"` | `parent` names an id that isn't in the file. Fix the id, add the parent, or remove `parent`. |
| `blocks link "A" -> "B": unknown target` (or `source`) | A link end isn't in the file, often because it was filtered out of an export. Drop the link or export the item. |
| `duplicate item id "S1"` | Two items share an id. Ids must be unique across the whole file, not just among siblings. |
| `parent cycle involving item "A"` | Following `parent` from A comes back to A. One of the parents is wrong. |
| Rust ``unknown field `priority` `` / Python `items[0] has unknown field(s) ['priority']` | Fields outside the format are rejected. Remove the field or move its contents into `description`. |
| Rust ``unknown variant `task` `` / Python `items[0]: 'task' is not a valid Kind` | `kind`/`status` must be one of the listed lowercase values. Map your own names when exporting. |
| `unsupported format version 2` | Set `"version": 1`. |
| `reload failed: …` in the status bar | The file changed and is now invalid. The previous data is still shown. Fix the file and press `R` again. |
| A link I added has no wire | Check that its type is enabled (`r`), that it doesn't connect an item to its own parent or child, and, if you have drilled in, that both ends are inside the current view (the status bar counts links leaving the view). |
| The trace lights up almost everything | A broad type such as `relates_to` is being followed. Turn it off with `r`. |
| Everything is too big to see | Drill into the part you care about with enter, or close boxes with `x` / `E`. If you zoomed by hand, press `z` to let auto-fit pick the size again. |
| Boxes suddenly got smaller or bigger | That is auto-fit reacting to boxes opening or closing. Press `+`/`-` to pick a size yourself (turns auto-fit off), `z` to turn it back on. |

The Rust messages are quoted above. The Python version prints the same checks
with single quotes, e.g. `item 'S3' has unknown parent 'E9'`.
