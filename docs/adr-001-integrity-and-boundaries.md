# ADR 001 — Data integrity first, boundaries later

**Date:** 2026-08-15 · **Status:** accepted · **Context:** architecture review
for production/open-source (external briefing + verification against the code).

> ⚠️ **Historical.** Written while the shell was iced. Where this names an iced
> type or file, read it as the shape of the problem, not as the code today —
> the cutover landed (`docs/floem-migration-trd.md` §5) and the shell is Floem.
> The decisions still hold; the mechanics moved.

## Decision

Do **now** everything that stops the app from damaging someone's files. Do not
do the split into crates, nor a protocol, nor collaboration.

The reason is the order of risk, not architectural laziness: the persistence
defects are data loss **today, in solo use**; crate boundaries only start being
worth something once a second consumer exists (CLI/MCP/server), and none does. A
four-crate `workspace` with no second consumer is maintenance cost buying an
option nobody exercised.

## Done (phase 1 — integrity)

| # | Before | Now |
| --- | --- | --- |
| 1 | `read()` returned an empty `String` on any IO error | `io::Result`; the note opens **marked** and the save is refused. **The mark was recorded and never drawn — it only reached the screen in n14** |
| 2 | `Canvas::load` fell back to an empty board on invalid JSON | `io::Result`; empty board on screen, file untouched |
| 3 | Save overwrote blindly | `write_checked`: mtime+size checked; a conflict is a refusal with a warning, and `reload vault` is the way out |
| 4 | Frontmatter became `---` + paragraphs | Stays out of the tree, comes back verbatim (under `⌘A` too) |
| 5 | `> a` / `> b` became two separate quotes | One two-line quote; `~~~` is opaque |
| 6 | An unknown JSON Canvas key was dropped on save | `#[serde(flatten)] extra` on node and edge; `url`/`background`/`backgroundStyle`/`fromEnd`/`color` typed; integer geometry. **The top-level `Canvas` was missing — fixed in n14** |
| 7 | Undo read the **visible** surface to build the redo | It reads the document the step names — otherwise a note's redo landed in the canvas's slot |
| 8 | Delete was `remove_file` | Moves to the vault's `.trash/`, under a unique name |
| 9 | `label.to_lowercase().find(q)` sliced the original | `match_range` compares char by char; `İ`/`ẞ` no longer take the palette down |
| 10 | `Save` said "saved" even when it refused | It only claims what happened |

Every item has a test. The application-level ones (`src/main.rs`) run the real
`update` over a scratch vault: editing a note with frontmatter, a conflict on
disk, a note that failed to load.

## Deferred, with a trigger

