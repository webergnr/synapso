//! The window bar (`design/Workspace.dc.html`'s `macChrome` and `winChrome`
//! strips, `design/Window Headers.dc.html`).
//!
//! - macOS: the native traffic lights float over its left end, then ‹ ›
//!   through the documents opened, the note and the board in the middle, and
//!   on the right the doors to ⌘K and ⌘P.
//! - Windows: the window has no system bar. The title runs from the left,
//!   then the doors, then the 46px minimize / maximize / close buttons.
//!
//! Pressing anywhere else on it drags the window; a double press zooms it.
//! Linux keeps the system's own title bar.
use std::path::{Path, PathBuf};

use freya::prelude::*;

use crate::{
    app::{App, Overlay, View},
    base_name, keys,
    theme::*,
};

/// The bar's height: AppKit's own title bar, so the native lights (macOS 26:
/// 14px, centred 16 down) sit centred on it. The design draws 38; the product
/// chose 32 with the lights centred, as the Floem shell had it. (An empty
/// `NSToolbar` would centre them on 38, but in full screen it stays on screen
/// and covers the whole bar.)
pub const HEIGHT: f32 = 32.;
/// Where the native traffic lights end, from the window's left edge —
/// measured on macOS 26 and not guessed. The design leaves 18 after them
/// (its gap plus the arrows' margin).
const LIGHTS_END: f32 = 68.;

/// Whether the bar is drawn: the Mac, where the window gives its title bar
/// to the content, and Windows, where it has none (`main`'s window config).
pub const fn shown() -> bool {
    cfg!(any(target_os = "macos", windows))
}

/// The documents opened, oldest first, and which one ‹ › stand on.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct History {
    /// The vault the paths are relative to.
    root: PathBuf,
    docs: Vec<String>,
    at: usize,
}

impl History {
    /// A document came up. The one ‹ › stand on is no step (that is them
    /// arriving); anything else drops what ‹ had walked back past. Another
    /// vault (⌘O) starts over: its paths name other files.
    fn visit(&mut self, root: &Path, rel: &str) {
        if self.root != root {
            *self = History {
                root: root.to_path_buf(),
                ..Default::default()
            };
        }
        if rel.is_empty() || self.docs.get(self.at).is_some_and(|d| d == rel) {
            return;
        }
        self.docs.truncate(self.at + 1);
        self.docs.push(rel.to_string());
        self.at = self.docs.len() - 1;
    }

    /// The next document `by` steps away that `go` accepts (still in the
    /// vault, not already on screen), and stand on it.
    fn step(&mut self, by: isize, go: impl Fn(&str) -> bool) -> Option<String> {
        let mut i = self.at as isize + by;
        while let Some(rel) = usize::try_from(i).ok().and_then(|i| self.docs.get(i)) {
            if go(rel) {
                self.at = i as usize;
                return Some(rel.clone());
            }
            i += by;
        }
        None
    }
}

/// The history ‹ › walk, fed by every document that comes up. Called from
/// the app's root: a theme flip remounts the bar, and the history outlives it.
pub fn use_history(app: App) -> State<History> {
    let mut history = use_state(History::default);
    use_side_effect(move || {
        let rel = app.note.read().clone();
        history.write().visit(&app.root(), &rel);
    });
    use_side_effect(move || {
        let rel = app.canvas.read().clone();
        history.write().visit(&app.root(), &rel);
    });
    history
}

#[derive(PartialEq)]
pub struct TitleBar {
    pub history: State<History>,
}

impl Component for TitleBar {
    fn render(&self) -> impl IntoElement {
        if cfg!(windows) {
            WinBar {}.into_element()
        } else {
            MacBar {
                history: self.history,
            }
            .into_element()
        }
    }
}

#[derive(PartialEq)]
struct MacBar {
    history: State<History>,
}

impl Component for MacBar {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let history = self.history;

        let note = app.note.read().clone();
        let canvas = app.canvas.read().clone();
        let dirty = app.dirty.read().contains(&note);
        let saving = !app.dirty.read().is_empty();
        let dot = if note.is_empty() {
            c().border_subtle
        } else {
            c().tone(app.index.read().tone(&note))
        };

