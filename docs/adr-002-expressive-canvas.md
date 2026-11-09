# ADR 002 — Expressive canvas: what goes in the file and what is only drawing

**Date:** 2026-08-17 · **Status:** **implemented** (branch
`feat/canvas-expressivo`, eight commits in the order below) · **Context:** a
grilling of a set of requested features (arrow types, group selection, colours,
edge labels, freehand drawing, thickness), checked against the code and against
the JSON Canvas 1.0 spec.

> ⚠️ **Historical.** Written while the shell was iced. Where this names an iced
> type or file, read it as the shape of the problem, not as the code today —
> the cutover landed (`docs/floem-migration-trd.md` §5) and the shell is Floem.
> The decisions still hold; the mechanics moved.

> **Two departures from the plan, decided during implementation and recorded
> here:**
>
> 1. **§D6 — the key is `strokeWidth`, not `width`.** On a node, `width` is the
>    spec's box. D6 asked for "a single key" serving both edges and freehand
>    strokes; `width` cannot be that key, because on a node the name is taken.
>    `strokeWidth` fulfils D6 better than the key D6 named.
> 2. **§D7 — the label draws with no occluders.** D7 said to pass `later` = every
>    visible node. But the chip is drawn in that same third pass and is visible
>    either way: hiding only the text would leave an empty chip. A link's name
>    sits on top, like the arrow itself. `Occlusion` stays on the path — the
>    `MIN_TEXT` cut and the ⌘F painting come from it.

## Decision

Build the features, **but in the order the code imposes, not the order they were
asked in**. Three of the six original requests (arrow colour, arrow name, arrow
type) depend on a primitive that does not exist: **an edge is not selectable**.
`Canvas::hit()` (`vault.rs:600-609`) iterates over `nodes` only.

The corollary is a bug in production today: `Event::DeleteSelected`
(`board.rs:285-291`) calls `canvas.remove(&id)`, which retains edges whose
`from_node`/`to_node` ≠ id. **You cannot delete an arrow.** The only way out is
deleting a node at one of its ends.

The ruler used in every format decision: the spec is the language, an extension
is an accent. An extension that degrades into *presentation* is cheap; one that
degrades into *content loss* has to be accepted with open eyes — and only one of
them is like that (§D5).

## The nine decisions

### D1 — `selected` becomes `Vec<Selection>`

`enum Selection { Node(String), Edge(String) }`. Today it is `Option<String>`
with 18 uses in `board.rs`. `Vec`, not `HashSet`: order matters for restacking
and the selection count is small.

**Rejected:** two fields (`selected_node` + `selected_edge`) — every "is anything
selected?" becomes a `||`, and a marquee catching a node *and* an edge means
keeping two vectors in sync. **Rejected:** `Option<Selection>` now and `Vec`
later — it pays the toll on those 18 call sites twice.

Undo needs nothing: `history.rs:132` compares the whole serialized canvas, so
moving 8 boxes is already one correct step.

### D2 — arrow style is two axes, not one

`elbow`/`curve` answer **where the line goes**; `dashed` answers **what the link
means**. A single enum makes "dashed elbow" unrepresentable.

| key (extension) | values | absent means |
| --- | --- | --- |
| `route` | `curve` \| `elbow` \| `straight` | `curve` |
| `dash` | `true` | solid |