| Deferred | Why | When to do it |
| --- | --- | --- |
| **Crate workspace** (`core`/`app`/`vault-fs`/`desktop`) | 9.7k lines, one binary, one consumer. The boundary that matters is not `Cargo.toml`, it is iced inside the model (`doc::Block` holds a `text_editor::Content`, `vault::Node` holds an `iced::Rectangle`) | At the **first** real consumer. Then the extraction is forced by the compiler instead of guessed |
| **Per-block source ranges** (byte-exact round trip under arbitrary editing) | Today: a file you did not edit is never written, and what the editor does not model comes back verbatim. **The trigger fired in n14 and four cases turned up; three were content loss and were fixed one by one** (see below) | What is left is ordered-list renumbering, which is a design decision. **Correction (audit n16, 2026-08-17): a new case did appear — #34, fences of four or more backticks and a list item's lazy continuation line are corrupted on save. Fixed 2026-08-18 by targeted patches (fences remember their length, continuations stay in their item, plus setext/indented-code guards). Per-block ranges stay deferred: what they would still buy is byte-fidelity for the deliberate style normalizations, not correctness.** **Closed (2026-08-27): ADR-008 made the buffer the file, and the block serializer (`Body::to_markdown`) it would have made byte-exact has been removed — there is no re-emission left to defer.** |
| **Revision by hash + real CAS, filesystem watcher** | mtime+size is check-then-write, not real CAS (#38) — it catches a writer that saved and left (Obsidian), not two live writers at once. Temp files are per-process and fsynced since 2026-08-18; the CAS itself stays here. A watcher is new UI (reload/merge) | When there is an actual concurrent writer — CLI, MCP or server |
| **Path containment (`VaultPath`)** | In local mode the `.canvas` belongs to the vault's own owner: `../` crosses no trust boundary | **Before** any surface that accepts a third party's path (MCP/server). There it is blocking |
| **MCP / CLI / server / versioned protocol** | Does not exist yet; an empty crate is debt | When the use case appears |
| **Collaboration (stage A/B), CRDT** | Needs product semantics that have not been decided | After A: a shared workspace with explicit conflict. CRDT only if it really is simultaneous editing — and then from a library, never hand-written |
| **`SYNAPSO_VAULT` injected instead of a global env in the tests** | The `Mutex` in the harness settles the race and costs 3 lines | If the suite gets slow or the env leaks somewhere else |
| **CI, cargo-deny, LICENSE, SECURITY.md, signing** | The repo is private and ships no binary | The day the code opens — then it is a checklist, not architecture |

## Consequences

- Saving can now **fail on purpose**, and the app has to say so. The unsaved dot
  became real information.
- A note in conflict is not retried on every autosave beat (only on `⌘S` or on
  reload); otherwise the same message would cover the status bar forever.
- `⌘K → reload vault` discards unsaved edits and **says which**.
- Undo stays snapshot-based and stays local. It does not become collaborative
  undo: that one is per actor, and restoring a whole snapshot over someone
  else's work is exactly what must not happen.

## Measured afterwards (2026-08-15, stress vault)

`tools/stress-vault.py` + `SYNAPSO_BENCH=1` produced numbers instead of
opinions. Two things changed status:

| | |
| --- | --- |
| **Canvas drawing** | It was 760ms per frame with 2000 nodes because it drew everything, visible or not. Viewport culling + unreadable text not drawn + the card reading only the top of the note → **7ms**. Done |
| **Loading the vault** | 1.7s and ~700MB for 100 large notes; the whole canvas (1500 nodes, 500 images) takes 20ms and 112MB. The cost is one `text_editor::Content` per block of **every** note, open or not |

The second one was done (2026-08-15): **no editor is built for a note nobody
opened**. It went 1.7s → **~100ms** and 838MB → **~147MB** idle.

The shape matters more than the number. `Doc` now has two views of the same
note: `lines` (blocks as plain text, always present) and `body` (the tree of
editors, built in `Doc::materialize` when the note comes on screen). Everything
that only **reads** a note — search, the canvas cards, ⌘K, ⌘P — moved to
`lines`, so a closed note stays whole on every surface. Both views come from the
same parse, and a test pins the invariant that holds it up:
`Lines::parse(md).blocks == Body::parse(md).lines().blocks`, or a ⌘F hit's block
index would point at the wrong place when the note opens.

*(2026-08-27: the `body` view is gone — ADR-008 made the note one buffer, so
`doc::Lines` is the only derived view and the invariant has nothing left to
compare against. `Body` and its serializer were removed from `doc.rs`.)*

That is the same boundary as the rest of this ADR — separating the model from
iced — only pushed along the cheap path: instead of extracting a crate, what
separates is that **the expensive half of the model is optional**. What remains
is ~30MB per open note, which is iced's `text_editor` and does not go away with
architecture.

## Decided by Weber (2026-08-15)

**Platform: macOS and Windows.** The window gets its own title bar on both
(design: the `macChrome` / `winChrome` blocks). Done now: on the mac the native
bar disappears (`title_hidden` + `titlebar_transparent` +
`fullsize_content_view`), the traffic lights sit on top of our content and the
bar drags the window; off the mac the native decoration stays and the same bar
sits below it. The real Windows prerequisites went in too —
`open`/`explorer`/`xdg-open` in one place, `%USERPROFILE%` when there is no
`HOME`, and reserved names (`con`, `lpt1`, …) diverted in the slug.

Still missing for Windows: the mock's `winChrome` (`decorations: false` +
─ ▢ ✕ buttons drawn by us, red hover on close). That is ~40 lines, but it
**cannot be verified from here** — it waits for the first Windows machine in
hand.

**Multi-user: client + server with server authority.** This screen becomes a
client; the state lives on the server, which shares it with the others. That is
**stage A** — revisioned commands, explicit conflict, event order from the
server. No CRDT: simultaneous editing of the same paragraph is not the
requirement. Deferred for now (his call), and the architecture does not close
the door: the step that matters is the one already listed — extract the model
from iced when the first non-desktop consumer exists, which in this case is the
client-server itself.

## Audit n14 (2026-08-16)

