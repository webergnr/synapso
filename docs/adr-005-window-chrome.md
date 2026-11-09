# ADR 005 — The window chrome is ours on Windows too

**Date:** 2026-08-17 · **Status:** accepted, implemented (PR #47) · **Context:**
grilling of "the bar on Windows could be better", with Zed's Windows title bar
as the reference, checked against what iced 0.14 and winit 0.30 already do.

> ⚠️ **Historical.** Written while the shell was iced. Where this names an iced
> type or file, read it as the shape of the problem, not as the code today —
> the cutover landed (`docs/floem-migration-trd.md` §5) and the shell is Floem.
> The decisions still hold; the mechanics moved.

## The rule inherited

`platform.rs` opens with it: **"Synapso draws its own title bar."** On macOS
that is already true — the native bar is transparent, the content runs to the
top edge and the traffic lights float over it. On Windows the same file says
"the native decorations stay **for now**, and the same bar sits under them",
which is the screenshot: two bars stacked, the top one in the system's font.

This is not a new feature. It is the unfinished half of a decision already
taken — and the design has been waiting for it: `Workspace.dc.html` carries a
`winChrome` block, 32px tall, with the three buttons at 46px and the close
hover at `#E81123`.

## What the reference asks for × what the stack already gives

| what Zed's bar does | state |
| --- | --- |
| drag the window by the bar | `window::drag(id)` |
| minimise / maximise / close | `window::minimize` · `toggle_maximize` · `close` |
| double-click the bar to maximise | ⚠️ **not available** — see below |
| resize by the window edges | **free** — winit keeps `WS_SIZEBOX` and does the frameless part in `WM_NCCALCSIZE` (`window_state.rs:256`, `event_loop.rs:1162`) |
| Aero Snap (Win+arrows, drag to edge) | **free** — winit keeps the styles for exactly this |
| maximise without covering the taskbar | **free** — winit clamps the maximised rect to the monitor |
| **Snap Layouts flyout** (hover the maximise button) | **not available** — needs `WM_NCHITTEST` → `HTMAXBUTTON` |
| hamburger + "Open Recent Project" | there is no vault to open yet |

⚠️ The buttons keep working inside a full-width drag area for free too:
`mouse_area` runs its child first and returns if the event was captured
(`iced_widget-0.14.2/src/mouse_area.rs:241`). No hit-region maths.

⚠️ **Corrected while building.** This ADR first listed double-click-to-maximise
as free, because `mouse_area` has an `on_double_click`. `Message::WindowDrag`
already carries the measurement that says otherwise: the press starts a
*native* drag loop that swallows the second click, so the handler can never
fire. Windows keeps the muscle memory only if that loop behaves differently
there — one line to add if the checklist says it does. Until then, ▢ is the
way to maximise, and Aero Snap still is.

## The four decisions

### D1 — Snap Layouts is the price, and we pay it

The bar ships with **zero lines of native code**. Everything else the system
offers survives: Win+arrows, drag-to-edge snapping, double-click to maximise,
native resize edges.

**Rejected: subclassing the HWND** (`SetWindowSubclass` on the handle from
`window::run`) to answer `HTMAXBUTTON`. It buys one hover flyout and costs a
custom `WNDPROC` in unsafe Rust living next to winit's own `WM_NCCALCSIZE`
override — code that cannot be run, read or debugged on the machine it is
written on. If it is ever missed, it goes in later without touching the bar.

**Rejected: keeping the native decorations and only darkening them** with
`DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)`. Fifteen lines and the
ugliness goes away, but the two stacked bars stay — 32px of chrome, forever,
in a font that is not ours.

### D2 — the left slot stays exactly as it is

The three buttons go on the right; nothing else about the bar moves. When
vaults arrive, the selector lands to the left of the note name and pushes it
along — one line of layout.

**Rejected: reserving the slot now** with a `☰` that opens a menu holding the
current vault and an empty "open recent". It is chrome that does nothing, and
a menu with one dead row teaches you not to click it.

**Rejected: making the note name itself the target.** It puts a click target
inside the drag area — the exact thing that makes this kind of bar annoying —
and contradicts `top_bar`'s own comment: *a title bar answers to dragging, not
to clicks*.

### D3 — our ✕ does what the native one does

`Message::Save`, then close. If the save is refused because the note changed on
disk, the refusal has already written the rescue beside it (that is why the
rescue exists), and the window closes anyway.

**Rejected: holding the close** until a second click when something was
refused. It invents a state ⌘Q cannot have — two exits with two different
rules, and the one that is harder to notice is the one nobody tested.

**Rejected: a confirmation modal.** It is what most apps do, and it is what
this one decided not to be: it saves by itself and rescues what it cannot save,
precisely so it never has to ask.

### D4 — the proof is a CI zip, so it ships whole

There is no Rust toolchain on the Windows machine: each round costs a
`workflow_dispatch` build and a download. So this does **not** get iterated
into shape — it lands complete, with a checklist of what to look at, and the
first batch of corrections is treated as one round rather than a ping-pong.

**What that buys, and it is not nothing:** every part of this except
`decorations: false` is the same code on both systems. The buttons, the drag,
the double-click and the layout are proved on macOS *before* a Windows build is
spent. What genuinely cannot be checked here is narrow: how the bar sits when
the window is maximised, and whether the drag feels right.

## Assumed, without a round

- **The drag goes on both systems, not behind a `cfg`.** macOS hides the native
  bar but keeps its invisible drag strip at ~28px, and ours is 32 — today the
  bottom four pixels do not drag. One `mouse_area` fixes that and removes a
  platform difference instead of adding one.
- **32px on Windows**, against the 28 there now: it is what the design says and
  what a 46×32 button needs. macOS keeps its 32 and its 66px of room for the
  traffic lights.
- **Shadow on; corners left to the system.** `undecorated_shadow: true` is what
  tells the window's edge from the desktop once the frame is gone. ⚠️ winit's
  own note: it puts a thin 1px line at the top of the window — first thing on
  the checklist. `corner_preference` stays `Default`, which already means
  "rounded" on Windows 11; naming it explicitly would only take the choice away
  from a system that is making the right one.
- **No escape hatch back to the native decorations.** Even in the worst case
  the window is not trapped: Win+arrows and drag-to-edge still move it.

## Implementation risk, named

Small — around fifty lines, and the two things that usually make this hard
(resize edges, the maximised rect) are already handled inside winit.

⚠️ The real risk is not the code, it is the **loop**: nobody here can see the
result. Everything provable on macOS gets proved before a Windows build is
spent, and the checklist has to be written as if the person reading it has one
run to answer it — because that is what a round costs.
