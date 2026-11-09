# Design frames — Claude Design export → static HTML + reference screenshots

Source: fresh `.dc.html` export (`Workspace.dc.html`, `Notes Canvas Workspace.dc.html`,
`Config.dc.html`, `tokens.css`, `support.js`). One frame/artboard per file turned out not to
apply literally — see "How the files relate" below.

## How the files relate

`.dc.html` is a small component format (dc-runtime), not plain HTML: a `<helmet>` (head
content), an `<x-dc>` template, and a `<script data-dc-script>` holding a
`class Component extends DCLogic { renderVals() {...} }` whose return value feeds the template.

- **`Workspace.dc.html`** and **`Config.dc.html`** are each *one* component with a prop schema
  (`data-props`: theme, chrome, view, palette, picker, rotate, empty, toast, search,
  searchScope for Workspace; theme, section for Config). Opened alone, each renders its
  **default** prop state — one frame each.
- **`Notes Canvas Workspace.dc.html`** is a spec-sheet document (`design_doc_mode: canvas`)
  that doesn't render app UI directly. It uses `<dc-import name="Workspace" theme="dark"
  toast="{{ true }}" ...>` tags to pull in *other* components with specific prop overrides,
  each wrapped in a `<section data-screen-label="...">` caption — this is how the format
  expresses "multiple frames in one file." It documents 12 UI-state frames this way, plus two
  pure reference sections (06 component specs, 07 token reference — color/type/spacing/radius/
  shadow/z-index tables) that aren't `dc-import` frames, so they weren't screenshotted
  individually (they're plain documentation, not a screen to implement against).

## Frames rendered

| # | Frame | File → PNG | Size (CSS px / actual @2x) |
|---|---|---|---|
| — | Workspace, default props | `Workspace-default.png` | 1600×940 / 3200×1880 |
| 00a | Config — dark, editor section | `Notes Canvas Workspace-00-config-1.png` | 1100×720 / 2200×1440 |
| 00b | Config — light, storage section | `Notes Canvas Workspace-00-config-2.png` | 1100×720 / 2200×1440 |
| 01 | Workspace — dark (default) + toast | `Notes Canvas Workspace-01-dark-workspace.png` | 1600×940 / 3200×1880 |
| 02 | Workspace — light | `Notes Canvas Workspace-02-light-workspace.png` | 1600×940 / 3200×1880 |
| 03 | Command palette (⌘K) open | `Notes Canvas Workspace-03-command-palette.png` | 1600×940 / 3200×1880 |
| 03b | Document picker (⌘P) open | `Notes Canvas Workspace-03b-document-picker.png` | 1600×940 / 3200×1880 |
| 04 | First-launch empty state | `Notes Canvas Workspace-04-empty-state.png` | 1600×940 / 3200×1880 |
| 05a | Notes-only view | `Notes Canvas Workspace-05a-notes-only.png` | 1600×940 / 3200×1880 |
| 05b | Canvas-only view | `Notes Canvas Workspace-05b-canvas-only.png` | 1600×940 / 3200×1880 |
| 05c | Global search ("auth", all scope) | `Notes Canvas Workspace-05c-global-search.png` | 1600×940 / 3200×1880 |
| 05d | Search scoped to notes | `Notes Canvas Workspace-05d-search-scope.png` | 1600×940 / 3200×1880 |
| 05e | Canvas node rotation (-12°) | `Notes Canvas Workspace-05e-node-rotation.png` | 1600×940 / 3200×1880 |

All 13 rendered at 2x device scale via headless Chrome, visually confirmed (real UI, no
`{{ }}`/`<sc-if>` leftovers, no blank pages). Section 06 (component specs) and 07 (token
reference) were compiled into `Notes Canvas Workspace.html` (the full static page) but not
split into individual PNGs — they're reference tables, not implementable screens.

## Files in this directory

- `build.mjs` — the compiler. `node build.mjs <in.dc.html> <out.html>` compiles one `.dc.html`
  into a dependency-free static page (tokens.css inlined, all control flow resolved). Run
  against all three source files to regenerate `Config.html`, `Workspace.html`, and
  `Notes Canvas Workspace.html` (the last one has every `dc-import` inlined, so it's a single
  scrollable page showing all 13+2 documented states at once — good for a quick full review).
- `shots.mjs` — `node shots.mjs <ds-dir> <out-dir>` regenerates every per-frame HTML + PNG
  listed above, using `build.mjs`'s compiler as a library.
- `Config.html`, `Workspace.html`, `Notes Canvas Workspace.html` — the three full compiled
  pages (deliverable #1).
- `*.png` / matching `*.html` — the 13 individual frames (deliverable #2), each a clean,
  isolated render at its native size (no spec-sheet caption/box-shadow chrome around it).

## `.dc.html` / `support.js` syntax handled by the compiler

- `<helmet>...</helmet>` — hoisted into `<head>`, `tokens.css` link replaced with inlined CSS.
- `<script data-dc-script>` — a `DCLogic` subclass; evaluated with `new Function('DCLogic',
  'props', script + 'return new Component(props).renderVals()')` against a stub base class
  (`constructor(props){this.props=props}`, no-op `setState`), since every prop read in these
  files is defensively `this.props.x ?? default`.
- `{{ expr }}` — mustache interpolation; dotted-path lookups (`d.bg`, `mn.heading.open`) plus
  `true`/`false`/`null`/number/quoted-string literals (a reduced version of dc-runtime's own
  `src/expr.ts` resolver — this project never uses `==`/`!`/parens, so those weren't needed).
- `<sc-if value="{{ expr }}">...</sc-if>` — conditional block, drops if falsy.
- `<sc-for list="{{ expr }}" as="x">...</sc-for>` — repeats per array item, substituting
  `{{ x.field }}` inside with that item's value before recursing (loop-scoped substitution has
  to happen **before** resolving any nested `<sc-if>`/`<sc-for>` inside the loop body, since
  those can reference the loop variable — e.g. `sc-for as="d"` containing
  `<sc-if value="{{ d.unsaved }}">`).
- Both tags nest arbitrarily (sc-if inside sc-for inside sc-if, etc.) — handled with a small
  stack-based block matcher rather than a single-pass regex, since real nesting showed up.
- `style-hover="decls"` → generated `.hvN:hover{decls}` rules, deduped by declaration text.
- React-style `onClick="{{ fn }}"` (and other `on*`) handler attributes — stripped (no
  interactivity in a static compile; every frame is a snapshot of one specific state).
- `<dc-import name="Other" prop="val" prop2="{{ true }}" hint-size="WxH" style="...">` — new
  syntax not seen in the earlier single-component imports. Resolves `Other.dc.html` as a
  sibling file, merges the given prop overrides (kebab-case attrs → camelCase, `{{ true/false }}`
  → real booleans) on top of that component's own defaults, and recursively compiles it with
  the exact same pipeline. Used to embed a fully-rendered `Workspace`/`Config` instance inside
  the spec-sheet page, or (in `shots.mjs`) rendered standalone as its own frame.
- One prior-compiler gotcha this project's build.mjs had to account for that the earlier
  AMS-landing script didn't: a bare `<body>` with a percentage-height component root
  (`width:100%;height:100%`) collapses to content height unless `html`/`body` have an explicit
  size. Invisible on the dark theme (collapse gap ≈ page's own near-black background), obvious
  on light (a black bar at the bottom). Fixed by sizing `<body>` to the component's own
  `$preview` dimensions when compiling a standalone frame.

## Known source-design quirk (not a compiler bug)

Frame **05b (canvas-only view)** has visibly misaligned connector arrows in the canvas
mock (`client → api gateway → auth service`). Root cause, confirmed by reading the source:
the canvas layer draws its connectors with one `<svg viewBox="0 0 960 916"
preserveAspectRatio="xMinYMin slice">`, while the boxes themselves (`client`, `api gateway`,
etc.) are separate `position:absolute; left:NNpx` divs in real CSS pixels. In split view the
canvas pane's actual width happens to be ≈960px, so the SVG's 1:1 scale lines up with the
boxes by coincidence. In canvas-only view the pane is nearly full-width (~1600px), so
`slice` scales the SVG content ~1.67x from the top-left — the boxes don't move (they're raw
px) but the connector lines do, so they drift. This is a property of the source `.dc.html`
markup itself (present in any faithful renderer, including the live `support.js` runtime),
not something introduced by the static compile — left as-is rather than patched around.

## What changed vs. the previous export (`~/Projects/synapso/design/`)

`tokens.css` and `support.js` are byte-identical to the previous export — no token or runtime
changes, only component markup/logic.

### `Config.dc.html` — entirely new

Didn't exist in the previous export at all. It's a full preferences/settings screen (app name
"grove", version shown as "0.6.2") with a left settings rail (editor / canvas / appearance /
storage) and a right form column; the active section gets a colored left spine, matching the
doc-spine visual language from Workspace. Two sections were captured: **editor** (font size
stepper, line-height segmented control, spellcheck/vim/line-numbers toggles) and **storage**
(vault path + browse, index backend dropdown, filesystem watch toggle, rebuild-index button
with note count).

### `Workspace.dc.html` — markdown toolbar rework + editor chrome polish

- **Formatting toolbar rebuilt**: the old row of always-visible flat icon buttons (B / I / S,
  `<>`, `ln`, H1, H2, bullet/task/blockquote) was replaced with four grouped dropdown menus —
  **Heading**, **Format** (bold/italic/strikethrough/underline/code, each with its shortcut),
  **Blocks** (code block/blockquote), **Lists** (ordered/bullet/task) — each an icon+caret
  button that opens a popover menu. This needed new interactive state in the component
  (`state.menu`, `menuHandlers()`, a `mn` object per menu key with `open`/`bg`/`fg`/`click`).
- **New "insert table" toolbar icon** added; the old plain-text "wysiwyg → md" label was
  removed from the toolbar's right edge.
- **Link button** changed from a text label (`ln`) to an actual link SVG icon.
- **Doc header row** gained inline **Rename (F2)** and **Delete file** icon buttons (with a
  divider) before the file-type badge.
- **"+2 more documents" overflow badge removed** from both the side doc rail and the notes-only
  top tab strip (previously shown when there were more open docs than fit).
- **Doc spine indicator**: in the top-tabs (notes-only) strip, moved from the tab's bottom edge
  to its top edge, plus a new standalone glowing spine bar rendered below the whole tab strip.
  In the editor's left spine, now conditional on view mode (`sideSpine: view !== 'notes'`) —
  hidden in notes-only view (redundant once the top-tab strip carries its own spine).
- Canvas tool-rail **Pen icon** redrawn (different SVG path, same slot/position) — cosmetic.

### `Notes Canvas Workspace.dc.html` — new frame + new color-picker component

- **New section "00 · config"** added at the top of the spec sheet, showing the two Config
  frames above (dark/editor, light/storage) via `<dc-import name="Config" ...>`.
- **Component-specs section (06)**: the vertical doc-tab spec sample gained an inline
  **Close (⌘W)** button on the active tab; the unsaved-change dot on an inactive tab moved
  from inline-with-the-label to an absolutely-positioned dot at the tab's bottom.
- **New custom color-picker component** documented: the existing row of preset color swatches
  (used for canvas doc/card accent colors) gained a divider plus a "choose colour" conic-
  gradient trigger swatch, and an entirely new floating picker panel was added — a
  saturation/lightness gradient square with a draggable handle, a hue slider, and a hex-code
  input field with a live swatch preview.
