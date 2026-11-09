# ADR 003 — The quote on the board: a photo, not a window

**Date:** 2026-08-17 · **Status:** accepted, implemented (PR #31) · **Context:**
grilling of the request "if I change the note, the reference on the board
should change with it", checked against the code.

## What the grilling found before deciding anything

**Live references already exist — in two of the three kinds.** In `board.rs`,
a card's body comes from:

```rust
let (body, meta) = match (&node.excerpt, doc) {
    (Some(excerpt), _) => (excerpt.clone(), …)   // frozen
    (None, Some(doc)) => {                        // reads the note NOW
        doc.lines.section(s, …)  or  doc.lines.body_preview(…)
```

Whole-note card: live. Section card: live. Both re-read the note on every
draw. Only the **quote** keeps a copy.

And the asymmetry goes further: the section uses `subpath`, a **spec** field
other readers understand; the quote uses `excerpt`, an extension of ours. The
one kind that does not follow is the one kind that opted out of the mechanism
that already worked — and the original proposal was to build a **third**
mechanism to fix exactly that one.

The real gap survives the finding: a section is heading-sized. A sentence is
not. The quote does buy something; the question was what it **is**.

## The three decisions

### D1 — the quote is a PHOTO

The slice of the note as it was when it was sent to the board. It never
changes on its own.

**Rejected: a window** (the slice as the note reads now). Three reasons, in
order of weight:

1. **The cost would land in someone else's file.** An anchor that survives
   editing needs a marker inside the `.md`. The original request said "never
   show it to the user", and that **cannot be honoured**: the vault is a plain
   folder, Obsidian opens the same folder, `git diff` shows it. Invisible would
   only hold inside Synapso — and the rest of the world is the whole reason the
   format is plain Markdown. A marker is forever; it outlives uninstalling the
   app.
2. **A window has a failure mode a photo does not:** the quote changes with
   nobody asking. You draw a diagram around a sentence, somebody edits the
   note, and the diagram now says something else. A photo is only ever wrong in
   one direction — it goes stale, and you can see that it is stale.
3. **The window already exists.** Anyone who wants it to follow references the
   **section**: live, spec, costs nothing new.

If the window ever comes back to the table, the marker is the `^id` Obsidian
already uses — small and visible — never an invisible one of ours.

### D2 — the photo says when it went stale

A photo you cannot tell is wrong is worse than one that is right. If the slice
is no longer in the note, the card says so.

**Where it is computed is half the decision.** On the 600 ms heartbeat — the
same funnel that already invalidates the card — and **only when the note's
revision moved**. Idle costs nothing. Never per frame: drawing happens sixty
times a second and this board has already gone from 760 ms per frame once;
searching a note per card per frame is that same mistake in new clothes.

`Doc` gains a revision, bumped in `Doc::touched` — the one place every change
to a note passes through. The verdict lives on the `Scene`, in memory: "this
went stale" is not data, it is an observation *about* two pieces of data, and
on disk it would get a chance to be wrong.

**Rejected: comparing a hash of the note** (O(1) per card). It would flag stale
whenever *any* part of the note changed — edit a paragraph further down and
every card lights up. An alarm that always rings is an alarm switched off.

**Assumed:** only the open board is checked. Knowing about cards in a closed
`.canvas` would mean opening all of them on every keystroke.

⚠️ **Known ceiling:** while you type, that is one search per quote card every
600 ms. With a wall of cards over a large note it costs. It deserves a
`ponytail:` naming the ceiling and the way out (index by note, or check only
what is on screen).

### D3 — you can ask for a new photo

A "refresh" row on the card. Nearly free: `add_excerpt` exists, the node's
context menu exists, and the "it is stale" from D2 is what makes it necessary —
without that the button would be superfluous; with it, not having one would be
showing the problem and hiding the answer.

⚠️ **Refresh is not "fetch the same slice again"** — the old slice is gone,
which is *why* the card is stale. It is **fetch from the current selection**:
select the new text in the note and the card quotes that.

## The result worth recording

**Zero format change.** The request opened with "hidden metadata in the
Markdown". The three decisions together need nothing new in any file:
`excerpt` is already the photo, "stale" is memory, refresh reuses what exists.

## Ceilings accepted

| What | Why it is fine |
| --- | --- |
| Rewriting a note leaves N cards stale, one refresh each | D2 makes you **see** it; without it the same damage was silent |
| A quote does not follow its note | That is the decision, not a defect. Whoever wants following uses a section |
| One search per card per 600 ms while typing | Gated on the revision; idle is zero. Index it if it ever costs |
