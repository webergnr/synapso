# ADR-009 — the shell moves from Floem to Freya

**Status:** accepted — the app is the Freya shell · **Date:** 2026-10-16

## Context

The Floem shell was built on a fork (`webergnr/floem`) that carried four fixes
upstream did not have: variable line heights, a layout-listener crash, hinting
and pixel snapping on 1x screens, and an image export for the Skia renderer.
Every one of them was a fight with the toolkit rather than work on the app, and
the note editor ended up as an owned surface (`src/surface/`) because Floem's
own editor could not do what the sheet needs.

The new Claude Design export asks for much more motion than the app had: a
tree that slides over the note, views that glide into each other, a colour
tooltip that previews live, an animated graph. On the fork, each would have
been another patch.

## The test

Before deciding, the whole workspace was rebuilt on Freya 0.5 (Skia, its own
component model, built-in animation) as a proof of concept, on top of the same
`synapso-core`, in `crates/synapso-freya` (PR #188). It had to:

1. rebuild the design's integrated workspace (`design/Workspace.dc.html`),
   not a sketch of it;
2. read and write a real vault through the core — `vault::write_checked`,
   `Canvas::load`, the sidecar — with the integrity rules of ADR-001;
3. stay inside a 160 Hz frame budget on a stress vault (a 3,121-line note, a
   200-node canvas).

It did all three. Every number is in the README's Performance table; the worst
p99 is 6.8 ms, in the view transition, against 6.25 ms per frame.

## Decision

The Freya shell is the app. The Floem shell, its fork pin and the owned note
surface are deleted.

- **Freya comes from our fork**, `webergnr/freya`, branch `synapso`, pinned by
  rev in `Cargo.toml`. It is upstream 0.5.0-rc.10 plus five commits of ours:
  - rotated labels are rasterised at 4× and downsampled, so the rail's
    vertical tabs aren't jagged;
  - ⇥ and the arrows don't move focus while a text field holds it — without
    it, Tab in the note jumps to the canvas;
  - a paragraph paints the holder its layout measured, not the element's
    own (`bfa200fd`, with a test in `crates/freya-core/tests/paragraph.rs`).
    Handlers never compare equal, so a re-render swaps in a new element for
    every paragraph with one; when nothing changed the layout it was never
    measured, and render unwrapped an empty holder — ⌘P's arrows panicked.
  - winit's trackpad `PinchGesture` is forwarded as `on_pinch` (`9f0323c5`,
    with a test in `crates/freya-core/tests/pinch.rs`). Upstream drops it, so
    the canvas could not zoom on a pinch. It is its own event, not a wheel
    one, so scroll views don't scroll on it. winit sends it on macOS only.
    Upstream has the same code.
  - a paragraph's line cursor has a settable width, `cursor_width`
    (`f0bba7cf`, default 2 as before). The canvas's inline editor is laid
    out at the text's own size and drawn under a scale effect, so it matches
    the board's text at any zoom. Its cursor was scaled with it, thick when
    zoomed in and faint when zoomed out.
  All are small and self-contained, and all are candidates for upstream.
  On top of them sit two upstream fixes picked ahead of a release (`3cbd8e3e`,
  `ab0e94ca`; upstream #2259 and #2425): a keyed list that gained, lost and
  reordered children in one render left the diff's paths stale, and the next
  lookup unwrapped nothing. ⌘P/⌘K's results do exactly that while typing; it
  was the crash we had never caught until a console run on 2026-10-29.
- **The core is untouched.** Inline syntax the design added (`***`, `==`,
  `<u>`, `[^1]`) is layered on in the shell (`src/rich.rs`); moving it into
  `synapso-core` is a separate change.
- **The vault stays readable both ways.** The sidecar keeps the Floem shell's
  keys — `colors`, `closed`, `recent` — and a note coloured by its bare name
  there still reads. Folder colours are new keys (`"auth/": 0`) that the old
  shell would simply ignore. *(Since #194, the open/closed and recent lists
  key each file — `closed_files`, `recent_files` — and the first open moves
  `closed`/`recent` onto them, so an old shell reading that vault sees every
  document open.)*
- `behave.rs` and `format.rs` (what ⏎, ⇥ and the format keys do to the text)
  were already toolkit-free; they came over unchanged, with their tests.

## Consequences

- One toolkit dependency instead of a fork of four fixes; animation is a hook,
  not a patch.
- Lost with the Floem shell, to be rebuilt: the app's own frameless title bar
  (the system one is used), and the card-metrics test, which measured text
  with Floem's layout.
- Freya is a release candidate. The pin is by rev, so nothing moves until we
  move it; moving it means rebasing the fork's commits (the list above).
- `docs/floem-migration-trd.md` and ADR-005 (window chrome) describe the Floem
  shell and are kept as history.
