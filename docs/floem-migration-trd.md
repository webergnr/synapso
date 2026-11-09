# TRD — Moving the shell from iced to Floem

> **Superseded** by [ADR-009](adr-009-freya.md): the Floem shell was replaced by
> the Freya one on 2026-10-16. Kept as history.

**Date:** 2026-08-19 · **Status:** phases 0–3 built and merged (2026-08-22);
**phase 4, the cutover, is done** — the iced shell and the `webergnr/iced` pin
are deleted and the Floem crate is the root `synapso` package (§10, step 4).
· **Context:** the
note editor hit a wall that is not ours to move — a `text_editor` cannot draw
two font faces on one line (ADR-008, *O teto que apareceu*). Written against
`main@9c856cc`. Every number below is counted, not estimated; where a number
cannot be counted honestly this document says so and Phase 0 exists to count it.

## Open questions for Weber (pinned — answer whenever, the rest can be planned)

1. **Does the board move too, or does it stay on iced?**
   It cannot stay: iced and Floem are both winit apps and neither hosts the
   other, so the cutover is per *process*, not per widget. [ASSUMED: everything
   moves, in phases, with two binaries alive during the transition — §5.] The
   alternative is a second window in a second process, which is worse than the
   migration.
2. **Pin Floem, or track it?** Floem is `0.2.0`, last commit 2026-06-21, and
   its README says "occasional breaking changes … on our way to v1".
   [ASSUMED: pinned to a git rev, exactly like `webergnr/iced` is today
   — see [[synapso-iced-fork]]. We already run this way and it works.]
3. **The inline-box gap (§4.4) — contribute upstream or fork?**
   [ASSUMED: contribute. It is a small, obviously-correct addition to Floem's
   `TextLayout`, and a fork of Floem is a much heavier thing to carry than a
   fork of iced was.]
4. **Is the ⌘K/⌘P/modal chrome a straight port or a redesign?**
   [ASSUMED: straight port, same design, same tokens. A migration that also
   redesigns cannot be reviewed — nothing can be diffed against "the same
   thing, working".]

## 1. Why — the finding, in one paragraph

Bold and italic **have never been drawn** in synapso's note. What was on screen
was the `**`, not the weight. cosmic-text resolves the font face **once per
shaping run**, from the attributes of the run's first byte (`shape_run`,
cosmic-text 0.15), while colour, size and metrics it reads **per glyph**. That
is why our fork's `Format::size` works — the marker vanishes — and
`Format::font` never has inside a line: a heading is bold because the *whole
line* asks for bold; `**this**` is not, because the line around it asks for
regular. Proven by probe, not by reading: painting the span red *and* italic
turned the word red and left it upright.

The three ways out of that inside iced are all forks (variable font,
`CacheKeyFlags::FAKE_ITALIC` plumbed through `Format`, or splitting runs in a
fork of cosmic-text). This TRD proposes the fourth: stop paying for a text
stack that cannot do rich text.

## 2. Evidence — what was measured, not read

**Spike 1 — `~/Projects/synapso-floem-spike`** (one file, 114 lines). One
Floem `rich_text`, one `AttrsList`, our own JetBrains Mono registered:

- `italico` came out **italic** and `negrito` came out **bold**, on the same
  line as regular body text. This is the exact thing iced refuses.
- A heading at 26 px and body at 15 px in the **same layout**, no fork.
- Markers at size `0.01` disappeared, the same trick we use today.

**Spike 2 — Floem's own `syntax-editor` example, run.** Gutter, current-line
highlight, per-span syntax colour, and typing `// editando ao vivo` into a line
re-styled it as a comment immediately. The editing path is real, not a demo of
static text.