A nightly audit over `82159e6`, 13 findings, all merged as separate PRs with the
test before the fix. Nine were data loss or a crash; the rest came out of a
product decision or of running the app.

**What the ADR got wrong about itself.** Two phase-1 items were marked done and
were not whole: the unknown JSON Canvas key only survived on nodes and edges
(the top-level `Canvas` was erased on save), and the note that "opens marked"
recorded `Doc::broken`/`Scene::broken` and **never drew it** — the only reader
was the save itself. The data was safe; it was the screen that lied.

**The round-trip trigger fired.** Four cases where saving changed the file, all
reachable by opening a file somebody else wrote. Three were content going away
and went in: an outline written with Tabs came back flat, `2020. was a year`
became `1. was a year` (the year **erased**), and a code block lost the blank
lines at its end — that last one was also a silent break of the
`Lines::parse == Body::lines()` invariant. The fourth — a list starting at `3.`
coming back at `1.` — **stays**: the numbering is positional by decision
(`Body::ordinal`), and keeping the source number would mean deleting the first
item of `1. 2. 3.` leaves `2. 3.` on screen.

*(2026-08-27: the serializer these were fixed in no longer exists — ADR-008
saves the buffer verbatim, so saving changes no file at all. The parse half of
each case is still pinned by the `audit_n14_*` tests in `doc.rs`. The fourth
case dissolved: the file keeps the typed number and the sheet counts
positionally on screen.)*

**Windows came off the page.** `split_front` matched the literal `"---\n"`, so a
note written on Windows had its frontmatter read as a rule + paragraphs and
written back that way on the first save. Folded to LF at the edge
(`vault::read`) — and the same fold on paste, which is the other door CRLF comes
in through.

**Saving can now fail on purpose — and a refusal has to stop what comes
after.** The consequence was already written in this ADR; what was missing was
applying it. `create_doc`/rename/delete reloaded the vault from disk without
saving first, and reloaded even when the save was **refused**. Now they stop,
and they say they stopped. The same pattern showed up in rename: refusing the
`fs::rename` was not enough, because the two lines below it named the
destination — the refusal handed over the note it had just protected. **Every
time a refusal path is born, the code after it was assuming success.**

**Search had two heads.** ⌘F folded case in ASCII and ⌘K char by char (item 9
above). Whenever case had to be folded on a non-ASCII letter the two
disagreed: `AÇÃO` did not find `ação`. `find` now uses the `palette`'s form. Measured on the
`tools/stress-vault.py` vault: `find::scan` 4.0 → 9.3 ms per keystroke, inside a
frame.

**What replaced auditing by hand.** The two crashes were the same thing in
different modules — a byte offset landing inside a character. Auditing every
`&s[..i]` by eye does not scale and does not stay done, so a torture test went in
(`the_text_pipeline_survives_the_torture_corpus`) that sweeps every offset of
every block of a markdown corpus that has already broken this parser. Applied
over `82159e6`, **11 of the 20 sources break**.

**Measured and dismissed:** `matched_notes` runs inside `view()` and sweeps the
whole vault, which looked expensive. It is 0.18 ms/frame with 2.4 MB of text.
The two-pass gate (`Doc::plain` before the blocks) already handles it. No change.

## Audit n15 (2026-08-17)

Weber brought three things from real use — a reference between canvas and note,
card text with a line break, and "does opening/closing a document work properly?".
All three live in the same neighbourhood of the code. The answer to the third is
**yes**, even in the ugly case (closing a note in conflict does not lose the
edit: the `Doc` stays in memory and only the tab goes away). But asking it led to
the hole below.

**The mention only reached half the vault.** `[[canvas: X]]` always went to
`focus_node`, which walks the card captions of the **open** board — so a mention
naming a `.canvas` file resolved to nothing, and the mention palette did not even
offer the files (only ⌘K did). On the other side, `attach_ref` forced `.md`: a
board could point at a note, never at another board. The grammar now reuses the
`#` that `[[note#section]]` already used:

| written | does |
| --- | --- |
| `[[canvas: board]]` | opens the board |
| `[[canvas: board#card]]` | opens the board and focuses the card — this is what the palette writes |
| `[[canvas: card]]` | a card on the open board — the old form, kept with a test |