        let mut title = rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .font_size(TEXT_SM)
            .child(
                rect()
                    .width(Size::px(6.))
                    .height(Size::px(6.))
                    .corner_radius(3.)
                    .background(dot)
                    .shadow(Shadow::new().blur(6.).color(dot)),
            );
        if !note.is_empty() {
            title = title.child(
                label()
                    .text(base_name(&note).to_string())
                    .color(c().text_primary)
                    .font_weight(FontWeight::MEDIUM),
            );
        }
        if dirty {
            title = title.child(label().text("•").color(c().text_faint));
        }
        if !note.is_empty() && !canvas.is_empty() {
            title = title.child(label().text("—").color(c().text_faint));
        }
        if !canvas.is_empty() {
            title = title.child(
                label()
                    .text(base_name(&canvas).to_string())
                    .color(c().text_muted),
            );
        }
        // nothing open: the app's name, as the Floem shell's bar read
        if note.is_empty() && canvas.is_empty() {
            title = title.child(label().text("Synapso").color(c().text_muted));
        }

        rect()
            .width(Size::fill())
            .height(Size::px(HEIGHT))
            .horizontal()
            .cross_align(Alignment::Center)
            .content(Content::Flex)
            .padding((0., 12.))
            .spacing(12.)
            .background(c().surface_panel)
            .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                bottom: 1.,
                ..Default::default()
            }))
            .window_drag()
            // the bar's own padding is the first 12 of it
            .child(rect().width(Size::px(LIGHTS_END - 12.)))
            .child(
                rect()
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(6.)
                    .margin((0., 0., 0., 6.))
                    .child(Step {
                        glyph: "‹",
                        by: -1,
                        history,
                    })
                    .child(Step {
                        glyph: "›",
                        by: 1,
                        history,
                    }),
            )
            .child(rect().width(Size::flex(1.)))
            .child(
                rect()
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(8.)
                    .font_size(TEXT_XS)
                    .color(c().text_muted)
                    .child(Door {
                        text: "⌕ search",
                        key: "K",
                        to: Overlay::Palette,
                    })
                    .child(Door {
                        text: "↗ open",
                        key: "P",
                        to: Overlay::Picker,
                    })
                    .child(
                        label()
                            .text("●")
                            .color(if saving { c().doc[1] } else { c().accent }),
                    )
                    .child(label().text(if saving { "saving…" } else { "synced" })),
            )
            .child(
                // centred on the window, not on what is left between the ends
                // (and not on the padded inside an absolute child is placed in)
                rect()
                    .position(Position::new_global().left(0.).top(0.))
                    // ⚠️ out of hit-testing: a long name runs over ‹ › and
                    // the doors, and its labels would take their presses
                    .interactive(Interactive::No)
                    .width(Size::window_percent(100.))
                    .height(Size::px(HEIGHT))
                    .center()
                    .child(title),
            )
    }
}

/// ‹ or ›: a step through the documents opened.
#[derive(PartialEq)]
struct Step {
    glyph: &'static str,
    by: isize,
    history: State<History>,
}

impl Component for Step {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let (by, mut history) = (self.by, self.history);
        let hov = *hover.read();
        rect()
            .padding((2., 4.))
            .corner_radius(3.)
            .background(if hov {
                c().surface_inset
            } else {
                Color::TRANSPARENT
            })
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            // the bar's drag must not take a press meant for the button
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(move |_| {
                let view = *app.view.peek();
                let (note, canvas) = (app.note.peek().clone(), app.canvas.peek().clone());
                let on_screen = |rel: &str| match view {
                    View::Notes => rel == note,
                    View::Canvas => rel == canvas,
                    View::Split => rel == note || rel == canvas,
                    View::Graph => false,
                };
                let next = history.write().step(by, |rel| {
                    app.index.peek().doc(rel).is_some() && !on_screen(rel)
                });
                if let Some(rel) = next {
                    app.open(&rel);
                }
            })
            .child(label().text(self.glyph).font_size(TEXT_SM).color(if hov {
                c().text_primary
            } else {
                c().text_faint
            }))
    }
}

/// A door on the bar's right: ⌘K's palette or ⌘P's picker, with its chord.
#[derive(PartialEq)]
struct Door {
    text: &'static str,
    key: &'static str,
    to: Overlay,
}

