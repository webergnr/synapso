//! A one-line text field: the ⌘K/⌘P query, an inline name, a rename. Keeps
//! the keyboard while focused; ⏎ submits, Esc cancels, ↑↓ go to `on_arrow`.
//! A rename (`blur_cancels`) also cancels on a press anywhere else, or when it
//! goes away (its drawer closed): only ⏎ keeps what was typed.
use freya::{elements::paragraph::ParagraphCursorExt, prelude::*, text_edit::*};

use crate::theme::*;

thread_local! {
    /// Where each rename on screen sits. ⚠️ The open note named from the
    /// tree shows two, its row's and its header's: a press in either is no
    /// press elsewhere for the other.
    static RENAMES: std::cell::RefCell<Vec<(AccessibilityId, Area)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn forget_rename(id: AccessibilityId) {
    RENAMES.with(|r| r.borrow_mut().retain(|(i, _)| *i != id));
}

#[derive(PartialEq, Clone)]
pub struct Field {
    pub value: State<String>,
    pub placeholder: &'static str,
    pub size: f32,
    pub on_submit: EventHandler<String>,
    pub on_cancel: EventHandler<()>,
    pub on_arrow: Option<EventHandler<i32>>,
    /// Select everything on open, so typing replaces it (a rename).
    pub select_all: bool,
    pub weight: FontWeight,
    /// Take the keyboard as soon as it appears (default).
    pub grab: bool,
    /// ⇧⏎ is a line break instead of a submit.
    pub multiline: bool,
    /// Only ⏎ keeps what was typed: a press elsewhere, or the field going
    /// away, cancels and puts the text back (a rename).
    pub blur_cancels: bool,
    /// The face, when not the app's (the board's hand).
    pub family: Option<&'static str>,
    /// The caret's thickness, in the field's own layout pixels.
    pub caret: f32,
    /// Where the caret starts: the start, unless asked.
    pub place: Place,
    /// The text's colour, when not the primary.
    pub color: Option<Color>,
    pub align: TextAlign,
}

impl Field {
    pub fn new(
        value: State<String>,
        on_submit: impl Into<EventHandler<String>>,
        on_cancel: impl Into<EventHandler<()>>,
    ) -> Self {
        Self {
            value,
            placeholder: "",
            size: TEXT_SM,
            on_submit: on_submit.into(),
            on_cancel: on_cancel.into(),
            on_arrow: None,
            select_all: false,
            weight: FontWeight::NORMAL,
            grab: true,
            multiline: false,
            blur_cancels: false,
            family: None,
            color: None,
            align: TextAlign::Left,
            caret: 2.,
            place: Place::Start,
        }
    }
    /// The caret's thickness in the field's layout pixels: a field drawn
    /// scaled divides by its scale to stay thin on screen.
    pub fn caret(mut self, width: f32) -> Self {
        self.caret = width;
        self
    }
    pub fn place(mut self, place: Place) -> Self {
        self.place = place;
        self
    }
    /// Typed in place, as it will be drawn: the face, colour and alignment
    /// of the text it edits (the board's inline editor).
    pub fn looks(mut self, family: &'static str, color: Color, align: TextAlign) -> Self {
        self.family = Some(family);
        self.color = Some(color);
        self.align = align;
        self
    }
    pub fn placeholder(mut self, p: &'static str) -> Self {
        self.placeholder = p;
        self
    }
    pub fn size(mut self, s: f32) -> Self {
        self.size = s;
        self
    }
    pub fn on_arrow(mut self, h: impl Into<EventHandler<i32>>) -> Self {
        self.on_arrow = Some(h.into());
        self
    }
    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }
    pub fn quiet(mut self) -> Self {
        self.grab = false;
        self
    }
    pub fn select_all(mut self) -> Self {
        self.select_all = true;
        self
    }
    pub fn weight(mut self, w: FontWeight) -> Self {
        self.weight = w;
        self
    }
    pub fn blur_cancels(mut self) -> Self {
        self.blur_cancels = true;
        self
    }
}

