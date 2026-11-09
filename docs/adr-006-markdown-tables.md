# ADR 006 — Markdown tables

**Date:** 2026-08-18 · **Status:** accepted, implemented · **Context:** the
request "can tables in markdown work in synapso?", answered against what the
editor already is — a live-markdown editor, not a renderer with a source mode.

> ⚠️ **Historical.** Written while the shell was iced. Where this names an iced
> type or file, read it as the shape of the problem, not as the code today —
> the cutover landed (`docs/floem-migration-trd.md` §5) and the shell is Floem.
> The decisions still hold; the mechanics moved.

## The rule inherited

`doc.rs` opens with it: *markdown is the input language and the file format —
never the thing on screen.* A heading eats its `##` because the block can carry
`Heading(2)` instead. Inline emphasis cannot do that — iced paints one
`text_editor` with one font and a [`Highlighter`] that changes colour and font
and nothing else — so `**bold**` keeps its markers **and fades them**, and the
words take the weight. That trade is already made, shipped, and lived with.

A table is the second thing that cannot give up its syntax. It gets the same
deal.

## The fork

| | A — the source, aligned | B — a drawn grid |
| --- | --- | --- |
| on screen | pipes padded into columns, faded; header in weight; a rule round the block | real borders; raw pipes only while the caret is inside |
| the widget | the one `text_editor` every other block is | a grid when idle, an editor when focused |
| ⌘F, ⌘A, undo, find-marks | keep working, untouched | stop seeing a table that is not an editor |
| the trap | none new | `block_view` documents it: iced keeps widget state **by position in the tree**, so a block that changes shape hands its editor fresh state and drops the keyboard |

**Chosen: A.** Every font in this app is JetBrains Mono. Padded columns in a
monospace font *are* a table — B buys borders and pays for them with the one
footgun this file already warns about, plus a second copy of every overlay
feature. Weber picked A with both mockups in front of him.

## The four decisions

### D1 — the padding is the rendering, so the padding is the format

`Kind::Table` holds the rows verbatim, pipes and all, and `to_markdown` writes
them back untouched. What makes it a table on screen is that the cells are
padded to their widest — which is also what every markdown formatter writes.
The file stays canonical GFM; nothing here is a Synapso extension.

`:---:` in the divider is honoured **as padding**, because here the padded text
*is* the rendered cell.

### D2 — realign when the caret leaves a cell, never on a keystroke

Padding that moves under the caret jitters, and the caret then has to be put
back by hand on every letter. So the snap happens on Tab, on Enter, and when
the note is parsed — org-mode's contract, for org-mode's reason.

The cost is honest and visible: while you type, the cell you are in grows out
of line. It snaps when you leave.

### D3 — a block this editor models is a block it normalizes

Opening a note aligns its tables, so a save after any edit writes them aligned.
That is not new: this editor already renumbers `1.` positionally, and already
puts the blank line markdown needs between blocks. `editing_a_note_leaves_the_rest_of_the_file_alone`
used to hold a ragged table as its example of markdown we have no block for —
that example is now wrong, and a setext heading took its place.

The promise underneath it (ADR-001) is unchanged: **a file is only ever
rewritten by an edit the writer made.** Opening a note with a ragged table and
closing it changes nothing on disk.

### D4 — a table is two lines before it is one

A row alone is a paragraph. A table starts where a row is followed by a divider
with **the same number of columns** — what GFM demands, so a thing drawn here
as a table is a thing GitHub renders as a table. Leniency would have been a
table on screen and a paragraph everywhere else.

Two ways in, both already the way in for something else:

- **`| a | b |` + Enter** builds the divider and an empty row under it — the
  same move ```` ``` ```` + Enter makes for a fence.
- **paste**: `parse_blocks` is the reading, and paste has used it since the
  markdown-paste work, so a table copied off GitHub arrives as one for free.
- (and `insert table` in the editor's context menu, for the person who does not
  know either.)

## Assumed, without a round

- **Tab grows the table.** Off the last cell of the last row it adds a row
  rather than doing nothing — the fastest way anyone fills a table in.
- **Enter on an empty last row leaves the table**, exactly as an empty bullet
  leaves a list.
- **Tab steps over the divider** in both directions. Nobody wants a caret in a
  run of dashes.
- **No column insert/delete commands.** Typing a cell is what adds a column;
  the rest is a menu nobody asked for yet.

## What the second engine found

A review by codex (`codex exec review --base main`) — the first one run here
after the Copilot credit ran out — turned up five, all real, one of them a
reachable panic. Two of them settled decisions this ADR had not made:

- **A table is a thing that can stop being one.** Editing is free to delete
  the divider out from under a `Kind::Table`; everything that walks a grid asks
  `table::is_whole` first. Enter turns what is left back into the paragraph it
  looks like; Tab walks away. The alternative — rebuilding the missing rows —
  would put back syntax the writer had just deleted on purpose.
- **A table under a list item is that item's.** The depth rides on the block
  and is written back on every row. Without it, an unrelated save moved the
  table out of the list — a rewrite of the kind ADR-001 exists to prevent.

## Known ceilings

- **A character is a column.** True for the ASCII the syntax is made of and for
  the accented Latin anyone here writes; a CJK or emoji cell lands a column off,
  because those are two terminal columns wide and one `char`. Fixable with a
  width table the day it bites.
- **`Motion::Right` is how the caret is placed** (one press per character), the
  same idiom `⌘F` already uses to land on a match.
- **The two units.** iced reports the cursor's `column` as a **byte** offset
  (cosmic-text's `Cursor::index`) and moves the caret by **characters**. Both
  are used here, one line apart, and `table.rs` says which is which at every
  boundary.