With no `#`, a name that matches a `.canvas` file is read as a board; a board's
name wins over a card caption when they collide. The **cards** of other boards
stay out of the palette — opening every `.canvas` per keystroke is the ceiling
the `ponytail` note in `palette.rs` always named; reaching a card on another
board is two steps.

**A card caption does not fit in a mention.** The canvas got multi-line text on
purpose; a mention cannot have it, because `inline` reads one line at a time.
Referencing a two-line card wrote the caption raw and the file came out with `[[`
on one line and `]]` on the other — a dead link on reopening, and the card's text
lost as far as the note is concerned. `vault::one_line` folds it, and **both**
sides go through it: what is written and what is compared when following.

**The screen lied again, on the board card.** Found by running the app, not in
the tests: the card points correctly on disk and a double-click opens it, but it
drew "missing note" because the preview looks for a *note* by that name. Same
kind as item 1 of this document's table.

### The hole n14 left open

**Refusing to save is not enough if the refusal only exists in memory.** n14 got
the easy half right — do not overwrite the note that changed on disk, and say so
on the status bar. But the bar only tells whoever is looking: with the note in
conflict and the edit only here, `⌘Q` leaves and the text is gone. Nothing on
disk, nothing in `.trash`. Reproduced in the app.

The fix is **not** a flush on exit, and that was the wrong attempt: the test went
green and the app kept losing. Instrumented, `⌘Q` delivers `Unfocused` and
nothing else — `CloseRequested` never arrives. An exit flush is a flush that does
not happen, and a crash or a `kill` fires none either. So the write happens **at
the refusal**: the save's `AlreadyExists` branch writes `<note>-unsaved.md` next
to the note. Nothing deletes that file afterwards — it is plain markdown in the
vault, it shows up in the trail and in ⌘P, and it goes when its owner deletes it,
like `.trash`.

⚠️ The name comes from `vault::unique_path`, **never** a fixed one, and the chosen
path is kept on the `Doc`. `note.md` beside `note-unsaved.md` is an ordinary pair
of names for somebody to already have, and writing the rescue over it would be
this fix causing exactly the loss it exists to prevent. Unique on the first
refusal, stable after — otherwise a minute of typing leaves a trail of
`-unsaved-7.md`. The first version got that wrong and was caught rereading its
own commit.

**The general rule, third time it appears in this ADR:** *every time a refusal
path is born, the code after it was assuming success.* In n14 it was the paths
that reloaded the vault. Here it is the end of the process itself.

### Measured again (2026-08-17)

A confirmation, not a change: `vault.ready` at **113.7 ms** and **149 MB** of RSS
on the stress vault (100 notes, 182 MB) — against the ~100 ms / ~147 MB this
section already recorded after the lazy materialization. An empty vault is
107 MB, so the content costs ~42 MB. Nothing to do.

### What that night's own audit found (32 agents, adversarial verification)

The four PRs above were read by independent reviewers before they reached Weber.
They found **four defects in code written that same night**, all fixed before
delivery — worth recording because three of them are the same kind of error this
ADR has been cataloguing:

| finding | what it was |
| --- | --- |
| **`#` in a caption** | `split_once('#')` ran before checking whether the left half was a board, so the legacy mention `[[canvas: bug #47]]` became board `bug`, resolved to nothing, and the status bar blamed a board nobody named. **A silent regression in an existing vault, on the very path the compatibility test claimed to protect** — it used the one caption without a `#`. Now the whole string is tried as a board first, and the split only counts when the left side resolves |
| **fourth instance of the refusal rule** | `follow_canvas` called `save_scene()` and ignored the bool. Board changed on disk = save refused = scene swapped anyway = every unsaved bit of board work evaporates. There were **four** places that swapped boards like that; three were pre-existing. They all go through `open_board()` now, which saves, checks, and only then swaps |
| **`attach_ref` guessing again** | it threw away the palette's choice and re-derived the target by `stat`. With `roadmap.md` and `roadmap.canvas` in the same vault, picking the NOTE handed back the BOARD, silently. The extension the caller sent is a decision, not a guess |
| **a rescue for a note nobody edited** | `write_checked` refuses on mtime, and a sync client rewriting identical bytes moves the mtime. With no `dirty` guard, switching tabs filled a vault that Obsidian is also watching with `-unsaved.md` files |

The refusal rule is now in **four** places in this document's history. It stopped
being a lesson and became a helper: `open_board` for the canvas,
`save_before_reload` for the notes. While it is prose in an ADR, it gets
relearned.