impl Component for Field {
    fn render(&self) -> impl IntoElement {
        let mut value = self.value;
        let mut editable = use_editable(
            {
                let v = value.read().clone();
                move || v
            },
            EditableConfig::new,
        );
        let holder = use_state(ParagraphHolder::default);
        let a11y = use_a11y();
        let select_all = self.select_all;
        let grab = self.grab;
        let place = self.place;
        use_hook(move || {
            if grab {
                a11y.request_focus();
            }
            if select_all {
                let len = editable.editor().read().len_utf16_cu();
                editable.editor_mut().write().set_selection((0, len));
            }
            match place {
                Place::Start => {}
                Place::End => {
                    let len = editable.editor().read().len_utf16_cu();
                    editable.editor_mut().write().move_cursor_to(len);
                }
                // the point is only a character once the text is laid out:
                // that is the first frame, so wait for the holder to fill
                Place::At(x, y) => {
                    spawn(async move {
                        for _ in 0..20 {
                            let at = holder.peek().0.borrow().as_ref().map(|h| {
                                let s = h.scale_factor;
                                h.paragraph
                                    .cursor_index_at_point((f64::from(x) * s, f64::from(y) * s))
                            });
                            if let Some(at) = at {
                                editable.editor_mut().write().move_cursor_to(at);
                                return;
                            }
                            freya::prelude::timer(std::time::Duration::from_millis(16)).await;
                        }
                    });
                }
            }
        });
        let on_submit = self.on_submit.clone();
        let on_arrow = self.on_arrow.clone();
        let multiline = self.multiline;
        let blur_cancels = self.blur_cancels;
        // what it opened with, for a cancel to put back: the next rename
        // starts from the name, not from what was typed and thrown away
        let opened = use_hook(|| value.peek().clone());
        let cancel = {
            let on_cancel = self.on_cancel.clone();
            move || {
                if blur_cancels {
                    value.set(opened.clone());
                }
                on_cancel.call(());
            }
        };
        // ⚠️ gone without ⏎ is a cancel too (its drawer closed, its row
        // folded away), or the rename outlives its field and comes back
        // with the drawer, taking the keyboard again
        use_drop({
            let on_cancel = self.on_cancel.clone();
            move || {
                if blur_cancels {
                    forget_rename(a11y);
                    on_cancel.call(());
                }
            }
        });
        let mut cancel_key = cancel.clone();
        let mut cancel_press = cancel;

        let on_key = move |e: Event<KeyboardEventData>| {
            if !a11y.is_focused() {
                return;
            }
            let multiline = multiline;
            match &e.key {
                Key::Named(NamedKey::Enter)
                    if multiline && e.modifiers.contains(Modifiers::SHIFT) => {}
                Key::Named(NamedKey::Enter) => {
                    e.stop_propagation();
                    on_submit.call(value.read().clone());
                    return;
                }
                Key::Named(NamedKey::Escape) => {
                    e.stop_propagation();
                    cancel_key();
                    return;
                }
                Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown)
                    if on_arrow.is_some() =>
                {
                    let d = if matches!(e.key, Key::Named(NamedKey::ArrowUp)) {
                        -1
                    } else {
                        1
                    };
                    on_arrow.as_ref().unwrap().call(d);
                    return;
                }
                _ => {}
            }
            if is_app_chord(&e.key, e.modifiers) {
                return;
            }
            editable.process_event(EditableEvent::KeyDown {
                key: &e.key,
                modifiers: e.modifiers,
                editor_line: None,
                holder: None,
            });
            let text = if multiline {
                editable.editor().read().text().to_string()
            } else {
                editable.editor().read().text().replace('\n', "")
            };
            if *value.read() != text {
                value.set(text);
            }
        };

        let editor = editable.editor().read();
        let text = editor.text().to_string();
        let focused = a11y.is_focused();
        let cursor = focused.then(|| editor.cursor_pos());
        let sel = editor.get_selection();
        drop(editor);
        let empty = text.is_empty();