**Parley 0.7, read directly** (Floem's text engine — it is not cosmic-text):

| capability | parley | floem exposes it | synapso today |
| --- | --- | --- | --- |
| per-range weight / style / size / family | ✅ | ✅ `Attrs` | ✗ (fork, and only size) |
| per-range brush (colour) | ✅ | ✅ | ✅ |
| **`Strikethrough` / `Underline`** (+ offset, size, brush) | ✅ | ✗ | painted by hand |
| **`InlineBox`** — a sized box at a byte offset, in the flow | ✅ | ✗ | ✗ (§4.4) |
| `PlainEditor` + IME + accesskit | ✅ | wrapped by `editor-core` | n/a |
| per-span background | ✗ | ✗ | painted by hand |

`InlineBox` is the piece worth naming twice: it is the *atomic image node*, the
one thing ADR-008 lists as a known ceiling and the one thing the current design
cannot express — the cursor stopping invisibly inside a picture's row, and the
arrow key entering where the click cannot.

## 3. Census — what actually has to move

23,387 lines, 16,698 of them production, **313 mentions of `iced` across 16
files**.

| module | lines | prod | tests | `iced` | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| `doc.rs` | 1309 | 762 | 547 | 0 | **moves as-is** |
| `history.rs` | 303 | 165 | 138 | 0 | **moves as-is** |
| `bench.rs` | 91 | 91 | 0 | 0 | **moves as-is** |
| `find.rs` | 360 | 248 | 112 | 1 | **moves as-is** (the one mention is in a test) |
| `table.rs` | 309 | 216 | 93 | 1 | **moves as-is** (a comment) |
| `i18n.rs` | 234 | 190 | 44 | 1 | **moves as-is** (a comment) |
| `theme.rs` | 162 | 162 | 0 | 1 | type swap — `Color`/`Shadow`/`Vector` |
| `vault.rs` | 1728 | 1206 | 522 | 34 | type swap — `Color`, `Rectangle` in the model |
| `color.rs` | 439 | 362 | 77 | 9 | type swap + one `canvas::Program` |
| `icons.rs` | 204 | 204 | 0 | 7 | paths — `canvas::Path` → `kurbo` |
| `inline.rs` | 495 | 326 | 169 | 7 | output type — `Format` → floem `Attrs` |
| `platform.rs` | 73 | 73 | 0 | 4 | window chrome, rewrite |
| `editor.rs` | 1016 | 865 | 151 | 21 | rewrite |
| `rail.rs` | 672 | 528 | 144 | 11 | rewrite |
| `picker.rs` | 715 | 634 | 81 | 10 | rewrite |
| `palette.rs` | 639 | 587 | 52 | 16 | rewrite |
| `ui.rs` | 412 | 412 | 0 | 28 | rewrite |
| `sheet.rs` | 1991 | 1666 | 325 | 28 | rewrite the view, keep the parser |
| `board.rs` | 5048 | 3859 | 1189 | 78 | rewrite the view, keep the geometry |
| `main.rs` | 7187 | 4142 | 3045 | 56 | rewrite |

**2,606 lines have zero coupling** (934 of them tests) and **1,890 more are a
type swap**. The remaining ~18,900 is where the shell lives — but that number
lies in our favour and I will not pretend otherwise: most of `board.rs` is edge
routing, hit tests, shape paths and resize anchors, which is arithmetic over
`Point`/`Rectangle` and survives a shell change with its imports rewritten.
Likewise most of `sheet.rs` is the line parser.

**I cannot split that honestly by reading. Phase 0 is the measurement** — when
the logic compiles as a crate with no shell dependency, the number is a fact
instead of a guess, and the decision to continue can be taken on it.

### 3.1 The measurement, taken (Phase 0, branch `feat/synapso-core`)

The crate exists and compiles clean with no shell dependency. Measured:

| | lines | of which tests |
|---|---:|---:|
| `crates/synapso-core` (12 modules + fixtures) | 5,758 | 1,838 |
| the iced shell (`src/`) | 18,177 | 5,276 |
| conversion scaffolding (`src/conv.rs` + call sites) | 427 sites | — |

The reading-based census above said 2,606 zero-coupling + 1,890 type-swap
lines would move; what actually moved is 5,749 — the parser halves of
`sheet`/`inline` and the board's card arithmetic went in on top of the
predicted set, and `find`/`vault` needed the `Scannable` trait and kurbo
adapters the census did not predict. **273 tests green across both crates** —
94 core + 179 shell — zero `iced` paths in core. `main.rs`'s 3,046-line
Message-driven test tail stays in the shell, as §8 risk 5 says, until each
phase rewrites its half.

## 4. Design

### 4.0 The update loop — from Elm to reactive

The shell is written in iced's Elm dialect: a `State` struct, a `Message`
enum with **67 variants** (`pub enum Message`, main.rs), and an `update()`
whose match runs main.rs:1454–2398 (**944 lines**). Floem is reactive:
state is a signal, an event is a closure on the view that receives it, and
there is no `update()` at all. Nothing in the model changes — this section
is the whole of the translation, which the first draft of this TRD never
spelled out.

| In the iced shell today | In Floem | What happens to the code |
|---|---|---|
| `State` struct field | `RwSignal<T>` created by the view that owns it | 1:1, field for field |
| `Message` variant | closure on the originating view (`.action()`, `.on_event_stop(listener::…)`) | variant + its match arm both die |
| `update()` arm | the body of that closure (or a fn it calls, for the non-trivial ones) | arm dies, body is reused |
| `Task::perform` async work | channel into the view; background thread updates the signal | **zero uses today** — nothing to port |
| `Task::batch([focus(…), scroll_to(…)])` | imperative calls inside the action closure | 56 `Task::` mentions, all focus/scroll plumbing |
| `subscription()` → `Tick` (600 ms autosave) | timer updating a signal; a save effect tracks it | 1 subscription |
| `subscription()` → `keys()` (global chords; `CloseRequested` → Save) | window-level `KeyDown` listener on the root, ahead of child views | the 36-chord table in §9 |
| `canvas::Cache` + manual `.clear()` in update arms | nothing — `canvas(|cx, size| …)` re-runs when a signal it read changes | every cache + every clear |
| `view(&State) -> Element` (whole tree, every message) | fine-grained: only views whose tracked signals changed re-run | the function dissolves into views |

The 67 variants, bucketed by what each becomes:

- **Widget actions with a clear origin — become closures, die as variants
  (41):** the modal family (ModalInput/Color/Pick/Hex/Submit/Cancel,
  CanvasColor), the find bar (FindOpen/Input/Step/Scope/Case/Close), the
  picker (OpenPicker/PickerInput/Choose/Scope/ScopeStep/ToggleOpen/New/
  Close), the palette (OpenPalette/PaletteInput/Choose/Step/Close), the
  rail and split (RailScroll, SplitDrag), SelectDoc, CloseDoc, SelectTool,
  Zoom, SetView, SetTheme, ToggleTheme, NodeInput, NodeCommit, Follow.
- **Keyboard chords — become the root `KeyDown` listener (19):** Escape,
  ⌘B/I/K/P/W/F/Z/⇧Z/C/V/D/N/S, ⌘1/2/3, ⇧1, ⇧0, Delete, ⌘⏎, ⇥/⇧⇥. The full
  list is §9's parity table.
- **Cross-surface coordination — become effects that watch signals (5):**
  `Board(board::Event)` (the canvas view owns its drag state machine and
  writes scene changes into signals the parent reads), `Menu(MenuItem)`,
  `Sheet(text_editor::Action)` (Floem's editor owns its buffer outright),
  `EditorPointer`, `EditorMenu`.
- **Ambient — become timers / window listeners (2):** `Tick` (autosave
  interval), and `Save` on `CloseRequested`/`Unfocused` — floem has a
  `WindowCloseRequested` listener today.
- **Menu-/button-origin twins:** `Pasted`, `Duplicate`, `Copy`, `Cut`,
  `Undo`, `Redo`, `Save` also fire from menus and buttons — the same
  closure attaches to every origin, which is what the single update arm
  does today.

**One example, end to end: `RailScroll`.** Today it is three places and a
cache:

```rust
// main.rs — the variant (enum Message)
RailScroll(f32),
// main.rs — the update arm
Message::RailScroll(offset) => {
    s.rail_scroll = offset.max(0.0);
    s.rail_cache.clear(); // the rail is a cached canvas — no clear, no redraw
}
// main.rs — the view reads it back
canvas(rail::Rail { active: s.active, matched: matched_notes(s),
                    scroll: s.rail_scroll, t, cache: &s.rail_cache, … })
```

After:

```rust
// the field a struct field used to be
let rail_scroll = RwSignal::new(0.0_f32);

canvas(move |cx, size| {
    let scroll = rail_scroll.get();      // tracked: paint re-runs on change
    draw_rail(cx, size, scroll, &index);
})
.on_event_stop(listener::PointerWheel, move |event, _| {
    let delta = event.delta().x;
    rail_scroll.update(|s| *s = (*s + delta).max(0.0));
})
```

The variant, the match arm, `s.rail_scroll`, `s.rail_cache` and the manual
`.clear()` all stop existing. That is the shape of every one of the 41
widget-action conversions; the other buckets are the same with a listener
or an effect in place of the wheel handler.

`State` itself does not vanish on day one: the document list, the vault,
and the undo stacks stay plain structs in `synapso-core`, and the Floem
views wrap them in signals. What dies is `update()` — the 944-line match
is the Elm/reactive seam, and §5 Phase 3 ports it surface by surface,
never all at once.

### 4.1 The model is already right — this is a substrate change

The buffer is the truth, the parse is derived, and one continuous document
carries the cursor. That landed in PR #76 and is not up for renegotiation here.
What changes is *who lays the text out*.

Two things get **deleted** rather than ported:

- `inline::vanishes` — the rule that keeps `*` and `**` on screen because
  nothing else says the word is emphasised. On parley the emphasis is drawn,
  so the marker can go, and the rule goes with it.
- The hand-painted strikethrough in `Gutter::over`, once §4.4 lands.

### 4.2 The note

Floem's `Styling` trait is the same shape as our highlighter:

```
fn apply_attr_styles(&self, edid, line, default: Attrs, attrs: &mut AttrsList)
fn font_size(&self, edid, line) -> usize
fn weight(&self, edid, line) -> FontWeight
fn italic_style(&self, edid, line) -> FontStyle
fn line_height(&self, edid, line) -> f32
```

`sheet::Highlight::highlight_line` becomes `apply_attr_styles`; `sheet::metrics`
becomes `font_size`/`weight`/`line_height`. The per-line half of what we do is
*built in* — a heading being bigger than the body is not a fork, it is a method.

The buffer becomes `lapce_xi_rope::Rope` instead of `String`, which also
answers the reparse ceiling ADR-008 flags: a rope edit is not an O(n) copy.

### 4.3 The board

`floem::views::canvas(|cx, size| …)` with `peniko`/`kurbo` — `cx.fill(&path,
color, blur)`, the same shape as `canvas::Program::draw`. The board's own
mathematics does not care which crate names the rectangle.

Three things in Floem's renderer are **better than what the board has today**,
and they delete code rather than port it:

- `set_transform(Affine)` — the viewport becomes a matrix. `Viewport::to_screen`
  / `screen_rect` and every hand-multiplied zoom in `board.rs` stop existing.
- `push_layer` / `pop_layer` / `set_z_index` — real layering. The trap recorded
  in `draw_node`'s comments (*"a canvas draws its images above its own fills,
  whatever order they were asked for in"*) is a cosmic-text/iced artefact and
  goes away; so does the `Layer::Under`/`Layer::Over` split in `sheet.rs`.
- `draw_svg` — `icons.rs` is 204 lines of hand-transcribed SVG paths because
  iced's canvas cannot take an SVG. It could go back to being the design's
  actual assets.

The parts that need real thought, not translation: the pointer-event routing
(`board::Event`), the drag state machine, and the stacked overlays (style bar,
inline editor, minimap, context menu). None of them is novel; all of them are
where the schedule will actually go.

### 4.4 The gap Floem has and parley does not

Floem's `TextLayout` does not pass parley's `InlineBox` or its
`Strikethrough`/`Underline` style properties through its `Attrs`. It does hand
out `parley_layout()` read-only, so the wrapper is thin — the addition is
`set_text` learning to take boxes and forward them to parley's `RangedBuilder`.

Until that exists: images stay as they are today (a row drawn at nothing with a
picture painted over it), and the strike stays painted. **Neither blocks the
migration** — they are the two things that get *better* after it, which is the
right order.

**Plan B if upstream does not take it:** maintain a floem fork pinned by rev
in `[patch.crates-io]` — the identical pattern this repo already runs for
`webergnr/iced` (per-span font size, `cfd5425`), which the fork's years of
service here prove is sustainable. The wrapper lives in one file either
way; a fork turns the pull request into a patch. The trigger to switch
plans: a Phase-1 PR sitting unmerged upstream for a month.

## 5. Phases — each one runnable, each one comparable

**The cutover is per process.** Two shells cannot share a window. What *can*
share is the logic, as a crate, driving two binaries.

- **Phase 0 — `synapso-core`.** Extract every module with zero or type-only
  coupling into a library crate with no shell dependency: `doc`, `history`,
  `find`, `table`, `i18n`, `vault`, `theme`, the parser halves of `inline`
  and `sheet`, and the arithmetic parts of `board`. Geometry and colour
  become `kurbo`/`peniko`.
  ⚠️ *Correction to the first draft of this TRD:* iced's public API does
  **not** consume kurbo/peniko — it takes its own `Point`/`Rectangle`/
  `Size`/`Color`, all f32 (kurbo is a *private* dependency of
  `iced_tiny_skia`, and not at the same version Floem wants). "`main` keeps
  working" is true only through a conversion layer: the shell carries
  `src/conv.rs`, two extension traits (`.iced()` at every draw call,
  `.kurbo()` at every event entry — the orphan rule bars `From` between two
  foreign types) that exist solely to bridge the two vocabularies.
  Measured on the built branch: **427 conversion sites**. The file and
  every site die at cutover. Two details the first draft also missed:
  `find::scan` took the shell's `Doc` by parameter — it now takes a
  `Scannable` trait (`plain()`, `markdown()`) the shell implements — and
  `vault` keeps `arboard` (system clipboard image) as a core dependency,
  because a clipboard is a system service, not a shell.
  *Value even if the migration is cancelled:* the coupling becomes a
  compiler error instead of an opinion, and §3's unknown becomes the
  measured table below.

  **Crate layout after Phase 0** (as built, branch `feat/synapso-core`):

  ```
  synapso/                    # workspace root
  ├── Cargo.toml              # [patch.crates-io] iced fork + [workspace] members
  ├── src/                    # the iced shell (bin) — conv.rs dies at cutover
  │   ├── main.rs             # State, Message, update(), view(), keys()
  │   ├── board.rs  sheet.rs  inline.rs  editor.rs  …     # view halves
  │   └── conv.rs             # .iced() / .kurbo() — the boundary scaffolding
  └── crates/
      └── synapso-core/       # no shell. Not one iced path in its code.
          └── src/            # doc, table, history, find, i18n, bench, theme,
                             # vault, color, card, + parser halves of
                             # sheet/inline
  ```
- **Phase 1 — the note, on Floem.** A second binary, `synapso-floem`, that
  opens a vault and shows *one note* on Floem's editor with our `Styling`.
  Side by side with the real app. This is where the ADR-008 ceilings are either
  gone or are not.
- **Phase 2 — the board**, split so every unit is a reviewable PR on its own
  (board.rs is 3,858 production lines — one PR is not a unit):
  1. **Card text metrics** — `wrap`, `ellipsize`, `one_line`,
     `card_height`, the column arithmetic. *(Already `synapso-core::card`
     after Phase 0 — this unit only verifies it from Floem.)*
  2. **Geometry & hit-testing** — `distance_to_path/segment`, `corner`,
     `spanning`, `normalized`, `handle_points/at`, `anchor_point`,
     `grip_point`, `resize`, `resize_locked`, `Occlusion`, `text_box`:
     moved to kurbo in Phase 0's tail or re-typed here, then given Floem's
     `Affine` so the hand-multiplied zoom dies.
  3. **The scene model** — `Viewport`, `Scene`, `Selection`, `Event`,
     `Outcome`: state + gesture semantics, signal-wrapped.
  4. **Pointer plumbing** — the drag state machine
     (`canvas::Program::update` → `.on_event_stop(listener::…)` on the
     canvas view). The part §4.3 already flags as "real thought, not
     translation".
  5. **Node & edge drawing** — `draw_node`, `image_node`, `doc_card`,
     `edge_path`, `edge_label`, `end_handle`, `angle_readout`,
     `selection`.
  6. **Chrome** — `grid`, `hint`, zoom/pan via `set_transform(Affine)`,
     and the style bar / inline editor / minimap overlays
     (`push_layer`).
- **Phase 3 — the chrome.** Rail, ⌘K, ⌘P, modals, status bar, window chrome.
  Split, as phase 2 was, into units that each ship on their own:
  1. **The frame** — top bar (name, view keys, save chip), both panes with a
     draggable divider, status bar, the rail over both halves of the vault.
     *(built)*
  2. **⌘K / ⌘P** — the palette, and with it the only way to create a note or a
     board in this binary. *(built)*
  3. **⌘F** — find in the open note, painted by the styling. *(built)*
  4. **The note's own chrome** — breadcrumb, rename/delete, the formatting
     toolbar. *(built)*
  5. **Tabs, themes and the modals** — the tab rail with its close keys, the
     theme switch (which makes `Tokens` reactive, today a value), the colour
     picker and the confirm-delete. *(built — `tabs.rs`, `tokens.rs`,
     `modal.rs`)*
  6. **Window chrome** — the traffic lights inside our own bar. *(built)*
  Also outstanding, and not chrome: the editor *behaviours* the shell's
  `sheet.rs` carries (list continuation, the checkbox, table editing) and the
  board's half of ⌘F. *(both built — `behave.rs`, and `board::mark`)*

  **Phases 1–3 are complete as of 2026-08-22.** What the Floem binary does not
  do that the shell does is now a short list, and none of it is chrome:
  - **⌘Z.** `synapso-core::history` is extracted and unused here; the Floem
    editor has floem's own undo inside one note and nothing across a rename,
    a delete or the board.
  - **the shell's longer palette** — mentions (`[[`), scopes for ⌘F, "follow a
    link", the commands that only make sense with several notes loaded.
  - **`paste`** that replaces a line which is nothing but its opening syntax.
  - **a heartbeat**: the board judges its quote cards once, when it opens.
- **Phase 4 — cutover.** `synapso-floem` becomes `synapso`; the iced binary and
  `[patch.crates-io]` are deleted, and `webergnr/iced` stops being load-bearing.
  ⚠️ **Weber's call, and only his.** Everything up to here has been additive —
  two binaries, either one runnable. This step deletes a working app (5,600
  lines under `src/`, recoverable only from git) and makes the Floem one the
  only one there is. It is the point where the gates below stop being a
  comparison and start being the whole test.

**The gate between every phase is the same question:** does the Floem binary do
what the iced one does, on the same vault, with the same file bytes out? The
vault format does not change in this migration — that is what makes the two
binaries comparable at all, and it is a hard constraint.

## 6. What this buys, stated as things that stop being true

- "Bold and italic are only their markers" — ADR-008.
- "A heading cannot be bigger than the body without our fork" — the reason the
  fork exists at all.
- "`inline::vanishes` keeps `*` on screen."
- "A picture's row has no cursor" — after §4.4.
- "The strike is painted" — after §4.4.
- "The viewport is arithmetic on every point" — `set_transform`, §4.3.
- "Anything filled must be drawn under the editor or it paints out the text."
- "`icons.rs` re-draws the design's SVGs by hand."

- "`webergnr/iced` must stay alive or the build breaks" — memory
  [[synapso-iced-fork]] stops being a liability.

## 7. Deferred, with triggers

- **Contributing `InlineBox` to Floem** — trigger: Phase 1 lands and the image
  row is still the worst thing in the note.
- **Dropping the JetBrains Mono static faces for the variable file** — parley
  handles both; only worth it if we want weights between Regular and Bold.
- **Accessibility** — parley ships accesskit integration and iced does not.
  Not a goal of this migration; do not let it become one mid-flight.
- **Moving off `vger`, floem's default renderer** (found drawing Phase 2 unit
  5's second slice, 2026-08-21; resolved 2026-08-22 — decision below). Three
  things that backend cannot do, all proved on screen:
  - **A cubic stroke.** vger hands the GPU ONE quadratic per cubic, control
    `((p1+p2)*3 - p0 - p3)/4`. For the symmetric curve every connector on the
    board is, that point lands exactly on the chord's midpoint — a degenerate
    quadratic the shader draws as *nothing*. Two edges of the parity board were
    invisible, arrowheads and label present.
  - **A dash pattern.** vger's `stroke` reads `stroke.width` and no other field;
    `Stroke::with_dashes` is silently dropped. Dashed edges and the frame's
    border came out solid.
  - **Rotated text.** vger's glyph path says `// This assumes that text is
    axis-aligned.` and discards rotation outright — the design's vertical rail
    and rotated node captions were never drawable on it. ⚠️ lapce/floem#685
    ("Support transformations in vger") has been open since Nov 2024 with no
    activity — no near-term fix to wait for upstream.
  `board::inked` (`crates/synapso-floem/src/board.rs:2373`) worked around the
  first two with `kurbo::flatten` + `kurbo::dash`; nothing worked around the
  third short of a different renderer.

  **`vello`, tried and rejected.** Floem's `vello` feature draws rotated text
  correctly, but any two visible children anywhere in the workspace paint an
  empty frame — bisected past `Resizable`, `width_pct`, editor+canvas, and
  flex-vs-absolute layout, still empty. Parked at branch
  `spike/vello-renderer`, not merged.

  **Decision: `skia`.** Floem's `skia` feature (`floem_skia_renderer`, merged
  upstream March 2026, present at our pinned rev) — PR #108. Split paints,
  rotated text draws, dashes are native; build ~1 min off prebuilt skia-safe
  binaries, binary ~36 MB. Two renderer bugs found and fixed:
  - `image_transform` in `skia/src/lib.rs` applied `.pre_scale_non_uniform()`
    before `.pre_translate()`, so every image (and SVG) landed at the scaled
    origin instead of its rect — fixed in the fork,
    `webergnr/floem@257889e` (branch `synapso`,
    `crates/synapso-floem/Cargo.toml:21`).
  - Skia draws nothing below ~4.9 px nominal text (4.5 px vanished, 4.95 px
    drew) — gated by `MIN_TEXT = 4.9` (`crates/synapso-floem/src/board.rs:66`,
    measured bracket recorded in the comment above it).
  `board::inked`'s flatten+dash workaround is now a **deletion candidate**
  once verified visually under skia — it was a vger-specific patch and skia
  needs neither half of it.

## 8. Risks, named

1. **Floem is 0.2.0 and moves.** Breaking changes are promised. Mitigation:
   pin the exact set the spike proved (~/Projects/floem-spike, lockfile of
   record): **floem 778bb5f** (authored 2026-06-22, committed 2026-06-21 —
   `origin/main` at spike time), **parley 0.7.0**, **kurbo =0.13.0**,
   **peniko =0.6.0**, **lapce-xi-rope 0.4.0**. `synapso-core` already pins
   kurbo/peniko with `=` — a caret would have taken 0.13.1, which changes
   `Rect::inflate`'s arity; the pin is load-bearing, not ceremony. When
   Phase 1 adds floem to the workspace it pins by git rev the same way
   `[patch.crates-io]` pins the iced fork today. Residual risk unchanged:
   an upstream refactor of the editor views, which is exactly the part we
   depend on most.
2. **The editor views are the least-loved part of Floem.** Floem's own README
   sells it as a general UI library; the editor is inherited from Lapce. If
   upstream deprecates `views/editor`, we inherit `editor-core` and the parley
   layer directly — expensive, but not a dead end, because parley is the part
   that matters and it is maintained by linebender.
3. **A migration that never finishes is worse than the wall.** Two binaries is
   the mitigation and also the danger: it makes stopping halfway comfortable.
   Phase 4 has to be scheduled, not hoped for.
4. **The board is the schedule.** 3,859 production lines and every pointer
   gesture in the app. If this TRD is wrong about effort, it is wrong here.
5. **6,689 lines of tests.** The ones over logic moved with Phase 0
   (1,843 lines, green in `synapso-core` on every build). The ones
   that drive `State` through `Message` (most of `main.rs`'s 3,045) do not
   survive a reactive shell. **Scheduled, not deferred:** every phase's
   PR-sized units carry their own rewrite — Phase 1 rewrites the
   editor-driving tests as buffer-level tests against `synapso-core` plus
   a small Floem harness; Phases 2–3 likewise, surface by surface. Until
   Phase 4 the iced binary keeps running the old suite on every CI build,
   so the net is never thinner than the un-migrated half.

## 9. How to test

`diff -rq` between vaults touched by each binary is the **format** gate — it
says the bytes out are the same, nothing about the shell. The shell gate is
the table below, run by hand on a copy of `~/Synapso`: every row is a
thing a person can do in five seconds and see pass or fail.

| # | Check | Phase |
|---|---|---|
| 1 | `cargo test` green (core + shell) · `grep -rn "iced::" crates/synapso-core/src` = 0 (no `iced` type paths; comments may say "iced" in prose) · `cargo check --bin synapso` green · the app boots on the real vault | 0 |
| 2 | Both binaries boot, open the same vault, show the same note list | 1 |
| 3 | A heading renders larger than the body **with no fork in play**; bold/italic/inline-code render as style, not markers; a picture row sits in the text flow; typing autosaves after 600 ms; file bytes diff-clean | 1 |
| 4 | Create/move/resize a rect; connect two nodes with an edge; ⌫ deletes; ⌘D duplicates the selection | 2 |
| 5 | ⇧1 (fit) / ⇧0 (100%); pan; drag rubber-band multi-selects; the snap grid is visible | 2 |
| 6 | Every chord in the list below fires the same behaviour in both binaries | 3 |
| 7 | Modals (rename / new note / colour picker ⌘⇧L flow), ⌘K palette (filter + choose), ⌘P picker (scope ⇥, toggle-open, `+ new note` row), find bar ⌘F (scope, case, wrap), title-bar drag, ✕ saves first | 3 |
| 8 | `[patch.crates-io]` gone; `webergnr/iced` deletable; full chord sweep + a day of real use | 4 |

**Rows 1–7, run 2026-08-22 on a copy of `~/Synapso` with phases 1–3 complete.**
The three format gates, driven with real clicks and keys and then read off
disk:

- opening the vault wrote **nothing** (`diff -rq` against an untouched copy is
  empty, and the binary added no file of its own);
- typing changed **exactly one** `.md`, by **exactly** the characters typed
  (`asd` → `asXYZd`, nothing else in the tree);
- dragging a node changed **only** that node's `x`/`y` — every other node byte
  for byte, the edges identical, no key added at the top level.

⚠️ Row 7's chord list is the shell's; the ones this binary does not have yet
are named under phase 3 above (⌘Z the notable one).

The chords (36, from `keys()` and the editor's internal handling):
Esc · ↓ · ↑ · ⇧⏎ (fence exit) · Tab · ⇧Tab · ⌘B · ⌘I · ⌘K · ⌘P · ⌘W · ⌘F ·
⌘Z · ⌘⇧Z · ⌘C · ⌘V · ⌘⇧V · ⌘D · ⌘N · ⌘S · ⌘1 · ⌘2 · ⌘3 · ⇧1 · ⇧0 ·
⌫/Delete · Enter (canvas confirm) · ⌘⏎ (toggle task) · ⇥/⇧⇥ (table cell) ·
⌘⇧L · tool digits 1–5 · ⌘X.

## 10. Design fidelity — the plan after Phase 3

**Observation** (2026-08-22 comparison, a screenshot of the design reference
vs. one of the app after the port): everything the app draws itself — the board —
matches the design. Everything rented from the toolkit — the note body on
floem's `text_editor`, the sidebar list instead of the rail — does not. Every
rented surface has cost a fork or a patch: iced's per-span font size, floem's
variable line height, and the vger workarounds in §7. Renting text layout has
been more expensive than owning it.

**Architecture — three layers:**
- **`synapso-core`** — model, toolkit-free. Exists.
- **Surfaces** — board and note, both owned display lists. The board already
  is one: a `canvas()` with our own hit-testing and gestures. The note becomes
  the same: `doc::Lines` → one parley layout per block → painted by us. The
  editing operations already exist in `format`/`behave`/`history` and do not
  change — only who lays the glyphs out does.
- **Host** (`synapso-floem`) — window, chrome, input, reactivity. Unchanged.

One renderer (skia, §7), one text stack (parley — already floem's at our
pinned rev, no new dependency).

**Known ceiling of an owned note surface:** IME composition (CJK) is out of
scope; dead-key accents arrive as ordinary text events, unaffected.

**Order:**
1. **The vertical rail** — cherry-pick the rotated-text drawing from
   `spike/vello-renderer` onto skia. PR in progress.
2. **The note as an owned surface on parley.** Shipped in slices:
   read-only, then the caret and typing, then the behaviours and ⌘F, then
   pictures and tables. **It is the note** — it became the default on
   2026-08-22 with `SYNAPSO_RENTED_EDITOR=1` as the way back to floem's
   `text_editor`, and that path was deleted after the week of real use it
   was kept for. The §9 format gates hold: opening a vault writes nothing,
   and typing changes exactly one `.md` by exactly the characters typed.
3. **Chrome to the five design frames.** `design/Notes Canvas Workspace.dc.html`
   needs a compiled export; `src/tokens.rs` is generated
   from `design/tokens.css`; each PR carries an app-vs-frame screenshot side
   by side, the same comparison behind the observation above.
4. **Phase 4 cutover** (§5) — done: the iced shell and the `webergnr/iced`
   pin are deleted and the Floem crate is the root `synapso` package.