impl Component for Door {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let hov = *hover.read();
        let to = self.to.clone();
        let mut overlay = app.overlay;
        // `Window Headers`: a 24px well, the chord's chip filling its right end.
        // ⚠️ Freya draws the 1px border over the padding: the mock's 2px gap
        // around the chip (6px before the text) needs one more
        rect()
            .height(Size::px(24.))
            .horizontal()
            .spacing(6.)
            .padding((3., 3., 3., 7.))
            .corner_radius(6.)
            .background(if hov {
                c().surface_raised
            } else {
                c().surface_inset
            })
            .border(
                Border::new()
                    .fill(if hov {
                        c().border_strong
                    } else {
                        c().border_subtle
                    })
                    .width(1.),
            )
            .cursor(CursorIcon::Pointer)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            // the bar's drag must not take a press meant for the door
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(move |_| overlay.set(to.clone()))
            .child(
                rect()
                    .height(Size::fill())
                    .main_align(Alignment::Center)
                    .child(label().text(self.text).color(if hov {
                        c().text_primary
                    } else {
                        c().text_muted
                    })),
            )
            .child(
                // 0 4px inside its own 1px border, 1px between ^ and the key
                rect()
                    .height(Size::fill())
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .padding((0., 5.))
                    .spacing(1.)
                    .corner_radius(3.)
                    .background(c().surface_raised)
                    .border(
                        Border::new()
                            .fill(if *app.dark.read() {
                                c().border_subtle
                            } else {
                                c().border_strong
                            })
                            .width(1.),
                    )
                    .child(keys::modifier(TEXT_XS, c().text_faint))
                    .child(label().text(self.key).color(c().text_faint)),
            )
    }
}

/// The Windows bar (`winChrome`): the app's mark and the title from the
/// left, the doors and the sync state, then the window's own buttons.
#[derive(PartialEq)]
struct WinBar {}

impl Component for WinBar {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let note = app.note.read().clone();
        let canvas = app.canvas.read().clone();
        let dirty = app.dirty.read().contains(&note);
        let saving = !app.dirty.read().is_empty();
        let vault = crate::vault_root()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dash = || label().text("—").color(c().text_faint);
        let has_canvas = !canvas.is_empty();

        // `note.md • — board.canvas — vault`
        let title = rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .child(
                label()
                    .text(if note.is_empty() {
                        "—".to_string()
                    } else {
                        base_name(&note).to_string()
                    })
                    .color(c().text_primary),
            )
            .maybe_child(dirty.then(|| label().text("•").color(c().text_faint)))
            .maybe_child(has_canvas.then(dash))
            .maybe_child(has_canvas.then(|| label().text(base_name(&canvas).to_string())))
            .child(dash())
            .child(label().text(vault));

        rect()
            .width(Size::fill())
            .height(Size::px(HEIGHT))
            .horizontal()
            .content(Content::Flex)
            .font_size(TEXT_XS)
            .color(c().text_muted)
            .background(c().surface_panel)
            .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                bottom: 1.,
                ..Default::default()
            }))
            .window_drag()
            .child(
                // what is left after the doors and the buttons: a long title
                // is cut here instead of pushing them off the window
                rect()
                    .width(Size::flex(1.))
                    .height(Size::fill())
                    .overflow(Overflow::Clip)
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(8.)
                    .padding((0., 0., 0., 12.))
                    .child(
                        rect()
                            .width(Size::px(12.))
                            .height(Size::px(12.))
                            .corner_radius(3.)
                            .background(c().accent)
                            .opacity(0.9),
                    )
                    .child(title),
            )
            .child(
                rect()
                    .height(Size::fill())
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(8.)
                    .padding((0., 12., 0., 0.))
                    .child(Door {
                        text: "⌕ search",
                        key: "K",
                        to: Overlay::Palette,
                    })
                    .child(Door {
                        text: "↗ open",
                        key: "P",
                        to: Overlay::Picker,
                    })
                    .child(
                        // a drawn dot: the font's ● stands taller than the mock's
                        rect()
                            .width(Size::px(5.))
                            .height(Size::px(5.))
                            .corner_radius(2.5)
                            .background(if saving { c().doc[1] } else { c().accent }),
                    )
                    .child(label().text(if saving { "saving…" } else { "synced" })),
            )
            .child(WinButton {
                what: WinAction::Minimize,
            })
            .child(WinButton {
                what: WinAction::Maximize,
            })
            .child(WinButton {
                what: WinAction::Close,
            })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum WinAction {
    Minimize,
    Maximize,
    Close,
}

/// One of the window's own buttons: 46px wide and the bar's full height,
/// as the system draws them; close turns red under the pointer.
#[derive(PartialEq)]
struct WinButton {
    what: WinAction,
}

