# ADR 007 — Reading the syntax document

**Date:** 2026-08-18 · **Status:** accepted, implemented · **Context:** Weber
handed over John Gruber's *Markdown: Syntax* — the document markdown is
specified in — and asked what does not render. The answer came from the parser,
not from reading: `src/fixtures/markdown-syntax.md` is now in the repo and
`the_markdown_syntax_document_reads_and_settles` is the test.

> Numbering note: ADR-006 (tables) is open in #63 and lands first.

## What the probe found

Blocks the document is *about*, that this editor was not reading:

| in the document | was | is |
| --- | --- | --- |
| `    This is a code block.` ×3 | a paragraph with four spaces in front | `Kind::Code`, indented |
| `----` under the note | a paragraph | `Kind::Rule` |
| ``` `` ` `` ``` | one backtick paired with the next | one code span holding a backtick |
| `>` on an empty quote line | written back as `> ` | written back as `>` |

## The three decisions

### D1 — an empty fence is the indented code block

`Kind::Code { lang, fence }` already carried the *exact* opening fence, because
a ```` block has to close on four backticks (#34). Markdown's other code block
has no fence at all — so **`fence: ""` is the indented one**, and serialization
writes four spaces on every line instead of a closer. No new variant, no new
match arm anywhere outside `doc.rs`: the editor already boxes it, highlights
it, keeps Enter as a line break and Tab as a tab.

### D2 — an indented run is code only when it stands alone

Blank line (or the file's edge) above **and** below, and the block before it is
not a list item. Anything else keeps the verbatim-paragraph behaviour it has
had since #34.

Both halves are load-bearing:

- **Words up against the run** (`    ```\nnot a fence`) would have to gain a
  blank line to come back as two blocks — the editor would be inventing a line
  the writer did not type. It stays one verbatim paragraph.
- **Right under a list item**, four spaces is markdown's *second paragraph of
  that item* — prose, not code. The document has both, four lines apart, and
  reading them the same way would have turned a sentence into a code sample.

### D3 — the document is held to settling, not to being untouched

A block this editor models is a block it normalizes. The document comes back
with `*` and `+` written as `-`, ordinals counted from one, `----` written as
`---`, a lazily-quoted paragraph carrying its `>` on every line, and hanging
indents at this editor's two spaces. None of that changes what any reader
renders.

So the corpus test asserts three things instead of equality:

1. **the reading** — the counts of each kind the document must produce;
2. **stability** — one pass and the file stops moving (`parse(save(x))` is a
   fixed point). This is what caught the empty quote line: dropping its `>`
   split one blockquote into two on the way back;
3. **no lost words** — every alphabetic token in equals every token out.

ADR-001's promise is untouched and now has its own test: **opening a note
rewrites nothing.** The normalizations above only ever reach the file through
an edit the writer made.

## Still not right, and why

- **Block structure inside a blockquote.** `> ## header`, `> 1. item`,
  `>     code` come back as raw text inside one quote. `Body` is a flat
  `Vec<Block>`; a quote that *contains* blocks needs a recursive tree, which is
  a different editor.
- **Nested `> >`.** Depth would fit in the block's `indent` — but two quotes at
  different depths and two quotes separated by a blank line are the same thing
  on the way out, so the model would need a "blank line before me" bit to
  serialize without merging separate quotes into one. Not worth that bit today.
- **A list item's own second paragraph and code block** stay flat blocks beside
  the item rather than inside it — same missing tree as above.
