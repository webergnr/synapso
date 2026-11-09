# ADR 004 — The board's style bar

**Date:** 2026-08-17 · **Status:** accepted, implemented · **Context:**
grilling of the request "a mini menu when I click a node", with three design
references from app.eraser.io (square node, arrow, text node), checked against
what the `.canvas` already holds.

## The rule inherited

ADR-002 settled it: **the spec is the language, an extension is an accent.** It
survived nine decisions and it still holds here. A design reference is not a
reason to invent format.

## What the reference asks for × what the file can carry

| control in the reference | state |
| --- | --- |
| shape, colour, thickness, line style | already keys in the file |
| route (Elbow/Straight) | `route` — and we also have curve |
| end arrowhead `→` | `toEnd` |
| start arrowhead `←` | `fromEnd` exists and is preserved, **never drawn** |
| text size | `fontSize` |
| bold | `fontWeight` — Synapso extension |
| **Text / Code** | does not exist |
| **Rough / Clean / Mono** | the app has one font, by design |
| **alignment** | does not exist |

## The four decisions

### D1 — the bar serves what the file already holds

The bar is a **surface**; the surface is the work. A new control later is one
row. Copying a screenshot is not a reason to invent a key.

**Rejected:** bringing `Text/Code`, font family and alignment along — three new
keys, a second font in the binary and `draw_node` changing layout, all so the
bar looks like one belonging to a product whose entire job is diagrams. Here a
node's caption is one line of mono **by design**; the single font comes from the
design system, it is not a gap.

**Weber's carve-out: alignment is in.** It is the cheapest of the three, by a
happy accident: the alignment maths already exists. `text_box()` — written for
the occlusion fix — already handles `Left/Center/Right`, and so does the ⌘F
highlight. What is missing is one key and one `match` in `draw_node`.

**Weber's second carve-out (2026-08-18): bold is in.** It uses the bold face
the binary already ships, so it adds no font family and no layout mode. Absent
still means regular. Synapso writes `fontWeight: "bold"`, reads the common
string and numeric bold forms, and preserves any value it does not understand.

**Freebie:** `←` is already in the file, just never drawn. Nearly free, and it
makes the arrow bar match the reference.

**A consequence that needed no round of its own:** the reference has three line
styles; `dash` is a boolean (ADR-002 §D2). It stays a boolean — solid and
dashed.

### D2 — an absent `align` means what happens today

Shapes centre their caption, a `label` is left-aligned. No existing file has the
key, and none of them changes.

**Rejected: absent = left**, "the way Obsidian shows a text node".

⚠️ This contradicts ADR-002 §D2 on purpose, and the difference is the point.
There, `absent = curve` was chosen **against** what the app did, to match other
readers — worth it because the line is the whole identity of an edge.

Here there is nothing to match: Obsidian **does not draw the box**. `shape` is
our extension; to it there is a text node and nothing else. There is no "the
alignment another reader gives my shape". Absent = ours.

### D3 — the bar takes styling, the menu keeps the rest

| | |
| --- | --- |
| **bar** | colour, shape, route, dash, thickness, alignment, bold, arrowheads |
| **right-click menu** | bring to front, send to back, duplicate, copy, doc reference, rename, delete |

**Rejected:** both carrying everything. This is not taste, it is the failure
mode: in three months a new control lands and one of the two is forgotten — and
then the feature exists if you right-click and does not exist if you use the
bar. It does not show up in testing; it shows up in use, on a bad day.

Colour does not become harder to reach: the bar appears **the moment something
is selected**, which is less work than a right-click, not more.

### D4 — on a multi-selection, the bar shows the intersection

A shape understands colour, shape, thickness, alignment, bold. An arrow understands
colour, route, dash, thickness, arrowheads. Selected together, the bar shows
**colour and thickness** and nothing else.

**Rejected:** showing everything and letting what does not apply sit there
inert. A control that is visible has to work: an "elbow" showing with a box
selected either lies (does nothing) or surprises (does something nobody asked
for).

**Free:** the code already behaves this way — thickness applies to nodes and
edges, route only to edges (`menu_action`, ADR-002 §PR-3/PR-7). The bar is
showing what the code already does.

## Amended 2026-08-19 — after the first real use of the board

### D5 — bare text is not a box, and the bar stops pretending it is

A `label` node is words on the board with nothing around them. Two of the bar's
groups are about the box and not the words: **shape** (rect/ellipse/triangle)
and **thickness** — a border it does not have. Selected on its own, a label gets
colour, alignment and bold, and nothing else.

This is D4's rule applied to one node instead of many: *a control that is
visible has to work.* Thickness on a label wrote `strokeWidth` into the file and
changed nothing on screen, which is the "lies (does nothing)" half of D4,
reached without needing a multi-selection at all.

**Accepted cost:** the trip is one-way. A shape can still be flattened to a
label (the `T` key is in the shape group), but a label can no longer be given a
box back from the bar — draw the shape and type in it. Re-shaping a label was
never the point of the group; describing the box you are looking at was.

**Amended 2026-10-20:** the trip is closed both ways. The `T` key left the
shape group and `Scene::set_shape` refuses to turn a shape into a label or a
label into a shape, mixed selections included — a box flattened by one stray
key lost its look, and nobody reached for it on purpose. A label is typed on
the board; a shape is drawn.

### D6 — ⏎ ends the caption, ⇧⏎ is the line break

The editor over a node was a paragraph field: ⏎ broke the line and ⌘⏎ was the
way out. In use it is the other way around — a node's caption is a name, and
typing one is the only reason the field opens. So ⏎ commits, ⇧⏎ puts a line
into the caption for the times it really is two lines, and ⌘⏎ still commits.

### D7 — bare text is edited where it stands

A `label` being edited no longer wears a text field. The editor is transparent,
in the node's own colour and size at the current zoom, over the node's own box
— so the words do not move when you click them and do not change size when you
stop typing. The focus effect is the ring around the whole box, which is what
the field's border used to say. A **shape** keeps its field: the box is already
drawn on screen, and the field is what says the words belong to the box and not
to the board.

⚠️ Known ceiling: `text_editor` has no horizontal alignment, so a centred or
right-aligned label edits from the left and settles back on commit. The size,
the colour and the vertical position agree; the horizontal one does not.

## Assumed, without a round

- **Bar fixed at the bottom**, like the reference and like the request ("below
  the canvas"). It does not follow the node: a target that moves while you drag,
  and jumps at the edges of the pane, is the worst target there is — and zoom
  already makes the node small.
- **No selection, no bar.**

## Implementation risk, named

Stacking the bar over the canvas is a pattern already used here (context menu,
inline editor, minimap, tool rail) — no risk.

⚠️ What is new: the bar's **dropdowns** (colour grid, route list) are an overlay
over an overlay. The app has the colour modal and the context menu to reuse, but
anchoring a popover to a button on the bar does not exist yet. That is where the
time will go.