impl Component for WinButton {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let hov = *hover.read();
        let what = self.what;
        // the font has no ✕, so the three are drawn
        let (glyph, size, name) = match what {
            WinAction::Minimize => (crate::icons::WIN_MIN, 10., "Minimize"),
            WinAction::Maximize => (crate::icons::WIN_MAX, 9., "Maximize"),
            WinAction::Close => (crate::icons::WIN_CLOSE, 10., "Close"),
        };
        let (bg, fg) = match (hov, what) {
            (false, _) => (Color::TRANSPARENT, c().text_muted),
            (true, WinAction::Close) => (Color::from_rgb(0xE8, 0x11, 0x23), Color::WHITE),
            (true, _) => (c().surface_inset, c().text_primary),
        };
        rect()
            .width(Size::px(46.))
            .height(Size::fill())
            .center()
            // named for a screen reader; out of the Tab order, as the
            // system's own are (Alt+F4 and Win+arrows stay the keyboard's)
            .a11y_role(AccessibilityRole::Button)
            .a11y_alt(name)
            .background(bg)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            // the bar's drag must not take a press meant for the button
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .on_press(move |_| {
                let id = Platform::window_id();
                match what {
                    WinAction::Minimize => {
                        Platform::get().with_window(id, |w| w.set_minimized(true))
                    }
                    WinAction::Maximize => {
                        Platform::get().with_window(id, |w| w.set_maximized(!w.is_maximized()))
                    }
                    WinAction::Close => {
                        // ⚠️ closing this way skips `main`'s on-close, which
                        // writes what the panes hold: write it here
                        crate::note::flush_note();
                        crate::board::flush_board();
                        Platform::get().close_window(id);
                    }
                }
            })
            .child(crate::icons::icon(glyph, size, fg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walked(rels: &[&str]) -> History {
        let mut h = History::default();
        for r in rels {
            h.visit(Path::new("/vault"), r);
        }
        h
    }

    #[test]
    fn back_and_forward_walk_the_documents_opened() {
        let mut h = walked(&["a.md", "b.md", "c.canvas"]);
        assert_eq!(h.step(-1, |_| true).as_deref(), Some("b.md"));
        assert_eq!(h.step(-1, |_| true).as_deref(), Some("a.md"));
        assert_eq!(h.step(-1, |_| true), None);
        assert_eq!(h.step(1, |_| true).as_deref(), Some("b.md"));
        assert_eq!(h.step(1, |_| true).as_deref(), Some("c.canvas"));
        assert_eq!(h.step(1, |_| true), None);
    }

    #[test]
    fn arriving_by_a_step_is_not_a_visit() {
        let mut h = walked(&["a.md", "b.md"]);
        let back = h.step(-1, |_| true).unwrap();
        // the pane opens what the step chose, and the bar hears it
        h.visit(Path::new("/vault"), &back);
        assert_eq!(h.step(1, |_| true).as_deref(), Some("b.md"));
    }

    #[test]
    fn a_new_document_after_going_back_drops_the_way_forward() {
        let mut h = walked(&["a.md", "b.md", "c.md"]);
        h.step(-1, |_| true);
        h.visit(Path::new("/vault"), "d.md");
        assert_eq!(h.step(1, |_| true), None);
        assert_eq!(h.step(-1, |_| true).as_deref(), Some("b.md"));
    }

    #[test]
    fn a_step_skips_what_it_may_not_open() {
        // b.md is on screen (or gone): ‹ goes past it, and › comes back past it
        let mut h = walked(&["a.md", "b.md", "c.canvas"]);
        assert_eq!(h.step(-1, |r| r != "b.md").as_deref(), Some("a.md"));
        assert_eq!(h.step(1, |r| r != "b.md").as_deref(), Some("c.canvas"));
    }

    #[test]
    fn another_vault_starts_its_own_history() {
        // ⌘O to a vault that has a todo.md too: ‹ must not open the old
        // vault's a.md here, where it is a different file or none at all
        let mut h = walked(&["a.md", "todo.md"]);
        h.visit(Path::new("/other"), "todo.md");
        assert_eq!(h.step(-1, |_| true), None);
        h.visit(Path::new("/other"), "b.md");
        assert_eq!(h.step(-1, |_| true).as_deref(), Some("todo.md"));
        assert_eq!(h.step(-1, |_| true), None);
    }

    #[test]
    fn nothing_open_is_no_step() {
        let mut h = walked(&["", "a.md", ""]);
        assert_eq!(h.docs, vec!["a.md".to_string()]);
        assert_eq!(h.step(-1, |_| true), None);
    }
}
