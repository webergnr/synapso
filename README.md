# Synapso

A notes + canvas workspace, with the look of an IDE and dark/light theming.
Native desktop in Rust, drawn with [Freya](https://github.com/marc2332/freya) on
Skia (the fork `Cargo.toml` pins is
[webergnr/freya](https://github.com/webergnr/freya/tree/synapso); why Freya is
in [ADR-009](docs/adr-009-freya.md)).

## Why Synapso

Your second brain should be fast, yours, and boring in the right ways.

- **Obsidian** is great, but closed source.
- **Notion** lives in someone else's cloud, and does a lot more than notes.
- **Logseq** is open, but everything is an outline block, and bending it to your
  way takes a lot of learning.

Synapso is the basics, done well, with nothing new to learn first:

- **Markdown that reads like a document** while you type it, and stays plain
  `.md` on disk.
- **A canvas that is simple and works**: shapes, arrows, frames, notes as cards.
  It's saved as [JSON Canvas](https://jsoncanvas.org), so it opens in Obsidian too.
- **Folders are groups**: every folder has a colour, and everything inside it
  wears that colour unless it has its own.
- **Search that makes sense**: `⌘K` for commands, `⌘P` for documents, `⌘⇧F` for
  every file whose text matches, and `⌘F` lights up every match in the note.
- **A graph that makes sense**: the note you are on in the middle, what it links
  to in rings around it, and what links back in the inspector.

Your vault is an ordinary folder, with no account, no database and no lock-in.
Delete Synapso and your notes are still there.

Made by a developer, for developers. Apache-2.0.

On Windows and Linux every `⌘` below is `Ctrl`, and the app spells it that way.

## Run

```sh
cargo run --release                     # vault in ~/Synapso, or the one ⌘O last picked
cargo run --release -- ~/notes          # open ~/notes as the vault, like ⌘O would
SYNAPSO_VAULT=/path cargo run --release # or another vault
```

To get a `synapso` command on your `PATH` (`synapso .`, `synapso ~/notes`):

```sh
cargo install --path . --locked
```

A vault to play in, with every feature in it:

```sh
python fixture/gen_vault.py                         # writes fixture/vault (git-ignored)
SYNAPSO_VAULT=fixture/vault cargo run --release
```

| Variable | What it does |
| --- | --- |
| `SYNAPSO_VAULT` | Open this folder as the vault |
| `SYNAPSO_CONFIG` | Where the remembered vault path lives (tests use it to stay off yours) |
| `SYNAPSO_STATS=<file.csv>` | One CSV row per presented frame: interval, update, layout, render |
| `SYNAPSO_DEBUG` | Print the chords and drag events the app sees |

## The workspace

- **The window bar** (macOS): the system's traffic lights over the design's
  own bar, with ‹ › back and forward through the documents opened, the note
  and board you are on (and the unsaved dot), `search ⌘K` and `open ⌘P`, and
  the save state. Drag it to move the window, double-click to zoom. Windows
  and Linux keep the system's title bar.
- **The tab rail**: vertical tabs with rotated labels, each in its colour. A
  tab ends in a fixed slot that shows × on hover and the unsaved dot
  otherwise, so the name never shifts. The rail scrolls with the wheel. In
  notes-only mode the tabs run across the top.
- **The folder tree** floats over the note (`⌘⇧E` or the folder button), slides
  in without reflowing anything, and closes when you open something, press Esc
  or click elsewhere. *pin* docks it beside the rail. It creates and renames in
  place (double-click), refuses a name that is taken, has a right-click menu
  (new note, new folder, rename, colour, trash), and a note dragged from it
  onto the canvas becomes a reference card.
- **Colours**: clicking any colour line opens the colour tooltip beside it. ↑
  (the folder's colour, in outline) hands the colour back to the folder, then
  come the eight group colours and the rainbow. The rainbow swaps the swatches
  for a strip: hue across, lightness down, chroma locked to the palette's
  (OKLCH), saved as hex. Hovering previews everywhere. Keys: 0–8 pick, 9 opens
  the strip, where the arrows move, ⏎ applies and Esc goes back.
- **The breadcrumb** reads `vault / auth / auth-flow.md`, in each folder's
  colour, with the name editable in place, rename and trash. A new note comes
  from the tab rail's `+` or `⌘N`.
- **Views**: notes, split and canvas (`⌘1`–`⌘3`) in the status bar's switch,
  and the graph as a toggle beside it (`⌘G`); toggling it off goes back to the
  view you came from. The split divider drags, and switching views glides.
- **Confirmations without modals**: a delete goes to `.trash/` at once and the
  toast holds undo for 5 s (`⌘Z` keeps working afterwards); emptying the trash
  is hold-to-confirm.
- Dark and light themes (`⌘⇧L`); interface zoom with `⌘+`, `⌘−` and `⌘0`.

## The editor

**The note is one buffer holding the raw markdown**, and what was a block is a
*line kind*, read back out of the text on every edit
(`crates/synapso-core/src/sheet.rs`, [ADR-008](docs/adr-008-single-buffer-note.md)).
The syntax that opens a line — `## `, `- `, `> `, `- [ ] ` — stays in the
buffer, where the file needs it, and **folds away** everywhere but the caret's
line. You type markdown and you see a document, with nothing consumed and
nothing to translate back on save.

It renders everything in the design's markdown guide (the fixture has it as
`inbox/markdown-guide.md`):

- inline: bold, italic, bold italic, strike, `<u>underline</u>`,
  `==highlight==`, inline code as a chip, external links, wiki links (accent,
  dashed underline) and footnote refs `[^1]`;
- headings 1 to 6;
- bullets, numbered lists and tasks, nested on a guide line. Nested bullets are
  ◦, nested numbers are letters, and done tasks are struck through;
- pipe tables as columns, with the `:---:` alignments and the header on an
  inset band;
- fenced code with light highlighting and its language at the top right,
  quotes nested on quieter rules, rules, pictures (a hatched placeholder when
  missing), and footnote definitions.

The core's inline reader is unchanged; the syntax it doesn't know
(`***`, `==`, `<u>`, `[^1]`) is layered on in `src/rich.rs`, which also paints
what a text style can't — the chip, the wash and the dashed underline — behind
the paragraph, from the rects its own layout measured.

Keys follow the app's own rules (`src/behave.rs`, `src/format.rs`):

- ⏎ continues a list or a table; ⏎ in an empty item leaves the list, one level
  at a time; ⇧⏎ leaves a fenced block;
- ⇥ and ⇧⇥ nest a list item or walk a table's cells;
- `⌘⏎` ticks the line's task;
- `⌘B` `⌘I` `⌘E` `⌘⇧X` `⌘⇧K` format, `⌘⌥1`–`3` set headings, `⌘⇧7` `⌘⇧8`
  `⌘⇧9` set lists and quotes; the format bar's menus do the same.

`[[` opens the link picker and clicking a `[[mention]]` follows it. `⌘F` finds
in the note, with highlights, n/N and scroll-to-hit. Right-click sends the
note — or the selected words, as a quote card — to the open canvas. Only the
visible lines are laid out, so a note of thousands of lines scrolls and types
at full frame rate.

## The canvas

Every gesture is the core's `Scene`: select, marquee, move, resize, rotate (⇧
snaps), connect and reconnect, pen, text, frames, edit a caption, delete,
duplicate, pan and zoom. One Skia pass draws it under the view transform;
paragraphs are cached, nodes outside the view are culled, and illegible text is
skipped.

- The tool rail (`V R O Y A L P T F`, with tooltips), a style bar (colour,
  stroke, shape, align, bold, route, dash, arrowheads; with the pen in hand,
  the next stroke's colour and thickness), a context menu, undo
  and redo, a board switcher and a minimap. The switcher lists the open
  canvases, each with a × on hover; a closed one stays in the tree and ⌘P.
- "new note here" in the canvas's right-click menu creates a note right there,
  as a card, named in place. Renaming a note rewrites every board that points
  at it.
- A caption breaks lines with ⇧⏎. `⌘C`/`⌘V` copy and paste nodes; `⌘V` with a
  picture on the clipboard writes it into `assets/` and puts it on the board.
- Pictures dropped from the file explorer land in `<board folder>/assets` at
  the cursor, several in a cascade. While they hover, the pane tints, a pill
  says where they will be saved, and a ghost sits where they will land.
- `⇧1` fits everything, `⇧0` is 100%.

## The graph

The spec is `design/GRAPH_VIEW_SPEC.md`. The focused note sits in the middle,
with rings by hop and dots for notes beyond the depth (1 to 3). Orbit and branch
layouts share their angles, a collision pass keeps nodes apart, and every change
animates. Groups are the top-level folders, with toggles; unlinked notes are
listed. The inspector shows an excerpt, backlinks with context, outgoing links,
and "mentioned, not linked" with a `link` button that rewrites the note. Also: a
vault history bar with play, hover cards, keyboard control (arrows, ⏎, `⌘⏎`,
`⌘[`, 1/2/3, O/B) and find node in `⌘K`.

## The vault

An ordinary folder, no database and no proprietary format:

- `*.md` — the notes, readable markdown.
- `*.canvas` — the diagrams, in [JSON Canvas](https://jsoncanvas.org) 1.0 (the
  same format Obsidian's canvas uses).

A note or a board is written **1.5 s after you stop**, and at once when you
switch documents, change theme or close the window — through a temp file and a
rename, so a crash mid-write truncates nothing. Until the write has happened,
the dot in the document's colour stays lit on the tab: that is the *unsaved*
state.

A key this build does not know — from a newer JSON Canvas or another editor —
is **kept and handed back** on save instead of vanishing.

**Extensions to the format:** geometric shapes do not exist in JSON Canvas, so
a `text` node gets a `"shape"` key (`rect` | `ellipse` | `triangle` | `label`),
a rotated node gets `"rotation"` in degrees and resized text gets `"fontSize"`
in px. Other readers ignore the keys and show a plain, upright text node.
Frames are `group`, document cards are `file`, arrows and lines are `edges`
(`toEnd: "none"` = a line with no head). An arrow dragged by its middle keeps
its `"bend"`. An end dropped along a side keeps where along it in
`"fromAt"`/`"toAt"`. An end dropped in empty space rests on a small
`"shape": "point"` node, because the spec has no edge without a node at each
end. `"fontFamily": "mono"` writes an item in the app's face instead of the
hand.

What a folder of markdown cannot hold goes into a hidden `.synapso.json` in the
vault, so notes don't get dirtied with frontmatter:

- `colors` — a folder's or a document's colour, keyed by its path (`"auth/": 0`,
  `"auth/auth-flow": "#8BC9FA"`): a group colour as its index, a picked one as
  hex, `null` for an explicit neutral. Keys written by the Floem shell (a
  note's bare name) still read.
- `closed_files` — the documents taken off the rail or out of the board
  switcher, keyed like `colors` (`auth/plan`, `auth/plan.canvas`). *Closed*, not
  *open*, so a vault with no sidecar opens everything.
- `recent_files` — the documents most recently opened, same keys; the app
  reopens with the last note and board you were on. The Floem shell's `closed`
  and `recent` (bare names) move onto these the first time a vault opens.

`⌘O` picks another folder; it becomes the vault, and is still the vault next
launch (the path lives in your user config, never in a vault).

### The file belongs to whoever wrote it

- **The buffer is the file**: saving writes what is on screen, byte for byte,
  instead of re-emitting a document from a tree. Syntax the editor has no line
  kind for comes back exactly as typed.
- A note you did not edit is **never written**.
- A file that could not be read opens empty and its save is refused: an empty
  editor never becomes the file ([ADR-001](docs/adr-001-integrity-and-boundaries.md)).
- A note changed on disk since it was read is not overwritten: the write is
  refused and what you typed goes to a `-unsaved` file beside it.
- Deleting **moves to `.trash/`** inside the vault, under a unique name — it is
  not `rm`.

## Shortcuts

| | |
|---|---|
| `⌘N` | new note, named in place |
| `⌘K` | palette: commands and every document by name |
| `⌘P` | document picker, with a folder column and a preview of the row under the cursor; offers "new note: X" when nothing matches |
| `⌘⇧F` | search the vault's text |
| `⌘F` | find in the note |
| `⌘W` | close what you are in: the note (off the rail) or, in the board, its canvas; the file is untouched |
| `⌘O` | open another vault |
| `⌘S` | save now — autosave already did, this is for muscle memory |
| `⌘Z` `⌘Y` | undo / redo in the note or on the canvas, whichever has the keyboard (`⌘⇧Z` redoes on the canvas too); with neither, `⌘Z` takes back the last trash |
| `⌘1` `⌘2` `⌘3` | notes / split / canvas |
| `⌘G` | the graph, on and off |
| `⌘⇧E` | the folder tree |
| `⌘⇧L` | dark / light |
| `⌘+` `⌘-` `⌘0` | the whole interface bigger / smaller / back to 100% |
| `Esc` | closes the palette, the colour tooltip, the floating tree |

## Performance

Measured on Windows with a 160 Hz monitor (6.25 ms per frame), release build,
on a stress vault: a 3,121-line note and a 200-node, 259-edge canvas. CPU time
per frame:

| scenario | p50 | p99 |
|---|---|---|
| canvas wheel-pan | 2.0 ms | 2.8 ms |
| canvas marquee | 2.4 ms | 3.3 ms |
| scroll the big note | 2.4 ms | 3.0 ms |
| type in the big note | 3.9 ms | 4.1 ms |
| notes/split/canvas transition (canvas with 200 nodes repainting) | 4.6 ms | 6.8 ms |
| graph (fixture vault) | 1.8 ms | 4.4 ms |

The one case over budget is the p99 of the view transition: while the split
width animates, the board and the sheet relayout every frame. Caching the board
as a picture during the transition would fix it. `SYNAPSO_STATS` writes these
numbers; `python3 tools/stress-vault.py /tmp/synapso-stress` builds a
deliberately unfair vault to try them on.

## Verifying for real

`cargo test` covers the logic; the interface is only proved by driving the app.
On macOS, `tools/drive.swift` does that with real CGEvents (AppleScript's
`click` is an accessibility action and never reaches the window):

```sh
swiftc -O -o /tmp/drive tools/drive.swift
/tmp/drive winid                       # window id + position
screencapture -x -o -l<id> /tmp/w.png  # captures even with another window in front
/tmp/drive drag 900 500 1100 560 middle
```

⚠️ Every synthetic event declares its own flags, empty ones included: a
`CGEvent` with a null source inherits the session's combined modifier state, so
a `key v cmd` leaves ⌘ hanging and the next `type` turns into a sequence of
⌘-something. And clicks close together in time become double/triple clicks by
the system's clock — wait ~1s between them.

⚠️ Always point a driven app at a scratch vault (`SYNAPSO_VAULT`) and a scratch
config (`SYNAPSO_CONFIG`): it writes.

## Design

- `design/` — the source of truth: `tokens.css` (the semantic token layer) and
  the `.dc.html` screens generated in Claude Design. `Workspace.dc.html` is the
  integrated mock; when the variant boards disagree, it wins. Tests hold both
  `crates/synapso-core/src/theme.rs` and `src/theme.rs` to `tokens.css`, so the
  palette and the design cannot drift.
- The `sc-for` / `sc-if` / `{{ … }}` in a `.dc.html` are rendered in a browser
  by the Claude Design runtime, `design/support.js`, which is not part of this
  repository: drop a Claude Design export's copy beside the screens to view
  them (it is git-ignored). Nothing in the build reads it.
- Signature element: the **colour line** — the 2px line in the document's (or
  its folder's) colour that runs tab → tree row → note gutter → card on the
  canvas.

Typefaces: JetBrains Mono for the app, and Excalifont for what is written
on the canvas (text, captions, edge labels). Both are vendored in
`assets/fonts/` and OFL licensed.

## Structure

| Module | Role |
|---|---|
| `crates/synapso-core/` | The model, parsers and geometry, with no UI dependency: the vault and JSON Canvas (`vault.rs`), the note's line kinds (`sheet.rs`), inline markdown (`inline.rs`), tables, the canvas scene (`board/`), undo history, tokens |
| `src/main.rs` | The window, the layout, global shortcuts, ⌘O, file drop |
| `src/app.rs` | The shared state: open documents, view, overlays, toasts, undo of trash |
| `src/session.rs` | What the app reopens with, from the sidecar |
| `src/index.rs` | The vault read once: folders, documents, links, colours and their inheritance |
| `src/note.rs`, `src/rich.rs` | The note sheet: the buffer, its lines, and the inline layer on top |
| `src/behave.rs`, `src/format.rs` | What ⏎/⇥/⌘⏎ mean, and what each format key does to the text |
| `src/board.rs` | The canvas, drawn in one Skia pass and driven through the core's `Scene` |
| `src/graph.rs` | The graph view |
| `src/titlebar.rs` | The macOS window bar |
| `src/tree.rs`, `src/rail.rs`, `src/status.rs` | The folder tree, the tab rail, the status bar |
| `src/overlays.rs`, `src/field.rs`, `src/toast.rs`, `src/colorpop.rs` | Palette, picker, search, inline fields, toasts, the colour tooltip |
| `src/theme.rs`, `src/icons.rs`, `src/keys.rs` | Tokens, 16×16 stroke icons, shortcuts spelled per platform |
| `src/stats.rs` | Frame timings (`SYNAPSO_STATS`) |

## Release

`git tag v0.3.0 && git push origin v0.3.0` fires
`.github/workflows/release.yml`: it tests and builds **Windows** on GitHub's
hosted image and leaves a **draft** release with `synapso.exe` zipped in.
The *Run workflow* button in Actions does the same build without publishing
anything, and the `.zip` stays as a run artifact.

The **macOS** half is built by hand on a Mac, with the recipe in
`tools/release-macos.sh`:

```sh
tools/release-macos.sh v0.3.0 --upload
```

It runs `cargo test --release --locked`, checks the binary really is arm64,
assembles `Synapso.app`, zips it with `ditto`, and adds it to the draft —
refusing to touch a release somebody already published. Publishing is by hand,
after both `.zip` files are on the draft.

Neither binary is signed, so Gatekeeper and SmartScreen warn on first launch
(the release notes say how to get past them).

On Windows the release binary has no console window, so nothing it prints is
seen; `SYNAPSO_STATS` writes to a file for that reason.

## Does not exist yet

- The app's own title bar on Windows and Linux: the system's is used there.
- Per-resource destinations (design 1c): where new notes, boards and pasted
  pictures go, picked from the status bar.
- Design variants not built: tree 5b, colour 2a–2d and 3, confirmations 1d and
  1g. Built: tree 5c (pinnable into 5a), colour 4, confirmations 1e and 1f.
- The `MD` badge in the header is a label, not a button: it becomes a picker
  when there is HTML export.

## License

**Apache License 2.0** — full text in [`LICENSE`](LICENSE), attributions in
[`NOTICE`](NOTICE).

Two exceptions: the fonts embedded in the binary are not under the Apache
licence. Each carries its own terms, the SIL Open Font License 1.1:
**JetBrains Mono** in [`assets/fonts/OFL.txt`](assets/fonts/OFL.txt), and
**Excalifont** in [`assets/fonts/OFL-Excalifont.txt`](assets/fonts/OFL-Excalifont.txt).

Found a security problem? Do not open an issue — [`SECURITY.md`](SECURITY.md)
says where to take it.
