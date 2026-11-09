# Graph View — Implementation Spec

Reference design: `Graph.dc.html` (standalone, 1600×940). Also shown in `Notes Canvas Workspace.dc.html` as screens **05f** (dark, orbit) and **05g** (light, branch).
Tokens: `tokens.css` is the single source of truth. Use only its CSS variables; theme switches via `data-theme="dark" | "light"` on the root.

---

## 1. Purpose

A fourth workspace view (`notes | split | canvas | graph`) that shows how notes link together. It should feel familiar (nodes + links, hover preview, backlinks) but is organized around **one focused note** instead of a free-floating force layout:

- The focused note sits in the center.
- Other notes sit on **rings by hop distance** (1 hop, 2 hops, …).
- Notes beyond the chosen depth appear as **dots on an outer "beyond" ring**.
- Clicking any note re-centers the graph on it (animated).

---

## 2. Layout of the screen

```
┌────────────┬──────────────────────────────────────────────┬──────────────┐
│ left panel │ graph area (canvas bg + dot grid)            │ inspector    │
│ 232px      │ flex:1                                        │ 320px        │
│            │  [‹ graph / auth-flow.md]   [orbit|branch] [depth 1 2 3] [⌕ find ⌘K]
│            │                                                │              │
│            │            stage 1040×760, centered            │              │
│            │                                                │              │
│            │  [▶ vault history ▁▂▃▅▂▇ … ]                   │              │
├────────────┴──────────────────────────────────────────────┴──────────────┤
│ status bar 24px                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Left panel (`--surface-panel`, right border `--border-subtle`)
- Title `graph` + caption `vault · <path>` (`--text-xs`, `--text-faint`).
- Filter input (`--surface-inset`, placeholder `filter notes…`, kbd hint `/`).
- **Groups** section: one row per group with a 2px color spine, name, count, and a mini toggle (24×13). A toggle that's off hides that group's notes from the graph (notes fade out, and their links disappear).
- **Unlinked · N** section: notes with zero links in or out. They are never drawn on the graph. Caption: "notes with no links in or out. they stay off the graph until linked."
- Legend (bottom, above a top border): ring = hops from focus · dot = beyond depth · spine = group color.

### 2.2 Graph area
- Background: `--canvas-bg` with a `radial-gradient(var(--canvas-dot) 1px, transparent 1px)` grid at 22px.
- Top-left pill: `‹` back (focus history, ⌘[), `graph /`, a 2px spine in the focus color, and the focus filename.
- Top-right pill: segmented `◎ orbit | ⊢ branch`, segmented `depth 1 2 3`, and `⌕ find node ⌘K`.
- Stage: 1040×760 at `top:52px`, horizontally centered. Center of the graph is `cx=520, cy=380`.
- Bottom: **vault history** bar (see §6).

### 2.3 Inspector (right, `--surface-panel`)
- A 2px spine in the focus note's color down the left edge, with glow (same "doc spine" idea as the editor gutter).
- Header: `inspector / <file>` and a group badge.
- Excerpt in an inset box.
- Stats: `N in · N out · N within L hops`.
- **Backlinks · N**: one row per incoming link: source name, colored spine, and a context snippet `…before [[target]] after` with the link in `--text-link`. Click = focus that note; hover = highlight it in the graph.
- **Outgoing · N**: chips with a spine. Same click/hover behavior.
- **Mentioned, not linked · N**: text that mentions another note's name without linking to it. The match is highlighted with `--search-hl` inside a dashed-border row, with a `link` button. Clicking `link` creates the link immediately, and the target note animates into its ring.
- Footer: `pin to canvas` (secondary) and `open note ↵` (primary).

### 2.4 Status bar
`GRAPH` badge (accent-dim) · focus name · `N notes · N links · N unlinked` · view-mode segmented control with `graph` active · sync · theme.

---

## 3. Data model

```ts
// Group = the note's top-level folder (e.g. `specs/auth/flow.md` → `specs`). Each folder has a color; that is the group color.
type Group = string;
interface Note { id: string; file: string; group: Group; createdWeek: number; edited: string; excerpt: string }
interface Link { from: string; to: string; before: string; after: string } // context around [[to]] in `from`
interface Mention { target: string; before: string; match: string; after: string } // per source note
```

Group → color token:

| group    | color             | dim (glow/halo)      |
|----------|-------------------|----------------------|
| auth     | `--doc-1` coral   | `--doc-1-dim`        |
| storage  | `--doc-2` amber   | `--doc-2-dim`        |
| planning | `--doc-3` violet  | `--doc-3-dim`        |
| canvas   | `--connector`     | neutral dim          |
| journal  | `--doc-4` cyan    | `--doc-4-dim`        |

Canvas files (`.canvas`) show a `◇ ` prefix in their label.

Graph is treated as **undirected** for layout and distance, and **directed** for the in/out counts and the backlinks/outgoing lists.

---

## 4. Layout algorithm

Visible set = notes where `createdWeek <= t` (history scrubber) **and** group not hidden **and** degree > 0.
Links are drawn only when both ends are visible.

1. **Focus fallback**: if the focused note isn't visible, use the visible note with the highest degree.
2. **BFS** from focus → `dist[id]`. Unreachable = 99.
3. **Angles** (shared by both layouts, so switching layout keeps things coherent):
   - Level 1: sort by group order (`auth, storage, planning, canvas, journal`), then by degree desc. Spread evenly: `θᵢ = -π/2 + (i + 0.5)·2π/n`.
   - Level k ≥ 2: desired angle = circular mean of the angles of its neighbors at level k-1. Then `relax()`.
   - Beyond-depth notes (`dist > L`): relaxed together on the outer ring.
   - `relax(ids, want, cap)`: sort by angle; iteratively push consecutive pairs apart until they're at least `min(2π/n·0.92, cap)` apart. A **fixed pseudo-node at -π/2** keeps the top of every ring free for the ring label (min gap ≈ 0.34 rad).
4. **Orbit positions** (ellipses, ry = 0.7·rx):
   - Ring k: `rx = 380 · (k/L)^0.7`.
   - Beyond ring: `rx=460, ry=330`.
5. **Branch positions** (mind map):
   - Side = `cos(θ) < 0 ? left : right`.
   - Column x offset: `L===1 ? 300 : 190 + (k-1)·240/(L-1)`. Beyond column at ±492.
   - Within each (side, level) bucket, sort by `sin(θ)` and spread vertically: spacing `min(64, 660/m)` (beyond: `min(44, 640/m)`).
6. **Collision pass** (after placement, up to 80 iterations):
   - Box sizes: focus card 208×58; pill ≈ `chars·6.6 + 34` (+13 for canvas glyph) × 24; dot 12×12.
   - For any overlapping pair (margin 10px x / 6px y), push apart along the axis with less overlap. **Branch: vertical only**, so the columns stay straight. The focus node never moves.
   - Clamp to the stage.
7. **Beyond-ring labels (orbit only)**: the label goes above the dot when `sin(θ)<0`, below otherwise. Hide it if its box (≈ `chars·6` × 14) would overlap any pill or the focus card. It then shows on hover.

---

## 5. Rendering

### Nodes (absolutely positioned HTML, centered via `translate(-50%,-50%)`)
- **Focus card**: 208px wide, `--surface-raised`, border `--border-strong`, `box-shadow: var(--shadow-2), 0 0 0 1px <color>, 0 0 32px <dim>`, 2px glowing spine, filename (`--text-sm`, medium), `focus` badge in accent, meta `N in · N out · edited …`.
- **Pill** (within depth): height 24, `--surface-raised`, 1px `--border-subtle`, `--radius-2`, `--shadow-1`, 2px spine, label (`--text-sm`), degree count (`--text-xs`, faint). 1-hop labels in `--text-primary`, deeper ones in `--text-muted`.
- **Dot** (beyond depth): 7px ring, `--canvas-bg` fill, 1.5px border in the group color, optional faint label.
- Hidden notes (filtered by time or group) stay mounted: `opacity:0; scale(.4); pointer-events:none`, positioned at the focus, so they "fly out" when they appear.
- Transition: `left/top .6s cubic-bezier(.2,.8,.2,1), opacity .25s, transform .3s`.

### Edges (one SVG layer under the nodes)
- Always a cubic path (`M … C …`), so `d` can be animated with CSS (`transition: d .6s`) between layouts.
  - Orbit: a quadratic curve bent 30% toward the center (straight if one end is the focus), converted to cubic.
  - Branch: horizontal S-curve, `C mx,y1 mx,y2`.
- Default stroke `--connector`, width 1, opacity .55. Links to beyond-depth notes: dashed `2 4`, opacity .35.
- **Hot edges** (touching the hovered note, or the focus when nothing is hovered): stroke = the *other* end's group color, width 1.5, opacity .9, drawn last.
- While hovering, the non-hot edges drop to opacity .12.

### Guides
- Orbit: dashed ellipses (`2 5`, `--border-strong`, .7) per ring, labeled `1 hop`, `N hops` at the top. The beyond ring is dotted (`1 7`), labeled `beyond · N`.
- Branch: vertical dashed lines per column, labeled at the top, and a `beyond` column.
- Labels: 10px mono, `--text-faint`, with a 4px `--canvas-bg` stroke (`paint-order: stroke`) so they stay readable over lines.

---

## 6. Interactions

| Action | Result |
|---|---|
| Hover node | Neighbors + focus stay at full opacity, the rest fade to .28. Hot edges are colored. Hover card appears (260px, `--surface-overlay`): name, hop badge, excerpt, `click · focus  ⌘click · open`, edited time. It flips left/up near the edges. |
| Click node | Becomes the focus. The previous focus is pushed onto the history stack (max 20). Everything animates to the new positions. |
| `‹` / ⌘[ | Pop the history stack. |
| ⌘click node | Open the note in the editor (not wired in the mock). |
| orbit / branch | Switch layout. Angles are shared, so the arrangement stays recognizable. |
| depth 1/2/3 | Change L. Rings resize with animation, and notes move between rings and the beyond ring. |
| Group toggle | Hide or show a group's notes. |
| `link` on a mention | Adds `{from: focus, to: target}`. The target moves into ring 1. |
| History bar click | Sets `t` to that week. Notes created later fade out. |
| ▶ replay | Animates `t` from 0 to the current week (≈170ms per week). Press again (❚❚) to stop. |

---

## 7. Vault history bar

- A floating panel at the bottom of the graph area (`--surface-panel`, `--radius-3`, `--shadow-1`).
- Play button, `VAULT HISTORY` caption, and a date label (`today · sep 30`, or `week of jun 03`).
- 26 weekly bars (height = 4 + edits·2). Past weeks `--border-strong`, the selected week `--accent` with glow, future weeks `--surface-raised`.
- Month labels under the bars at the first week of each month.
- Right side: `N notes · N links` and `+ files created this week` (or `no new notes this week`).

---

## 8. Props / settings

| prop | values | default |
|---|---|---|
| `theme` | `dark` \| `light` | `dark` |
| `layout` | `orbit` \| `branch` | `orbit` |
| `depth` | 1–3 | 2 |
| `focus` | note id | `auth-flow` |
| `history` | show the history bar | `true` |

User interaction state (layout, depth, focus, t) overrides props until the prop changes.

---

## 9. Implementation notes for a real app

- The layout is deterministic (no physics), so it's cheap to recompute on every change. For large vaults (>300 visible nodes), cap the rings by showing the top-N per ring by degree, plus a `+N more` chip per ring.
- The collision pass is O(n²) per iteration. Use a spatial grid if n is large.
- Mentions come from full-text search for each note's title/aliases in the focused note's body, excluding existing links.
- The history bar needs `createdAt` per note and edit events per week (from git history or the app's own change log).
- Keyboard: arrow keys to move the hover between neighbors, ↵ to focus, ⌘↵ to open, ⌘[ back, `1/2/3` for depth, `O/B` for layout.
- Accessibility: every node is a focusable button with `aria-label="<file>, N links, M hops from <focus>"`, and focus uses `--shadow-focus`.