        rect()
            .width(Size::fill())
            .a11y_id(a11y)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::TextInput)
            .on_global_pointer_down(move |e: Event<PointerEventData>| {
                let p = e.global_location();
                let at = (p.x as f32, p.y as f32);
                let cancels = RENAMES
                    .with(|r| press_cancels(blur_cancels, r.borrow().iter().map(|(_, a)| *a), at));
                if cancels {
                    cancel_press();
                }
            })
            .maybe(blur_cancels, |r| {
                r.on_sized(move |e: Event<SizedEventData>| {
                    forget_rename(a11y);
                    RENAMES.with(|r| r.borrow_mut().push((a11y, e.area)));
                })
                // a press in a rename is its own: the row under it would open
                // the note, close the drawer, or start a drag
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                .on_press(|e: Event<PressEventData>| e.stop_propagation())
            })
            .on_global_key_down(on_key)
            .on_global_key_up(move |e: Event<KeyboardEventData>| {
                editable.process_event(EditableEvent::KeyUp { key: &e.key })
            })
            .on_global_pointer_up(move |_| editable.process_event(EditableEvent::Release))
            .child(
                paragraph()
                    .width(Size::fill())
                    .holder(holder.read().clone())
                    .on_mouse_down(move |e: Event<MouseEventData>| {
                        a11y.request_focus();
                        editable.process_event(EditableEvent::Down {
                            location: e.element_location,
                            editor_line: EditorLine::SingleParagraph,
                            holder: &holder.read(),
                        });
                    })
                    .on_mouse_move(move |e: Event<MouseEventData>| {
                        editable.process_event(EditableEvent::Move {
                            location: e.element_location,
                            editor_line: EditorLine::SingleParagraph,
                            holder: &holder.read(),
                        });
                    })
                    .cursor_index(cursor)
                    .cursor_color(c().accent)
                    .cursor_width(self.caret)
                    .highlights(sel.map(|s| vec![s]))
                    .highlight_color(c().selection)
                    .font_size(self.size)
                    .font_weight(self.weight)
                    .maybe(self.family.is_some(), |p| {
                        p.font_family(self.family.unwrap_or_default())
                    })
                    .text_align(self.align)
                    .max_lines(if self.multiline { None } else { Some(1) })
                    .span(if empty {
                        Span::new(self.placeholder).color(c().text_faint)
                    } else {
                        Span::new(text).color(self.color.unwrap_or(c().text_primary))
                    }),
            )
    }
}

/// Where a field's caret starts.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Place {
    Start,
    End,
    /// Under this point, in the field's own layout pixels from its text's
    /// top-left: where the press that opened it landed.
    At(f32, f32),
}

/// A press anywhere but in a rename (`renames`, where each sits) ends one;
/// any other field keeps its text (the canvas label, ⌘K).
fn press_cancels(
    blur_cancels: bool,
    renames: impl IntoIterator<Item = Area>,
    at: (f32, f32),
) -> bool {
    blur_cancels && !renames.into_iter().any(|a| a.contains(at.into()))
}

/// ⌘G, ⌘⇧L, ⌘1…: the app's, not text. The editor types any ⌘+letter it has
/// no shortcut for, so those never reach it; its own (a c x v z y) do. ⚠️
/// AltGr arrives as Ctrl+Alt on Windows and types characters, so Alt passes.
fn is_app_chord(key: &Key, mods: Modifiers) -> bool {
    let Key::Character(c) = key else {
        return false;
    };
    // either one, as `crate::ctrl` reads the app's chords
    mods.intersects(Modifiers::CONTROL | Modifiers::META)
        && !mods.contains(Modifiers::ALT)
        && !matches!(c.as_str(), "a" | "c" | "x" | "v" | "z" | "y")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shortcut_typed_into_a_field_is_not_text() {
        let ch = |c: &str| Key::Character(c.into());
        let cmd = Modifiers::META;
        let ctrl = Modifiers::CONTROL;
        assert!(is_app_chord(&ch("g"), cmd));
        assert!(is_app_chord(&ch("L"), cmd | Modifiers::SHIFT));
        assert!(is_app_chord(&ch("1"), ctrl));
        // the editor's own: select all, copy, cut, paste, undo, redo
        for c in ["a", "c", "x", "v", "z", "y"] {
            assert!(!is_app_chord(&ch(c), cmd), "{c}");
        }
        // ⌘⇧Z arrives as "Z", which the editor doesn't know: it would type it
        assert!(is_app_chord(&ch("Z"), cmd | Modifiers::SHIFT));
        // plain typing, AltGr's @, and keys that aren't characters
        assert!(!is_app_chord(&ch("g"), Modifiers::empty()));
        assert!(!is_app_chord(&ch("G"), Modifiers::SHIFT));
        assert!(!is_app_chord(&ch("@"), ctrl | Modifiers::ALT));
        assert!(!is_app_chord(&Key::Named(NamedKey::Backspace), cmd));
    }

    #[test]
    fn only_a_rename_gives_up_on_a_press_elsewhere() {
        let row = Area::new((10., 10.).into(), (100., 20.).into());
        let header = Area::new((400., 40.).into(), (200., 20.).into());
        // the canvas label, ⌘K: a press elsewhere keeps the text
        assert!(!press_cancels(false, [], (300., 300.)));
        // a rename: a press elsewhere cancels, one in the field moves the caret
        assert!(press_cancels(true, [row], (300., 300.)));
        assert!(press_cancels(true, [row], (50., 40.)));
        assert!(!press_cancels(true, [row], (50., 20.)));
        // the same note named in its row and its header: a press in the
        // other one is not a press elsewhere
        assert!(!press_cancels(true, [row, header], (450., 50.)));
        assert!(press_cancels(true, [row, header], (300., 50.)));
    }
}