`dash` as a bool: the dash pattern already exists in the repo (`[6.0, 4.0]`, the
Frame's border in `board.rs:1329-1332`). Reuse it.

**`absent = curve` is the interop decision, not the key.** Today `connector()`
(`board.rs:1763`) draws a `Path::line`: a `.canvas` with no extension at all
already renders differently in Synapso and in any other reader. The straight line
is the divergence, not the neutral state. Cost accepted: existing canvases change
their look — they start showing what the other reader was already showing.

**Route is a property of the edge, not a tool on the rail.** A tool explodes:
arrow×line × 3 routes = 6 slots, 12 with dash. The rail has 10 in total.

The spec has no attribute for line style, routing or thickness — an `edge`'s
fields are `id`, `fromNode`, `fromSide`, `fromEnd`, `toNode`, `toSide`, `toEnd`,
`color`, `label`. Verified in the source.

### D3 — the user picks the exit/entry face: the spec's 4

`fromSide`/`toSide` already exist and are already read (`board.rs:985-993`).
Expose, do not invent.

**Rejected:** a free point / 8 anchors. Two arrows on the same face overlap — and
the right answer to that is distributing the N arrows that share a face
(*fan-out*), which is render code, not a file field. A free point solves the
symptom while charging a new key forever.

**Gesture:** a handle at each end of the selected edge (`a` and `b` are already
computed in `board.rs:995`), `Drag::Reconnect { edge, end }`. Dropping on a face
**pins** it; dropping in the node's **middle clears** the side and returns it to
automatic. Without that, a user who brushes a face by accident loses
auto-routing forever with nothing on screen explaining it. The handle draws
differently when pinned.

**Free with it:** dragging an end onto another node reconnects the edge —
impossible today.

**Consequence:** stop recording `from_side`/`to_side` at creation time
(`board.rs:494-496`). Today they are always recorded, so the `unwrap_or_else` to
`facing_side` in `board.rs:987` is **dead code** and the side is never
recalculated: move a node and the arrow keeps leaving by the original face. With
a straight line that is cosmetic; with an **elbow it is grotesque** — it exits
right, goes around and enters from behind. The elbow does not create the bug, it
makes it impossible to ignore.

Edges already on disk have explicit sides and are therefore born *pinned*. **Do
not rewrite the file on load** to fix that — it is the sin `Node.extra`
(`vault.rs:394-398`) exists to prevent. The user drags to the middle if they want
it released.

**Render:** control point = `side_point + face_normal * k`, with `k` proportional
to the distance between the ends. That is what produces the mock's S instead of a
crooked arc. It holds for the elbow too: the first and last segments run
perpendicular to the faces.

### D4 — colour: write hex, read hex **and** the 6 presets

**Do not reuse `DocColor`** (`vault.rs:187-192`). Three independent reasons:

1. It is `#[serde(untagged)]`, so `Preset(2)` serializes as the **number** `2`
   (confirmed in `README.md:82`: `"note": 2`). JSON Canvas's `color` is a
   **string**. Reusing it writes `"color": 2`, a type violation.
2. The indices do not line up and `+1` does not fix it: you have 4 presets 0..3
   (`theme.rs:109`), the spec has 6 in `"1".."6"` with fixed meanings (1 red,
   2 orange, 3 yellow, 4 green, 5 cyan, 6 purple). `Preset(0)` → `"0"` **is not
   a valid preset**; and even shifted, violet→yellow and cyan→green come out
   wrong. Half of them break.
3. On top of that: your presets change with the theme (`theme.rs:109` vs `:145`);
   the spec's do not. Writing an index means writing "slot N of my theme".

**Asymmetry on purpose:** written hex is unambiguous, theme-independent and
round-trips perfectly. Reading presets **is not optional** —
`color::from_hex("3")` (`color.rs:78`) returns `None`, so without a 6-arm `match`
a canvas coloured in Obsidian opens with no colour at all.

New code ≈ zero: `Node.color`/`Edge.color` are already `Option<String>`,
`to_hex`/`from_hex` already exist, and the whole picker
(`Field`/`Rail`/`Wheel`/`Pick`, wired in `main.rs:1638-1670`) is already built as
a modal. It is pointing the note-colour UI at another target.

**Render:** stroke in the full colour + fill in the same colour at alpha `0.35`.
A pattern already used twice in the repo (Frame in `board.rs:1317-1335`, search
highlight at `:1870`). Alpha over the surface keeps text readable in both themes;
a solid fill does not, with coral `#E8737F`.

**Consequence:** `s_doc_color` (`board.rs:1903-1906`) picks a colour by
`node.id.parse() % 4`. It starts lying — minimap and board disagreeing about the
same node. It has to consult `node.color`, falling back to the module.

### D5 — freehand drawing: an extension node with a legal skeleton

⚠️ **The only extension whose loss destroys content rather than appearance.**
`shape` lost → still a node with its text. `route` lost → still an arrow. `draw`
lost → **an empty box where the drawing was**.

The spec's node types are enumerated — `text`, `file`, `link`, `group`. A
`"type": "draw"` is invalid, not an extension. So: `"type": "text"`,
`x/y/width/height` = the stroke's bbox, `"draw": [[x,y],…]`.

**The text has to be non-empty.** `Node.text` is
`#[serde(skip_serializing_if = "String::is_empty")]` (`vault.rs:355-356`) and
`text` is *required* on a `text` node. Empty text disappears from the JSON and
the node is born illegal. `"(drawing)"` or equivalent — what looked like a
flourish is a requirement.

**Rejected:** a sidecar like `.synapso.json`. There is precedent
(`README.md:78-84`), but the colour sidecar holds one integer per note; geometry
needs rename, delete, copying between vaults and a new failure mode (board and
sidecar disagreeing). And copying just the `.canvas` to another folder would
erase the drawing **silently** — worse than the risk it came to avoid.

Deserves a `ponytail:` naming the ceiling: *the stroke lives in a key only
Synapso promises to preserve; a round trip through another editor may hand back
an empty box.*

It fits the `P` key — `Icon::Pen` (`icons.rs:97-100`, a bezier glyph) has been on
the rail since forever and does nothing (`board.rs:330`: *"the not-yet-built
pen"*).

### D6 — thickness: a number in the file, 3 options in the UI

`"width": <px>`, not `"weight": "medium"`. Restrict the interface, not the
format — the same line of code, without locking anything in. A slider was
rejected: it hands back `1.7314` and two arrows nobody matches again. **One key**,
serving both edges and freehand strokes.

**What thickness blows up:** `connector()` does `.with_width(1.5)`
(`board.rs:1764`) — a constant in **screen space**, with no `* z`, while
everything else scales explicitly (`:1340`, `:1342`). On a 1.5px thread nobody
notices; on a 5px arrow, the diagram shrinks on zoom out and the arrow stays at
5px. And the head (`board.rs:1776-1782`) uses hardcoded `8.0`/`4.0` — it scales
with neither zoom **nor** thickness: a 5px arrow ending in a pin.

**Fixed along the way:** `(width * z).max(1.0)` on the stroke (the floor keeps it
from vanishing on zoom out) and a head derived from the thickness (~`5w` long,
~`2.5w` half-width), scaled too. Done together it is 4 lines; done later it is
hunting for why "the arrows get fat when I zoom out".

### D7 — edge label: its own pass, after the nodes

iced keeps a `Frame`'s text in a separate batch painted **above every fill** — it
is documented in `board.rs:1007-1013`, with *"and no amount of geometry ordering
changes that"*, and it is why `Occlusion` (`board.rs:1157`) exists. Edges draw
**before** the nodes (`board.rs:976-1005`), so a label emitted there crosses
whatever box covers it.

And a label over the line with no background is unreadable; but a chip is a mesh,
and a mesh in the edge pass sits **below** the nodes — chip below and text above
is the worst of both.

**A third pass, after the nodes, via `Occlusion`** with `later` = every visible
node. It gets the `MIN_TEXT` cut (`board.rs:1170`) and the ⌘F painting (`:1177`)
for free — *"the one place ⌘F has to paint"*. A label that does not go through
there is a label search cannot find.

**One geometry function, three consumers:**
`edge_path(from_rect, to_rect, route) -> (Path, Vec<Point>)`, returning the
drawable and the sampled polyline. It serves the edge's hit-test (D1), the
label's anchor (here) and the culling bbox (D8). "The middle of the edge" is not
the middle of the segment when the route is a curve: it is `t=0.5` of the bezier,
and on an elbow the middle of the longest segment.

### D8 — culling: an expanded bbox, no sampling

`board.rs:997-1003` builds the box of the **two ends**. An elbow is safe (an L
fits inside the rectangle of its diagonal). **A curve is not**: the controls come
out perpendicular to the face and the bezier bursts the chord's bbox — the arrow
disappears at certain scroll angles.

`bbox(a,b).expand(k)`, `k` = the control's maximum displacement. Two lines,
conservative by construction. Do **not** use D7's polyline here: sampling every
edge each frame to decide whether it is worth drawing is work to avoid work.
Sample only what got through.

### D9 — marquee takes left-click-on-empty

`board.rs:375-379`: with Select, clicking on empty space **already starts
`Drag::Pan`**. That is exactly the gesture the marquee needs; you cannot have
both.

Panning loses nothing: `Event::PanPressed` (`board.rs:117-118`) already exists as
a first-class event for space+drag and the middle button. The empty-drag is the
*fallback*, and it is the fallback that is in the way. **Rejected:** the marquee
behind a modifier — that buries the frequent action and leaves the rare one on
the bare gesture. Cost accepted: muscle memory breaks once. It is what
Figma/Excalidraw/Miro do.

**What the marquee catches:** any node it intersects (freehand drawings included,
which are nodes per D5); an edge **only if both ends** are inside — an edge has
no position of its own, half a link is only good for styling, and moving it means
nothing.

## What starts going into the file

| field | origin | written by us | lost in another reader |
| --- | --- | --- | --- |
| `Node.color`, `Edge.color` | **spec** | hex always; presets `"1".."6"` read only | nothing |
| `Edge.label` | **spec** | yes | nothing |
| `fromSide`/`toSide` | **spec** | only when the user pins them | nothing |
| `Edge.route` | extension | when ≠ curve | becomes the other reader's route |
| `Edge.dash` | extension | when true | becomes a solid line |
| `strokeWidth` (edge and node) | extension | when ≠ default | becomes the default thickness |
| `Node.draw` | extension | always on a freehand stroke | ⚠️ **an empty box** |
| `fromAt`/`toAt` (2026-10-28) | extension | when a pinned end was dropped off its side's middle | the end goes back to the middle of that side |
| `Edge.bend` (2026-10-28) | extension | when the line was dragged by its middle | a line straight between its ends |
| `"shape": "point"` (2026-10-28) | extension | a loose arrow end: a 12px `text` node whose text is `·` | a tiny card with a dot, where the arrow ends |
| `fontFamily` (2026-10-28) | extension | `"mono"` when the item is in the app's face | the reader's own face |

Proved by a round trip with the app open over a hand-written `.canvas`: every key
above came back, plus the top-level `metadata` and the pinned
`fromSide`/`toSide`. 113 tests green, `clippy --all-targets` clean.

## Execution order

| # | PR | Delivers | Why here |
| --- | --- | --- | --- |
| 1 | selection foundation | `Vec<Selection>`, edge hit-test and edge delete | Invisible to the user; without it nothing from 3–8 has a door. Already pays off a bug we have today |
| 2 | marquee | group selection + moving in bulk | The biggest visible gain, and it stresses PR-1 immediately |
| 3 | route | `edge_path()`, `route`, `dash`, culling, stop recording sides | First one that touches the format; the foundation is proved by then |
| 4 | pinnable faces | end handles, `Drag::Reconnect`, reconnecting | Depends on 3 (the geometry) |
| 5 | colour | hex written, presets read, stroke+`0.35`, `s_doc_color` | Independent; the picker already exists |
| 6 | label | post-node pass via `Occlusion` | Depends on 3 (the label's anchor) |
| 7 | thickness | the `width` key, zoom scaling, derived head | Depends on 3 and 4 (same surface) |
| 8 | freehand | the `P` key, `Node.draw` | Biggest format risk, so last |

## Punch list — found during the grilling, out of scope

| Severity | What | State |
| --- | --- | --- |
| **high** | A `text` node with no `text` field is **already on disk**: a shape created and left unnamed serializes without the key (`skip_serializing_if`), and `text` is *required* by the spec | **OPEN.** Out of scope for these eight PRs, and D5 only works around it: freehand writes `"(drawing)"` so as not to fall into the same hole. The fix needs serialization conditional on `type`, which serde does not do by attribute |
| medium | An edge cannot be deleted — `Canvas::remove` only matches node ids | ✅ PR-1 |
| medium | `facing_side` in the draw is dead code: sides are always recorded at creation | ✅ PR-1 (pulled forward from PR-3: the fallback already existed, so it was one line) |
| low | A frame does not carry its children when moved | **OPEN.** `Drag::Move` now carries the whole selection, which gives the manual way out — marquee over the frame and its children. The frame still adopts nobody |
| low | `Occlusion::text` tests `n.rect().contains(world)` without applying the occluder's rotation | **OPEN** |
| low | The `P` key has had no function since forever | ✅ PR-8 |
| — | A shape's border with a hardcoded `1.5`, no `* z` — the same bug as the edge, found in PR-7 | ✅ PR-8 (the node's `strokeWidth`) |
| trigger | Fan-out of N edges sharing one face | Has not fired |
