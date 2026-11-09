//! The note pane: breadcrumb, format bar, and the sheet — the markdown buffer
//! edited in place, one paragraph per line, its markers folded away except on
//! the line being edited.
use freya::animation::*;
use std::rc::Rc;

use freya::{prelude::*, text_edit::*};
use synapso_core::{doc::Lines, vault};

use crate::{
    app::{App, Caret},
    field::Field,
    icons,
    rich::{self, Deco, Rich},
    tag,
    theme::*,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind {
    Blank,
    Heading(u8),
    Para,
    Task(bool),
    Bullet,
    Quote,
    FenceOpen,
    FenceClose,
    Code,
    /// `1. ` — the number is the one typed.
    Ordered,
    /// `---`
    Rule,
    /// `| a | b |` rows, drawn as one boxed block.
    Table,
    /// `|:---|:---:|`: the alignments, folded to the header's rule.
    TableRule,
    /// `[^1]: text`, a footnote's definition.
    Footnote,
    /// `![alt](file)`: the picture, the alt as its caption.
    Image,
}

/// The block each line belongs to. Cheap enough to redo per keystroke.
fn classify(text: &str) -> Vec<Kind> {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in text.split('\n') {
        let t = line.trim_start();
        let kind = if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
            if in_code {
                Kind::FenceOpen
            } else {
                Kind::FenceClose
            }
        } else if in_code {
            Kind::Code
        } else if t.is_empty() {
            Kind::Blank
        } else if let Some(level) = heading_level(t) {
            Kind::Heading(level)
        } else if t.starts_with("- [ ] ") || t.starts_with("* [ ] ") {
            Kind::Task(false)
        } else if t.starts_with("- [x] ") || t.starts_with("- [X] ") || t.starts_with("* [x] ") {
            Kind::Task(true)
        } else if t.starts_with("- ") || t.starts_with("* ") {
            Kind::Bullet
        } else if t.starts_with('>') {
            Kind::Quote
        } else if ordered_marker(t).is_some() {
            Kind::Ordered
        } else if t == "---" || t == "***" || t == "___" {
            Kind::Rule
        } else if rich::is_table_rule(t) {
            Kind::TableRule
        } else if t.starts_with('|') {
            Kind::Table
        } else if t.starts_with("[^") && t.contains("]:") {
            Kind::Footnote
        } else if synapso_core::doc::image_line(t).is_some() {
            Kind::Image
        } else {
            Kind::Para
        };
        out.push(kind);
    }
    // a row alone is a paragraph: `|test|` on its own line is drawn as the
    // words it is until a second row lands next to it (`table::in_a_run`)
    let shaped: Vec<bool> = out
        .iter()
        .map(|k| matches!(k, Kind::Table | Kind::TableRule))
        .collect();
    let rows = synapso_core::table::in_a_run(&shaped);
    for (i, kind) in out.iter_mut().enumerate() {
        if shaped[i] && !rows[i] {
            *kind = Kind::Para;
        }
    }
    out
}

fn heading_level(t: &str) -> Option<u8> {
    let hashes = t.chars().take_while(|c| *c == '#').count();
    ((1..=6).contains(&hashes) && t[hashes..].starts_with(' ')).then_some(hashes as u8)
}

/// Bytes of block syntax at the start of a line: `## `, `- [ ] `, `> `.
fn prefix_len(kind: Kind, line: &str) -> usize {
    let indent = line.len() - line.trim_start().len();
    indent
        + match kind {
            Kind::Heading(l) => l as usize + 1,
            Kind::Task(_) => 6,
            Kind::Bullet => 2.min(line.trim_start().len()),
            Kind::Quote => line
                .trim_start()
                .chars()
                .take_while(|c| *c == '>' || *c == ' ')
                .count(),
            Kind::Ordered => ordered_marker(line.trim_start()).unwrap_or(0),
            Kind::Footnote => line.trim_start().find("]:").map_or(0, |e| {
                let t = line.trim_start();
                (e + 2 + usize::from(t[e + 2..].starts_with(' '))).min(t.len())
            }),
            _ => 0,
        }
}

#[derive(PartialEq)]
pub struct NotePane {
    pub rel: String,
}

/// A pane's "write now", saying why when the edit is still on screen only.
type Flush = Box<dyn Fn() -> Option<String>>;

thread_local! {
    /// The open note's "write now": switching notes, flipping the theme and
    /// closing the window all call it while the pane is still alive.
    static FLUSH: std::cell::RefCell<Option<Flush>> = const { std::cell::RefCell::new(None) };
}

/// Says why, when the write failed and the edit is still on screen only (a
/// rescue to `-unsaved` is on disk, so it counts as written).
pub fn flush_note() -> Option<String> {
    FLUSH.with(|f| f.borrow().as_ref().and_then(|f| f()))
}

/// What a save attempt came to.
enum Saved {
    Wrote,
    Rescued(std::path::PathBuf),
    Failed(String),
}

impl Component for NotePane {
    fn render_key(&self) -> DiffKey {
        (&self.rel).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let rel = self.rel.clone();
        let path = app.root().join(&rel);
        let loaded = use_state({
            let path = path.clone();
            // a failed load never saves: remember whether there is a file to overwrite
            move || vault::read(&path).ok()
        });
        let stamp = use_state({
            let path = path.clone();
            move || vault::stamp(&path)
        });
        let rescued = use_state(|| None::<std::path::PathBuf>);
        let mut editable = use_editable(
            {
                let loaded = loaded.read().clone();
                move || loaded.unwrap_or_default()
            },
            EditableConfig::new,
        );
        let caret = app.caret;
        let edits = use_state(|| 0u64);
        let saved_at = use_state(|| 0u64);
        let a11y = use_a11y();
        let touched = use_state(|| 0u64);
        let mut seen_touch = use_state(|| 0u64);
        let mut width = use_state(|| 600f32);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        let mut fmt_request = use_state(|| None::<Fmt>);
        let mut view_h = use_state(|| 600f32);
        let follow = use_state(|| false);
        let fmt_menu = use_state(|| None::<usize>);
        let mut scrolled_to = use_state(|| None::<f32>);
        // where the sheet starts in the window, to read a drop's line from
        let mut sheet_top = use_state(|| 0f32);
        // a landing's row and the width it was last scrolled for (`held_row`)
        let mut pin = use_state(|| None::<(usize, f32)>);
        // which re-scroll the pin's settle timer belongs to: only the latest
        // lets go
        let mut pin_gen = use_state(|| 0u64);

        let publish = move |editable: &UseEditable| {
            let mut caret = caret;
            let editor = editable.editor().read();
            let text = editor.text().to_string();
            caret.set(Caret {
                row: editor.cursor_row(),
                col: editor.cursor_col(),
                words: Lines::parse(&text).words(),
            });
        };
        use_hook({
            let editable = editable;
            move || {
                publish(&editable);
                a11y.request_focus();
                // the keyboard is the note's now, and so is ⌘W (`View::pane`):
                // the graph's round trip and ⌘O remount it with no press
                let mut pane = app.pane;
                pane.set_if_modified(crate::index::DocKind::Note);
            }
        });

        // write the buffer if it moved since the last write. A refused write
        // (the file changed under us) goes to one stable `-unsaved` file.
        let save = {
            let path = path.clone();
            let rel = rel.clone();
            move || -> Option<Saved> {
                let mut stamp = stamp;
                let mut saved_at = saved_at;
                let mut dirty = app.dirty;
                let mut rescued = rescued;
                let generation = *edits.read();
                if generation == *saved_at.read() || loaded.read().is_none() {
                    return None;
                }
                let text = editable.editor().read().text().to_string();
                let expected = *stamp.read();
                let result = match vault::write_checked(&path, &text, expected) {
                    Ok(new) => {
                        stamp.set(new);
                        // the graph, the backlinks and the cards hear about it
                        let change = app.index.peek().note_written(&rel, &text);
                        if let Some(change) = change {
                            let mut index = app.index;
                            index.write().apply(&rel, change);
                        }
                        // and the section cards, whatever the index made of it
                        let mut rev = app.note_rev;
                        *rev.write() += 1;
                        Saved::Wrote
                    }
                    Err(_) => {
                        let target = rescued.read().clone().unwrap_or_else(|| {
                            let dir = path.parent().unwrap_or(&path);
                            vault::unique_path(
                                dir,
                                &format!("{}-unsaved", vault::name(&path)),
                                "md",
                            )
                        });
                        match vault::write(&target, &text) {
                            Ok(()) => {
                                rescued.set(Some(target.clone()));
                                Saved::Rescued(target)
                            }
                            Err(e) => Saved::Failed(e.to_string()),
                        }
                    }
                };
                // ⚠️ a failed write is still unsaved: the next flush tries again
                if !matches!(result, Saved::Failed(_)) {
                    saved_at.set(generation);
                    dirty.write().remove(&rel);
                }
                Some(result)
            }
        };
        let report = move |s: Option<Saved>, explicit: bool| match s {
            Some(Saved::Wrote) if explicit => app.toast("workspace saved", "just now", None),
            Some(Saved::Rescued(p)) => {
                app.toast(
                    "the file changed on disk",
                    format!("kept yours in {}", vault::display_name(&p)),
                    None,
                );
                app.reindex();
            }
            Some(Saved::Failed(e)) => app.toast("not saved", e, None),
            _ => {}
        };
        {
            let save = save.clone();
            FLUSH.with(move |f| {
                *f.borrow_mut() = Some(Box::new(move || {
                    let saved = save();
                    let why = match &saved {
                        Some(Saved::Failed(e)) => Some(e.clone()),
                        _ => None,
                    };
                    report(saved, false);
                    why
                }))
            });
        }
        use_drop(|| FLUSH.with(|f| *f.borrow_mut() = None));

        // every edit: mark dirty, and write after 1.5 s of quiet
        let touch = {
            let save = save.clone();
            let rel = rel.clone();
            move || {
                let mut edits = edits;
                let mut dirty = app.dirty;
                *edits.write() += 1;
                if !dirty.read().contains(&rel) {
                    dirty.write().insert(rel.clone());
                }
                let generation = *edits.read();
                let save = save.clone();
                spawn(async move {
                    freya::prelude::timer(std::time::Duration::from_millis(1500)).await;
                    if *edits.read() == generation {
                        report(save(), false);
                    }
                });
            }
        };
        // a click on a checkbox edits from inside a line: same path
        if *touched.read() != *seen_touch.read() {
            seen_touch.set(*touched.read());
            touch();
        }

        let on_key_down = {
            let save = save.clone();
            let touch = touch.clone();
            let rel = rel.clone();
            move |e: Event<KeyboardEventData>| {
                if !a11y.is_focused() || app.color_pick.read().is_some() {
                    return;
                }
                if crate::ctrl(&e) && e.code == Code::KeyS {
                    report(save(), true);
                    return;
                }
                // the shell's chords are not text
                if crate::ctrl(&e)
                    && matches!(
                        e.code,
                        Code::KeyK
                            | Code::KeyP
                            | Code::KeyN
                            | Code::KeyO
                            | Code::KeyG
                            | Code::KeyW
                            | Code::Digit1
                            | Code::Digit2
                            | Code::Digit3
                            | Code::Digit4
                            | Code::KeyL
                    )
                {
                    return;
                }
                // a chord the editor doesn't own is never text
                if crate::ctrl(&e)
                    && matches!(e.key, Key::Character(_))
                    && !matches!(
                        e.code,
                        Code::KeyA | Code::KeyC | Code::KeyX | Code::KeyV | Code::KeyZ | Code::KeyY
                    )
                    && sheet_key(&e, &editable).is_none()
                {
                    return;
                }
                // ⌘V with a picture on the clipboard: the picture, a line of its
                // own — the editor's paste only knows text. ⚠️ past the focus
                // guard above: the board answers ⌘V only while IT has focus, so
                // a paste here never lands there too
                if crate::ctrl(&e)
                    && e.code == Code::KeyV
                    && let Some(pictures) = clipboard_pictures(app, &rel)
                {
                    e.prevent_default();
                    let ed = editable.editor().read();
                    let (text, row) = (ed.text().to_string(), ed.cursor_row());
                    drop(ed);
                    apply(&mut editable, &picture_paste(&text, row, &pictures));
                    publish(&editable);
                    touch();
                    app.reindex();
                    return;
                }
                let before = editable.editor().read().text().to_string();
                // the sheet's own keys first: lists, tables, fences, tasks, formats
                if matches!(e.key, Key::Named(NamedKey::Tab)) {
                    // ⇥ belongs to the sheet, not to focus navigation
                    e.prevent_default();
                }
                if let Some(edit) = sheet_key(&e, &editable) {
                    e.prevent_default();
                    apply(&mut editable, &edit);
                    publish(&editable);
                    touch();
                    return;
                }
                // ⌘←/⌘→/⌘↑/⌘↓ are the Mac's Home/End
                let (key, modifiers) =
                    crate::keys::line_ends(&e.key, e.modifiers, crate::keys::is_mac())
                        .unwrap_or_else(|| (e.key.clone(), e.modifiers));
                editable.process_event(EditableEvent::KeyDown {
                    key: &key,
                    modifiers,
                    editor_line: None,
                    holder: None,
                });
                publish(&editable);
                let mut follow = follow;
                follow.set(true);
                if *editable.editor().read().text() != before {
                    touch();
                }
                // `[[` asks what the mention points at
                if matches!(&e.key, Key::Character(ch) if ch == "[") {
                    let ed = editable.editor().read();
                    let text = ed.text().to_string();
                    let at = byte_at(&text, ed.cursor_pos());
                    drop(ed);
                    if text[..at].ends_with("[[") {
                        let mut o = app.overlay;
                        o.set(crate::app::Overlay::Link);
                    }
                }
            }
        };
        // pictures dropped on the note go in as lines of their own: where they
        // land on the sheet (`gap`), or under the caret's line off it
        let take_drop = {
            let touch = touch.clone();
            let rel = rel.clone();
            move |e: &Event<FileEventData>, gap: Option<usize>| {
                let (pictures, rest): (Vec<_>, Vec<_>) = e
                    .file_paths
                    .iter()
                    .cloned()
                    .partition(|p| vault::is_image(&p.to_string_lossy()));
                if pictures.is_empty() {
                    return; // the window's, as before
                }
                e.stop_propagation();
                let files: Vec<String> = pictures
                    .iter()
                    .filter_map(|p| crate::absorb(app, p, &rel))
                    .collect();
                if !files.is_empty() {
                    let ed = editable.editor().read();
                    let (text, row) = (ed.text().to_string(), ed.cursor_row());
                    drop(ed);
                    let mut editable = editable;
                    apply(
                        &mut editable,
                        &picture_drop(&text, gap.unwrap_or(row + 1), &files),
                    );
                    publish(&editable);
                    touch();
                }
                crate::take_files(app, &rest);
            }
        };
        let sheet_drop = take_drop.clone();

        let on_key_up = move |e: Event<KeyboardEventData>| {
            editable.process_event(EditableEvent::KeyUp { key: &e.key });
        };

        // the format bar asked for something
        let asked = *fmt_request.read();
        if let Some(f) = asked {
            fmt_request.set(None);
            if let Some(edit) = fmt_edit(f, &editable) {
                apply(&mut editable, &edit);
                touch();
            }
            a11y.request_focus();
        }
        // the `[[` picker answered: write it where the caret is
        let pending = app.insert.read().clone();
        if let Some(ins) = pending {
            let mut i = app.insert;
            i.set(None);
            let ed = editable.editor().read();
            let text = ed.text().to_string();
            let at = byte_at(&text, ed.cursor_pos());
            drop(ed);
            apply(
                &mut editable,
                &crate::format::Edit {
                    range: at..at,
                    text: ins.clone(),
                    caret: at + ins.len(),
                },
            );
            a11y.request_focus();
            touch();
        }
        // a `[[note#section]]` asked for a place in this note. ⚠️ the BUFFER,
        // not the file: a note with unwritten words is the one most likely to
        // be linked into.
        let asked = app.land.read().clone();
        let mut landed = None;
        if let Some((_, section)) = asked.filter(|(to, _)| *to == rel) {
            let mut l = app.land;
            l.set(None);
            let text = editable.editor().read().text().to_string();
            match landing(&text, &section) {
                Some(at) => {
                    // the click on the link must not drag a selection from here
                    editable.process_event(EditableEvent::Release);
                    let mut ed = editable.editor_mut().write();
                    ed.clear_selection();
                    ed.move_cursor_to(at);
                    landed = Some(ed.cursor_row());
                    drop(ed);
                    publish(&editable);
                    a11y.request_focus();
                }
                None => app.toast(no_section(&vault::name(&path), &section), "", None),
            }
        }
        let text = editable.editor().read().text().to_string();
        let kinds = classify(&text);
        let lines: Vec<&str> = text.split('\n').collect();
        let (cursor_row, cursor_col) = {
            let ed = editable.editor().read();
            (ed.cursor_row(), ed.cursor_col())
        };
        let selections: Vec<_> = {
            let ed = editable.editor().read();
            (0..kinds.len())
                .map(|i| ed.get_visible_selection(EditorLine::Paragraph(i)))
                .collect()
        };
        let focused = a11y.is_focused();
        let query = app.find.read().clone().unwrap_or_default();
        let hits: Vec<Vec<(usize, usize)>> = lines.iter().map(|l| find_in(l, &query)).collect();
        let flat: Vec<(usize, usize, usize)> = hits
            .iter()
            .enumerate()
            .flat_map(|(i, v)| v.iter().map(move |(a, b)| (i, *a, *b)))
            .collect();
        let total = flat.len();
        let current = (total > 0).then(|| flat[*app.find_at.read() % total]);
        let dirty = app.dirty.read().contains(&rel);
        let tone = app.index.read().tone(&rel);
        let color = c().tone(tone);
        let broken = loaded.read().is_none();

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(c().surface_panel)
            .a11y_id(a11y)
            .a11y_focusable(true)
            .a11y_role(AccessibilityRole::MultilineTextInput)
            .on_global_key_down(on_key_down)
            .on_global_key_up(on_key_up)
            .on_global_pointer_up(move |_| editable.process_event(EditableEvent::Release))
            .on_file_drop(move |e: Event<FileEventData>| take_drop(&e, None))
            .child(
                // the note's spine: also the colour control
                rect()
                    .position(Position::new_absolute().left(-3.).top(0.))
                    .height(Size::fill())
                    .layer(Layer::Relative(1))
                    .child(crate::Spine {
                        key: rel.clone(),
                        tone,
                        height: Size::fill(),
                        glow: true,
                    }),
            )
            .maybe_child(
                (*app.view.read() == crate::app::View::Notes)
                    .then(|| crate::rail::TopTabs {}.into_element()),
            )
            .child(breadcrumb(app, &rel, dirty, color))
            .child(format_bar(fmt_request, fmt_menu))
            .maybe_child(
                app.find
                    .read()
                    .is_some()
                    .then(|| FindBar { total }.into_element()),
            )
            .maybe_child(broken.then(|| {
                rect()
                    .width(Size::fill())
                    .padding((8., 16.))
                    .background(c().danger_dim)
                    .child(
                        label()
                            .text("this note did not load — nothing will be saved over it")
                            .font_size(TEXT_SM)
                            .color(c().danger),
                    )
            }))
            .child({
                // only the lines on screen are built and laid out; the rest are
                // just heights, computed from the mono advance
                let sheet = Rc::new(Sheet {
                    lines: lines
                        .iter()
                        .map(|l| l.trim_end_matches('\r').to_string())
                        .collect(),
                    kinds,
                    selections,
                    hits,
                    current,
                    cursor: focused.then_some((cursor_row, cursor_col)),
                    editable,
                    caret,
                    touched,
                });
                let heights: Rc<Vec<f32>> = Rc::new(
                    (0..sheet.lines.len())
                        .map(|i| line_height(&sheet, i, *width.read() - PAD_LEFT - PAD_RIGHT))
                        .collect(),
                );
                let n = sheet.lines.len();
                // the caret stays on screen while typing or moving
                if *follow.read() {
                    let mut follow = follow;
                    follow.set(false);
                    if let Some((row, _)) = sheet.cursor {
                        let top = PAD_TOP + heights[..row.min(heights.len())].iter().sum::<f32>();
                        let bottom = top + heights.get(row).copied().unwrap_or(20.);
                        let (_, sy): (i32, i32) = scroll.into();
                        let (from, h) = (-sy as f32, *view_h.read());
                        if top < from + 8. {
                            scroll.scroll_to_y(-((top - 16.).max(0.) as i32));
                        } else if bottom > from + h - 8. {
                            scroll.scroll_to_y(-((bottom - h + 32.).max(0.) as i32));
                        }
                    }
                }
                // a landing reads from the top: its line goes there, and is
                // held there while the pane's width still moves — a note opened
                // from the canvas glides in from nothing, and the heights on
                // the way put the line somewhere else. Let go once the width
                // has held still past the glide.
                let (w, pinned) = (*width.read(), *pin.read());
                if let Some(row) = held_row(landed, pinned, w) {
                    pin.set(Some((row, w)));
                    let top = PAD_TOP + heights[..row.min(heights.len())].iter().sum::<f32>();
                    scroll.scroll_to_y(-((top - 16.).max(0.) as i32));
                    // ⚠️ a generation, not the width: a divider dragged back,
                    // or a glide reversed, revisits a width while still moving,
                    // and the older timer must not let go for it
                    let g = *pin_gen.peek() + 1;
                    pin_gen.set(g);
                    spawn(async move {
                        freya::prelude::timer(std::time::Duration::from_millis(SETTLE_MS)).await;
                        if *pin_gen.peek() == g {
                            pin.set(None);
                        }
                    });
                }
                // ⌘F: bring the current hit to the upper third
                let target = sheet
                    .current
                    .map(|(l, _, _)| PAD_TOP + heights[..l].iter().sum::<f32>());
                if target != *scrolled_to.read() {
                    scrolled_to.set(target);
                    if let Some(y) = target {
                        scroll.scroll_to_y(-((y - 160.).max(0.) as i32));
                    }
                }
                let below = tail(*view_h.read(), PAD_TOP + heights.iter().sum::<f32>());
                let size = {
                    let heights = heights.clone();
                    move |i: usize| match i {
                        0 => PAD_TOP,
                        i if i > n => below,
                        i => heights[i - 1],
                    }
                };
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .on_sized(move |e: Event<SizedEventData>| {
                        let w = e.area.width();
                        if (*width.read() - w).abs() > 0.5 {
                            width.set(w);
                        }
                        let h = e.area.height();
                        if (*view_h.read() - h).abs() > 0.5 {
                            view_h.set(h);
                        }
                        sheet_top.set_if_modified(e.area.min_y());
                    })
                    .on_file_drop({
                        let heights = heights.clone();
                        move |e: Event<FileEventData>| {
                            let (_, y) = crate::file_cursor(&e);
                            let (_, sy): (i32, i32) = scroll.into();
                            let down = y as f32 - *sheet_top.read() - sy as f32;
                            sheet_drop(&e, Some(gap_at(&heights, down)));
                        }
                    })
                    .on_mouse_down(move |e: Event<MouseEventData>| {
                        a11y.request_focus();
                        // the right click that opened the menu bubbles here too
                        let mut m = app.note_menu;
                        if e.button != Some(MouseButton::Right) && m.read().is_some() {
                            m.set(None);
                        }
                    })
                    .child(
                        VirtualScrollView::new_with_data_controlled(
                            sheet,
                            |item, sheet: &Rc<Sheet>| {
                                let row = rect()
                                    .key(item.index)
                                    .width(Size::fill())
                                    .height(Size::px(item.size));
                                let i = item.index;
                                if i > sheet.lines.len() {
                                    // under the last line: a click goes to the note's end
                                    let mut editable = sheet.editable;
                                    let mut caret = sheet.caret;
                                    return row
                                        .on_mouse_down(move |e: Event<MouseEventData>| {
                                            if e.button == Some(MouseButton::Right) {
                                                return;
                                            }
                                            let mut ed = editable.editor_mut().write();
                                            to_end(&mut *ed, crate::app::shift_held());
                                            let (row, col) = (ed.cursor_row(), ed.cursor_col());
                                            drop(ed);
                                            let mut c = caret.write();
                                            c.row = row;
                                            c.col = col;
                                        })
                                        .into_element();
                                }
                                if i == 0 {
                                    return row.into_element();
                                }
                                let i = i - 1;
                                let cursor = sheet.cursor.filter(|(r, _)| *r == i);
                                row.padding((0., PAD_RIGHT, 0., PAD_LEFT))
                                    .overflow(Overflow::Clip)
                                    .child(SheetLine {
                                        index: i,
                                        kind: sheet.kinds[i],
                                        prev: if i > 0 {
                                            sheet.kinds[i - 1]
                                        } else {
                                            Kind::Blank
                                        },
                                        table: (sheet.kinds[i] == Kind::Table).then(|| {
                                            rich::table_row(
                                                &sheet.lines,
                                                |k| {
                                                    matches!(
                                                        sheet.kinds[k],
                                                        Kind::Table | Kind::TableRule
                                                    )
                                                },
                                                i,
                                            )
                                        }),
                                        text: sheet.lines[i].clone(),
                                        highlights: match sheet.selections[i] {
                                            Some(sel) => vec![sel],
                                            None => sheet.hits[i].clone(),
                                        },
                                        search: sheet.selections[i].is_none()
                                            && !sheet.hits[i].is_empty(),
                                        current: sheet
                                            .current
                                            .filter(|(l, _, _)| *l == i)
                                            .map(|(_, a, b)| (a, b)),
                                        editable: sheet.editable,
                                        revealed: cursor.is_some(),
                                        cursor: cursor.map(|(_, c)| c),
                                        caret: sheet.caret,
                                        touched: sheet.touched,
                                    })
                                    .into_element()
                            },
                            scroll,
                        )
                        .length(n + 2)
                        .item_size(size)
                        .drag_scrolling(false)
                        .scroll_with_arrows(false),
                    )
            })
    }
}

pub fn empty_pane(app: App) -> impl IntoElement {
    // the mock's "no documents yet" is an empty vault; with notes on disk
    // but none open, ⌘P is the useful hint
    let vault_empty = app.index.read().docs.is_empty();
    rect()
        .expanded()
        .background(c().surface_panel)
        .center()
        .spacing(16.)
        .padding(32.)
        .child(
            label()
                .text(if vault_empty {
                    "no documents yet"
                } else {
                    "no note open"
                })
                .font_size(TEXT_MD)
                .font_weight(FontWeight::MEDIUM),
        )
        .child(
            rect()
                .spacing(8.)
                .font_size(TEXT_SM)
                .color(c().text_muted)
                .child(hint_row("⌘N", "new note"))
                .child(hint_row("⌘K", "command palette"))
                .child(if vault_empty {
                    hint_row("⌘O", "open a folder of .md files")
                } else {
                    hint_row("⌘P", "open a note")
                }),
        )
        .child(
            rect()
                .background(c().accent)
                .corner_radius(4.)
                .padding((6., 16.))
                .on_press(move |_| crate::new_note(app, None))
                .child(
                    label()
                        .text("new note")
                        .font_size(TEXT_SM)
                        .font_weight(FontWeight::MEDIUM)
                        .color(c().on_accent),
                ),
        )
        .child(
            label()
                .text("or drag markdown files anywhere in this window")
                .font_size(TEXT_XS)
                .color(c().text_faint),
        )
}

fn hint_row(k: &'static str, what: &'static str) -> impl IntoElement {
    rect()
        .horizontal()
        .spacing(8.)
        .cross_align(Alignment::Center)
        .child(
            rect()
                .min_width(Size::px(34.))
                .background(c().surface_inset)
                .border(Border::new().fill(c().border_subtle).width(1.))
                .corner_radius(3.)
                .padding((1., 6.))
                .center()
                .child(key_hint(k)),
        )
        .child(label().text(what))
}

/// `vault / auth / auth-flow.md •` — each folder with its colour line, the
/// name editable in place (click it, or straight after ⌘N). The file has a
/// line of its own only when its colour isn't the one its folder hands down.
fn breadcrumb(app: App, rel: &str, dirty: bool, color: Color) -> impl IntoElement {
    let parts: Vec<&str> = rel.split('/').collect();
    let (folders, file) = parts.split_at(parts.len() - 1);
    let file = file[0].to_string();
    let index = app.index.read();
    let mut acc = String::new();
    // spine + name, as in the mock: 2x10 line (45% when the colour is
    // inherited), 6px gap, 1px 4px padding; a click opens the tree
    let crumb = move |name: String, line: Option<Color>| {
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .padding((1., 4.))
            .corner_radius(3.)
            .on_press(move |_| {
                let mut tree = app.tree;
                tree.set(true);
            })
            .maybe_child(line.map(|line| {
                rect()
                    .width(Size::px(2.))
                    .height(Size::px(10.))
                    .background(line)
            }))
            .child(label().text(name).color(c().text_muted))
            .into_element()
    };
    // the vault is no group: its crumb has no line
    let mut crumbs: Vec<Element> = vec![crumb("vault".into(), None)];
    for f in folders {
        acc.push_str(f);
        acc.push('/');
        // ⚠️ its own colour, not "inherits from nobody": an uncoloured folder
        // has no ancestor to inherit from either, and fades like an inherited one
        let own = index.own.contains_key(&acc);
        let line = c().tone(index.tone(&acc));
        crumbs.push(label().text("/").color(c().text_faint).into_element());
        crumbs.push(crumb(
            f.to_string(),
            Some(scaled(line, if own { 1. } else { 0.45 })),
        ));
    }
    let tone = index.tone(rel);
    let mark = (tone != index.handed_down(rel)).then(|| c().tone(tone));
    drop(index);
    crumbs.push(label().text("/").color(c().text_faint).into_element());
    if let Some(mark) = mark {
        crumbs.push(
            rect()
                .width(Size::px(2.))
                .height(Size::px(10.))
                .background(mark)
                .into_element(),
        );
    }
    let naming = app.naming.read().as_deref() == Some(rel);
    crumbs.push(
        NameCrumb {
            rel: rel.to_string(),
            file,
            naming,
            dot: dirty.then_some(color),
        }
        .into_element(),
    );

    let rel_del = rel.to_string();
    let rel_ren = rel.to_string();
    rect()
        .width(Size::fill())
        .height(Size::px(34.))
        .horizontal()
        .cross_align(Alignment::Center)
        .padding((0., 16.))
        .spacing(6.)
        .content(Content::Flex)
        // ⚠️ 1.5 and strong, not the 1px subtle `design/Workspace.dc.html`
        // draws under the note's crumb row: on a 1x monitor that line vanished
        // and the crumbs ran into the format bar (Weber, 2026-09-05)
        .border(Border::new().fill(c().border_strong).width(BorderWidth {
            bottom: 1.5,
            ..Default::default()
        }))
        .font_size(TEXT_SM)
        .children(crumbs)
        .child(
            IconButton::new(icons::PENCIL, 14., c().surface_raised, false).on(move || {
                let mut naming = app.naming;
                naming.set(Some(rel_ren.clone()));
            }),
        )
        .child(
            IconButton::new(icons::TRASH, 14., c().danger_dim, true)
                .on(move || app.trash(std::slice::from_ref(&rel_del))),
        )
        .child(
            rect()
                .width(Size::px(1.))
                .height(Size::px(12.))
                .margin((0., 4.))
                .background(c().border_subtle),
        )
        .child(tag("MD"))
}

/// The file name in the breadcrumb: a field while it's being named.
#[derive(PartialEq)]
struct NameCrumb {
    rel: String,
    file: String,
    naming: bool,
    /// The unsaved dot, which follows the name.
    dot: Option<Color>,
}

impl Component for NameCrumb {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let (stem, ext) = match self.file.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), format!(".{e}")),
            None => (self.file.clone(), String::new()),
        };
        let value = use_state(|| stem.clone());
        let mut room = use_state(|| f32::MAX);
        let name: Element = if !self.naming {
            let rel = self.rel.clone();
            // ⚠️ the extension and the dot never shrink: the name takes what's
            // left and ends in an ellipsis. Unbounded, a long name wrapped
            // and squeezed everything after it to one character per line.
            let taken = ext.chars().count() as f32 * advance(TEXT_SM)
                + if self.dot.is_some() { 5. + 6. } else { 0. }
                + 6.;
            rect()
                .horizontal()
                .on_press(move |_| {
                    let mut naming = app.naming;
                    naming.set(Some(rel.clone()));
                })
                .child(
                    label()
                        .text(stem)
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .max_width(Size::px((*room.read() - taken).max(0.)))
                        .font_weight(FontWeight::MEDIUM)
                        .color(c().text_primary),
                )
                .child(
                    label()
                        .text(ext)
                        .max_lines(1)
                        .font_weight(FontWeight::MEDIUM)
                        .color(c().text_primary),
                )
                .into_element()
        } else {
            let rel = self.rel.clone();
            let own = rel.clone();
            // ⚠️ the field is the flex child and the extension keeps its own
            // width: with `fill` the field took all 220px and the ".md" was
            // left none, one character per line
            rect()
                .width(Size::px(
                    // room holds the dot and its gap too
                    220f32.min(*room.read() - if self.dot.is_some() { 11. } else { 0. }),
                ))
                .horizontal()
                .content(Content::Flex)
                .cross_align(Alignment::Center)
                .child(
                    rect().width(Size::flex(1.)).overflow(Overflow::Clip).child(
                        Field::new(
                            value,
                            move |v: String| {
                                let mut naming = app.naming;
                                match crate::rename(app, &rel, &v) {
                                    Ok(_) => naming.set(None),
                                    Err(msg) => {
                                        app.toast(msg, "names that already exist are refused", None)
                                    }
                                }
                            },
                            move |_| {
                                // ⚠️ only its own: the field also cancels as it
                                // goes, and by then another rename may have begun
                                let mut naming = app.naming;
                                if naming.peek().as_deref() == Some(own.as_str()) {
                                    naming.set(None);
                                }
                            },
                        )
                        .select_all()
                        .blur_cancels()
                        .weight(FontWeight::MEDIUM),
                    ),
                )
                .child(label().text(ext).max_lines(1).color(c().text_faint))
                .into_element()
        };
        // the whole remaining width of the breadcrumb: that is what bounds the name
        rect()
            .width(Size::flex(1.))
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .on_sized(move |e: Event<SizedEventData>| {
                let w = e.area.width();
                if (*room.read() - w).abs() > 0.5 {
                    room.set(w);
                }
            })
            .child(name)
            .maybe_child(self.dot.map(|dot| {
                rect()
                    .width(Size::px(5.))
                    .height(Size::px(5.))
                    .corner_radius(999.)
                    .background(dot)
            }))
            .into_element()
    }
}

const PAD_TOP: f32 = 32.;
const PAD_BOTTOM: f32 = 24.;
const PAD_LEFT: f32 = 38.;
const PAD_RIGHT: f32 = 32.;

/// The room under the last line: what is left of the view, so a click
/// anywhere below the text lands on it — never less than the bottom pad.
fn tail(view_h: f32, above: f32) -> f32 {
    // whole pixels: a note that fits must not scroll by a rounding error
    (view_h - above).floor().max(PAD_BOTTOM)
}

/// Which gap between lines a drop `y` px down the sheet's content (scroll
/// included) lands in: 0 above the first line, `heights.len()` under the
/// last. The nearer edge of the line under it wins.
fn gap_at(heights: &[f32], y: f32) -> usize {
    let mut top = PAD_TOP;
    for (i, h) in heights.iter().enumerate() {
        if y < top + h / 2. {
            return i;
        }
        top += h;
    }
    heights.len()
}

/// Pictures dropped on the note: each `![](file)` a line of its own, put in
/// before line `gap` (the line count: after the last). The caret lands on the
/// line after them.
fn picture_drop(text: &str, gap: usize, files: &[String]) -> crate::format::Edit {
    let pictures: Vec<String> = files.iter().map(|f| format!("![]({f})")).collect();
    lines_at(text, gap, &pictures)
}

/// ⌘V's pictures, each a line of its own at the caret: on the caret's line
/// when it is blank, under it when it holds words — never through a sentence.
fn picture_paste(text: &str, row: usize, pictures: &[String]) -> crate::format::Edit {
    let blank = text
        .split('\n')
        .nth(row)
        .is_some_and(|l| l.trim().is_empty());
    lines_at(text, if blank { row } else { row + 1 }, pictures)
}

/// Text that is nothing but picture lines — what a picture card copied off
/// the board is (`Scene::copy_text`). Anything else is the editor's paste.
fn picture_lines(clip: &str) -> Option<Vec<String>> {
    let lines: Vec<String> = clip
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let all = lines
        .iter()
        .all(|l| synapso_core::doc::image_line(l).is_some());
    (all && !lines.is_empty()).then_some(lines)
}

/// The pictures ⌘V brings into the note at `rel`, as picture lines over files
/// in the vault: image files copied in Finder (one already in the vault is
/// used where it is, the drop's rule), else a picture's pixels — a screenshot,
/// a browser's "Copy Image" — written as `assets/pasted-N.png` beside the
/// note, else text that is only pictures. `None` is the editor's own paste.
fn clipboard_pictures(app: App, rel: &str) -> Option<Vec<String>> {
    if let Some(files) = vault::clipboard_files() {
        // ⚠️ never the pixels after files: those are the file's icon
        let pictures: Vec<String> = files
            .iter()
            .filter(|p| vault::is_image(&p.to_string_lossy()))
            .filter_map(|p| crate::absorb(app, p, rel))
            .map(|f| format!("![]({f})"))
            .collect();
        return (!pictures.is_empty()).then_some(pictures);
    }
    let folder = crate::index::parent_folder(rel);
    if let Ok(file) = vault::paste_image(&app.root().join(&folder)) {
        return Some(vec![format!("![]({folder}{file})")]);
    }
    picture_lines(&freya::clipboard::Clipboard::get().ok()?)
}

/// `lines`, each a line of its own, put in before line `gap` (the line count:
/// after the last). The caret lands on the line after them.
fn lines_at(text: &str, gap: usize, lines: &[String]) -> crate::format::Edit {
    let at = text
        .split('\n')
        .take(gap)
        .map(|line| line.len() + 1)
        .sum::<usize>()
        .min(text.len());
    let mut lines: String = lines.iter().map(|l| format!("{l}\n")).collect();
    // under a last line that has no newline of its own
    if at == text.len() && !text.is_empty() && !text.ends_with('\n') {
        lines.insert(0, '\n');
    }
    crate::format::Edit {
        range: at..at,
        caret: at + lines.len(),
        text: lines,
    }
}

/// A click under the last line: the caret at the very end, nothing selected —
/// or with ⇧, the selection stretched there from where it was anchored, as a
/// ⇧-click on a line does.
fn to_end(ed: &mut impl TextEditor, extend: bool) {
    let end = ed.len_utf16_cu();
    let anchor = match *ed.selection() {
        TextSelection::Cursor(at) => at,
        TextSelection::Range { from, .. } => from,
    };
    *ed.selection_mut() = if extend && anchor != end {
        TextSelection::new_range((anchor, end))
    } else {
        TextSelection::new_cursor(end)
    };
}

/// What the virtual list needs to build any visible line.
#[derive(PartialEq)]
struct Sheet {
    lines: Vec<String>,
    kinds: Vec<Kind>,
    selections: Vec<Option<(usize, usize)>>,
    /// ⌘F hits per line (UTF-16 ranges), and the current one.
    hits: Vec<Vec<(usize, usize)>>,
    current: Option<(usize, usize, usize)>,
    cursor: Option<(usize, usize)>,
    editable: UseEditable,
    caret: State<Caret>,
    touched: State<u64>,
}

/// JetBrains Mono advances 600/1000 em, so a line's wrap is arithmetic.
fn advance(size: f32) -> f32 {
    size * 0.6
}

/// Greedy word wrap, the way the paragraph will do it, in rows.
fn rows(text: &str, cols: usize) -> usize {
    let cols = cols.max(1);
    let mut rows = 1;
    let mut col = 0;
    for word in text.split_inclusive(' ') {
        let w = word.chars().count();
        let ink = word.trim_end().chars().count();
        if col > 0 && col + ink > cols {
            rows += 1;
            col = 0;
        }
        if w > cols {
            rows += (col + w) / cols;
            col = (col + w) % cols;
        } else {
            col += w;
        }
    }
    rows
}

/// Where a click on a folded line leaves the caret (a UTF-16 column): never
/// inside the syntax nobody can see. All of an empty `- [ ] ` is that syntax,
/// so a click beside its box lands where its words would start.
fn click_col(kind: Kind, line: &str, col: usize) -> usize {
    col.max(u16_at(line, prefix_len(kind, line)))
}

/// ⚠️ a folded line with no words is all hidden syntax: 0.1px glyphs, a
/// paragraph too thin to click. So a paragraph is at least this tall (`row`
/// is one row of its text): a line whose row is as tall as its text gets one
/// row, and a line that is only ever syntax — a rule, a fence — its whole
/// row, the height `measure_line` gave it.
fn click_floor(kind: Kind, line: &str, revealed: bool, row: f32) -> Option<f32> {
    match kind {
        Kind::Heading(_)
        | Kind::Task(_)
        | Kind::Bullet
        | Kind::Ordered
        | Kind::Quote
        | Kind::Footnote
        | Kind::Para => Some(row),
        Kind::Rule | Kind::FenceOpen | Kind::FenceClose => {
            Some(measure_line(kind, line, revealed, false, 0.))
        }
        _ => None,
    }
}

/// The text the reader sees on a line: block and inline markers fold away
/// unless the line is being edited.
fn shown(kind: Kind, line: &str, revealed: bool) -> String {
    if revealed || matches!(kind, Kind::Code) {
        return line.to_string();
    }
    let cut = prefix_len(kind, line).min(line.len());
    rich::shown(&line[cut..])
}

/// Headings one to six: size, colour, tracking.
fn heading_style(level: u8) -> (f32, Color, f32) {
    match level {
        1 => (TEXT_XL, c().text_primary, -0.24),
        2 => (TEXT_LG, c().text_primary, 0.),
        3 => (TEXT_MD, c().text_primary, 0.),
        4 => (TEXT_BASE, c().text_primary, 0.),
        5 => (12., c().text_muted, 0.),
        _ => (TEXT_SM, c().text_faint, 1.),
    }
}

/// How deep a quote line sits: `> > x` is two.
fn quote_depth(line: &str) -> usize {
    line.trim_start()
        .chars()
        .take_while(|c| *c == '>' || *c == ' ')
        .filter(|c| *c == '>')
        .count()
        .max(1)
}

/// A code line's row, whole pixels: the 18.6 of 12px × 1.55 left the row's
/// background short of its slot, a faint line between every two lines.
const CODE_ROW: f32 = 19.;

/// An empty line's row. The same with the caret on it as without: it was
/// 2px taller while edited, and every line below jumped as the caret passed.
const BLANK_ROW: f32 = 12.;

/// The window's scale factor (1.25 and 1.5 on Windows).
fn scale() -> f32 {
    *Platform::get().current_window().scale_factor.read() as f32
}

/// A height that is a whole number of DEVICE pixels: layout multiplies by
/// the scale afterwards, so whole logical pixels still land mid-pixel at 125%.
fn snap(h: f32) -> f32 {
    let s = scale();
    (h * s).ceil() / s
}

/// The table rows are a fixed height so the cells can centre in them.
const TABLE_ROW: f32 = 26.;

/// A line's height, matching what [`SheetLine`] lays out. Cached by what
/// decides it: a keystroke changes one line, not three thousand.
fn line_height(sheet: &Sheet, i: usize, width: f32) -> f32 {
    type Key = (String, Kind, bool, bool, u32, u32);
    thread_local! {
        static HEIGHTS: std::cell::RefCell<std::collections::HashMap<Key, f32>> = std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let revealed = sheet.cursor.is_some_and(|(r, _)| r == i);
    let after_blank = i > 0 && sheet.kinds[i - 1] == Kind::Blank;
    let key: Key = (
        sheet.lines[i].clone(),
        sheet.kinds[i],
        revealed,
        after_blank,
        width.to_bits(),
        scale().to_bits(),
    );
    if let Some(h) = HEIGHTS.with(|c| c.borrow().get(&key).copied()) {
        return h;
    }
    // whole device pixels, so a block's rows never start mid-pixel: a
    // background that begins at y.15 antialiases a faint line into every
    // seam below it
    let h = snap(measure_line(
        sheet.kinds[i],
        &sheet.lines[i],
        revealed,
        after_blank,
        width,
    ));
    HEIGHTS.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 50_000 {
            c.clear();
        }
        c.insert(key, h);
    });
    h
}

/// A line's height from what decides it: the line and its kind, whether the
/// caret is on it, whether a blank line comes before it, the width.
fn measure_line(kind: Kind, line: &str, revealed: bool, after_blank: bool, width: f32) -> f32 {
    let text = shown(kind, line, revealed);
    let wrap = |size: f32, lh: f32, avail: f32| {
        rows(&text, (avail / advance(size)).floor() as usize) as f32 * (size * lh)
    };
    match kind {
        Kind::Heading(l) => {
            let (size, _, track) = heading_style(l);
            let top = if after_blank { 4. } else { 0. };
            let rows = rows(&text, (width / (advance(size) + track)).floor() as usize) as f32;
            top + rows * size * 1.25 + if l == 1 { 6. } else { 2. }
        }
        Kind::Blank => BLANK_ROW,
        Kind::Task(_) => 6. + wrap(TEXT_BASE, 1.55, width - 21. - nest(line)).max(13.),
        Kind::Bullet => 4. + wrap(TEXT_BASE, 1.55, width - 16. - nest(line)),
        Kind::Quote => 4. + wrap(TEXT_BASE, 1.55, width - 14. * quote_depth(line) as f32),
        Kind::Footnote => 4. + wrap(TEXT_SM, 1.55, width - 26. - 22.),
        Kind::TableRule if !revealed => 1.,
        Kind::TableRule => TABLE_ROW,
        Kind::FenceOpen | Kind::FenceClose if !revealed => 12.,
        Kind::Code | Kind::FenceOpen | Kind::FenceClose => CODE_ROW,
        Kind::Para => wrap(TEXT_BASE, 1.55, width),
        Kind::Ordered => 4. + wrap(TEXT_BASE, 1.55, width - 24. - nest(line)),
        Kind::Rule => 22.,
        Kind::Table => TABLE_ROW,
        Kind::Image => IMAGE_H + 22.,
    }
}

/// What the format bar asks the sheet to do.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fmt {
    Heading(u8),
    Paragraph,
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    CodeBlock,
    Quote,
    Bullet,
    Ordered,
    Task,
    Table,
}

/// A format menu: its name, and each entry with its label and chord.
type FmtMenu = (&'static str, &'static [(Fmt, &'static str, &'static str)]);

const MENUS: [FmtMenu; 4] = [
    (
        "heading",
        &[
            (Fmt::Heading(1), "Heading 1", "⌘⌥1"),
            (Fmt::Heading(2), "Heading 2", "⌘⌥2"),
            (Fmt::Heading(3), "Heading 3", "⌘⌥3"),
            (Fmt::Paragraph, "Paragraph", ""),
        ],
    ),
    (
        "format",
        &[
            (Fmt::Bold, "Bold", "⌘B"),
            (Fmt::Italic, "Italic", "⌘I"),
            (Fmt::Strike, "Strikethrough", "⌘⇧X"),
            (Fmt::Code, "Code", "⌘E"),
        ],
    ),
    (
        "blocks",
        &[
            (Fmt::CodeBlock, "Code block", "⌘⌥C"),
            (Fmt::Quote, "Blockquote", "⌘⇧."),
        ],
    ),
    (
        "lists",
        &[
            (Fmt::Ordered, "Ordered list", "⌘⇧7"),
            (Fmt::Bullet, "Bullet list", "⌘⇧8"),
            (Fmt::Task, "Task list", "⌘⇧9"),
        ],
    ),
];

/// The edit one format request makes at the caret / over the selection.
fn fmt_edit(f: Fmt, editable: &UseEditable) -> Option<crate::format::Edit> {
    use crate::format;
    use synapso_core::doc::Kind as DKind;
    let ed = editable.editor().read();
    let text = ed.text().to_string();
    let (from, to) = match ed.selection() {
        TextSelection::Cursor(p) => (*p, *p),
        TextSelection::Range { from, to } => ((*from).min(*to), (*from).max(*to)),
    };
    drop(ed);
    let caret = byte_at(&text, to);
    let sel = byte_at(&text, from)..caret;
    match f {
        Fmt::Heading(n) => format::set_kind(&text, caret, &DKind::Heading(n)),
        Fmt::Paragraph => format::set_kind(&text, caret, &DKind::Paragraph),
        Fmt::Bold => Some(format::wrap(&text, sel, format::Format::Bold)),
        Fmt::Italic => Some(format::wrap(&text, sel, format::Format::Italic)),
        Fmt::Strike => Some(format::wrap(&text, sel, format::Format::Strike)),
        Fmt::Code => Some(format::wrap(&text, sel, format::Format::Code)),
        Fmt::Link => Some(format::link(&text, sel)),
        Fmt::CodeBlock => Some(format::fence(&text, caret)),
        Fmt::Quote => format::set_kind(&text, caret, &DKind::Quote),
        Fmt::Bullet => format::set_kind(&text, caret, &DKind::Bullet),
        Fmt::Ordered => format::set_kind(&text, caret, &DKind::Ordered),
        Fmt::Task => format::set_kind(&text, caret, &DKind::Task(false)),
        Fmt::Table => Some(format::table(&text, caret)),
    }
}

fn format_bar(mut request: State<Option<Fmt>>, mut menu: State<Option<usize>>) -> impl IntoElement {
    let sep = || {
        rect()
            .width(Size::px(1.))
            .height(Size::px(14.))
            .margin((0., 4.))
            .background(c().border_subtle)
    };
    let open = |i: usize| {
        move || {
            let mut menu = menu;
            let cur = *menu.read();
            menu.set(if cur == Some(i) { None } else { Some(i) });
        }
    };
    let current_menu = *menu.read();
    // a menu button and, when it's the open one, its drop-down 4px beneath it
    let slot = |i: usize, icon: (&'static str, &'static [u8])| {
        let dropdown = (current_menu == Some(i)).then(|| {
            rect()
                .position(Position::new_absolute().left(0.).top(26.))
                .layer(Layer::Relative(6))
                .width(Size::px(menu_width(i) + 10.))
                .padding(4.)
                .spacing(1.)
                .background(c().surface_overlay)
                .border(Border::new().fill(c().border_strong).width(1.))
                .corner_radius(4.)
                .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                .children(MENUS[i].1.iter().map(|(f, l, k)| {
                    let f = *f;
                    rect()
                        .width(Size::fill())
                        .height(Size::px(29.))
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .spacing(16.)
                        .padding((6., 12.))
                        .corner_radius(3.)
                        .on_press(move |_| {
                            request.set(Some(f));
                            menu.set(None);
                        })
                        .child(
                            rect()
                                .width(Size::flex(1.))
                                .child(label().text(*l).font_size(TEXT_SM)),
                        )
                        .child(key_hint(k))
                        .into_element()
                }))
                .into_element()
        });
        rect()
            .child(
                IconButton::menu(icon)
                    .active(current_menu == Some(i))
                    .on(open(i)),
            )
            .maybe_child(dropdown)
    };
    rect()
        .width(Size::fill())
        .height(Size::px(30.))
        .horizontal()
        .cross_align(Alignment::Center)
        .padding((0., 12., 0., 14.))
        .spacing(2.)
        .border(Border::new().fill(c().border_subtle).width(BorderWidth {
            bottom: 1.,
            ..Default::default()
        }))
        .child(slot(0, icons::HEADING))
        .child(sep())
        .child(slot(1, icons::FORMAT))
        .child(sep())
        .child(
            IconButton::new(icons::LINK, 15., c().surface_raised, false)
                .on(move || request.set(Some(Fmt::Link))),
        )
        .child(sep())
        .child(slot(2, icons::BLOCKS))
        .child(sep())
        .child(slot(3, icons::LISTS))
        .child(sep())
        .child(
            IconButton::new(icons::TABLE, 15., c().surface_raised, false)
                .on(move || request.set(Some(Fmt::Table))),
        )
}

/// `col` at `k` of its own alpha. ⚠️ Freya's `with_a(a)` already multiplies by
/// the colour's alpha, so passing it `col.a() * k` squares the fade.
fn scaled(col: Color, k: f32) -> Color {
    col.with_af32(k)
}

/// A menu's content width: 170 at least, else its widest row. ⚠️ Freya has no
/// shrink-to-fit with a fill child, so this counts monospace cells (11px label
/// = 6.6, 10px key = 6) instead.
fn menu_width(i: usize) -> f32 {
    let row = |(_, l, k): &(Fmt, &str, &str)| {
        let key = k
            .strip_prefix('⌘')
            .map_or(k.chars().count() as f32 * 6., |r| {
                10. + r.chars().count() as f32 * 6.
            });
        l.chars().count() as f32 * 6.6 + 16. + key + 24.
    };
    MENUS[i].1.iter().map(row).fold(170., f32::max)
}

/// A menu entry's chord: the modifier as an icon, then the rest.
fn key_hint(k: &str) -> impl IntoElement {
    let (has_mod, rest) = match k.strip_prefix('⌘') {
        Some(rest) => (true, rest.to_string()),
        None => (false, k.to_string()),
    };
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .maybe_child(has_mod.then(|| crate::keys::modifier(10., c().text_faint)))
        .child(label().text(rest).font_size(TEXT_XS).color(c().text_faint))
}

/// A toolbar icon whose hover fades in rather than snapping.
#[derive(PartialEq)]
struct IconButton {
    src: (&'static str, &'static [u8]),
    size: f32,
    hover_bg: Color,
    danger: bool,
    caret: bool,
    /// Its menu is open: lit like a hover, caret included.
    active: bool,
    press: Option<EventHandler<()>>,
}

impl IconButton {
    fn new(src: (&'static str, &'static [u8]), size: f32, hover_bg: Color, danger: bool) -> Self {
        Self {
            src,
            size,
            hover_bg,
            danger,
            caret: false,
            active: false,
            press: None,
        }
    }
    fn on(mut self, mut f: impl FnMut() + 'static) -> Self {
        self.press = Some(EventHandler::new(move |_| f()));
        self
    }
    fn active(mut self, on: bool) -> Self {
        self.active = on;
        self
    }
    fn menu(src: (&'static str, &'static [u8])) -> Self {
        Self {
            src,
            size: 15.,
            hover_bg: c().surface_raised,
            danger: false,
            caret: true,
            active: false,
            press: None,
        }
    }
}

impl Component for IconButton {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let rest = if self.size < 15. {
            c().text_faint
        } else {
            c().text_muted
        };
        let lit = if self.danger {
            c().danger
        } else {
            c().text_primary
        };
        let hover_bg = self.hover_bg;
        let fade = use_animation_transition(use_memo(move || *hover.read()), move |from, to| {
            let f = |h: bool| if h { 1. } else { 0. };
            AnimNum::new(f(from), f(to)).time(120)
        });
        let t = if self.active { 1. } else { fade.read().value() };
        let bg = scaled(hover_bg, t);
        let fg = if t > 0.5 { lit } else { rest };
        rect()
            .height(Size::px(if self.size < 15. { 20. } else { 22. }))
            .padding((0., 4.))
            .horizontal()
            .center()
            .spacing(1.)
            .corner_radius(3.)
            .background(bg)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press({
                let press = self.press.clone();
                move |_| {
                    if let Some(p) = &press {
                        p.call(());
                    }
                }
            })
            .child(icons::icon(self.src, self.size, fg))
            .maybe_child(self.caret.then(|| {
                let caret = if self.active {
                    c().text_muted
                } else {
                    c().text_faint
                };
                label().text("▾").font_size(8.).color(caret)
            }))
    }
}

#[derive(PartialEq)]
struct SheetLine {
    index: usize,
    /// The line itself and its slice of the selection, handed down so a
    /// line that didn't change compares equal and isn't rebuilt.
    text: String,
    highlights: Vec<(usize, usize)>,
    search: bool,
    current: Option<(usize, usize)>,
    kind: Kind,
    prev: Kind,
    /// For a table row: its cells and the table's columns.
    table: Option<rich::TableRow>,
    editable: UseEditable,
    revealed: bool,
    cursor: Option<usize>,
    caret: State<Caret>,
    touched: State<u64>,
}

const HIDDEN: f32 = 0.1;

thread_local! {
    /// Where the last press that unfolded its line went down, in window
    /// coordinates; cleared once the pointer leaves it.
    static UNFOLDED_AT: std::cell::Cell<Option<(f64, f64)>> = const { std::cell::Cell::new(None) };
}

/// A press that unfolds a line moves the text under a pointer that hasn't
/// moved: the syntax comes back and pushes the words right, a line above
/// folds and pulls everything up. The first pixels of jitter would then
/// select whatever slid beneath, so near such a press the pointer has not
/// dragged yet.
const DRAG_SLOP: f64 = 4.;

/// Whether a move to `at` drags, given where an unfolding press went down.
fn drags(press: Option<(f64, f64)>, at: (f64, f64)) -> bool {
    press.is_none_or(|(x, y)| (at.0 - x).hypot(at.1 - y) > DRAG_SLOP)
}

/// Syntax that folds away: invisible and next to no width, but still in the
/// paragraph so every click lands on the right buffer offset.
fn marker(text: &str, revealed: bool) -> Span<'static> {
    if revealed {
        Span::new(text.to_string()).color(c().text_faint)
    } else {
        // no tracking either: a heading's letter spacing would still widen it
        Span::new(text.to_string())
            .font_size(HIDDEN)
            .letter_spacing(0.)
            .color(Color::TRANSPARENT)
    }
}

/// A line's inline runs (bold, code, links…) with their markers folded, and
/// what to paint behind them (UTF-16 ranges, counted from `at16`, where the
/// body starts in the paragraph). `struck` draws a line through all of it: a
/// done task.
fn inline_spans(
    body: &str,
    revealed: bool,
    base: Color,
    struck: bool,
    out: &mut Vec<Span<'static>>,
    decos: &mut Vec<(std::ops::Range<usize>, Deco)>,
    at16: usize,
) {
    let mut at = 0;
    let mut at16 = at16;
    let strike = |s: Span<'static>| {
        if struck {
            s.text_decoration(TextDecoration::LineThrough)
        } else {
            s
        }
    };
    let mut push =
        |text: &str, span: Span<'static>, deco: Option<Deco>, out: &mut Vec<Span<'static>>| {
            let n = text.encode_utf16().count();
            if let Some(d) = deco {
                decos.push((at16..at16 + n, d));
            }
            at16 += n;
            out.push(span);
        };
    for (range, style) in rich::spans(body) {
        if range.start > at {
            let gap = &body[at..range.start];
            push(
                gap,
                strike(Span::new(gap.to_string()).color(base)),
                None,
                out,
            );
        }
        let piece = &body[range.clone()];
        let text = piece.to_string();
        let (span, deco) = match style {
            Rich::Marker => (marker(piece, revealed), None),
            Rich::Bold => (
                strike(Span::new(text).font_weight(FontWeight::BOLD).color(base)),
                None,
            ),
            Rich::Italic => (
                strike(Span::new(text).font_slant(FontSlant::Italic).color(base)),
                None,
            ),
            Rich::BoldItalic => (
                strike(
                    Span::new(text)
                        .font_weight(FontWeight::BOLD)
                        .font_slant(FontSlant::Italic)
                        .color(base),
                ),
                None,
            ),
            Rich::Strike => (
                Span::new(text)
                    .text_decoration(TextDecoration::LineThrough)
                    .color(c().text_muted),
                None,
            ),
            Rich::Underline => (
                Span::new(text)
                    .text_decoration(TextDecoration::Underline)
                    .color(base),
                None,
            ),
            Rich::Code => (
                strike(Span::new(text).font_size(12.).color(c().text_primary)),
                Some(Deco::Chip),
            ),
            Rich::Highlight => (strike(Span::new(text).color(c().doc[1])), Some(Deco::Wash)),
            Rich::Wiki => (
                strike(Span::new(text).color(c().accent)),
                Some(Deco::Dashed),
            ),
            Rich::Link => (
                Span::new(text)
                    .text_decoration(TextDecoration::Underline)
                    .color(c().text_link),
                None,
            ),
            Rich::FootRef => (
                Span::new(text).font_size(TEXT_XS).color(c().text_link),
                None,
            ),
        };
        push(piece, span, deco, out);
        at = range.end;
    }
    if at < body.len() {
        let rest = &body[at..];
        push(
            rest,
            strike(Span::new(rest.to_string()).color(base)),
            None,
            out,
        );
    }
}

/// Just enough Rust colouring for the design's snippet.
fn code_spans(line: &str, out: &mut Vec<Span<'static>>) {
    const KEYWORDS: [&str; 30] = [
        "fn",
        "let",
        "mut",
        "pub",
        "impl",
        "struct",
        "use",
        "enum",
        "trait",
        "match",
        "if",
        "else",
        "for",
        "while",
        "return",
        "export",
        "interface",
        "const",
        "function",
        "type",
        "class",
        "new",
        "import",
        "from",
        "async",
        "await",
        "null",
        "void",
        "true",
        "false",
    ];
    const TYPES: [&str; 8] = [
        "str", "string", "number", "boolean", "u32", "usize", "i64", "f32",
    ];
    let (code, comment) = match line.find("//") {
        Some(i) => (&line[..i], Some(&line[i..])),
        None => (line, None),
    };
    let mut word = String::new();
    let mut in_str = false;
    let flush = |word: &mut String, out: &mut Vec<Span<'static>>| {
        if word.is_empty() {
            return;
        }
        let w = word.as_str();
        let c = if KEYWORDS.contains(&w) {
            c().doc[2]
        } else if TYPES.contains(&w) || w.chars().next().is_some_and(char::is_uppercase) {
            c().doc[3]
        } else {
            c().text_primary
        };
        out.push(Span::new(std::mem::take(word)).color(c));
    };
    for ch in code.chars() {
        if in_str {
            word.push(ch);
            if ch == '"' {
                out.push(Span::new(std::mem::take(&mut word)).color(c().doc[1]));
                in_str = false;
            }
        } else if ch == '"' {
            flush(&mut word, out);
            word.push(ch);
            in_str = true;
        } else if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            flush(&mut word, out);
            out.push(Span::new(ch.to_string()).color(c().text_primary));
        }
    }
    if in_str {
        out.push(Span::new(word).color(c().doc[1]));
    } else {
        flush(&mut word, out);
    }
    if let Some(comment) = comment {
        out.push(Span::new(comment.to_string()).color(c().text_faint));
    }
}

impl Component for SheetLine {
    fn render_key(&self) -> DiffKey {
        (&self.index).into()
    }

    fn render(&self) -> impl IntoElement {
        let index = self.index;
        let mut editable = self.editable;
        let mut caret = self.caret;
        let mut touched = self.touched;
        let holder = use_state(ParagraphHolder::default);
        let line = self.text.clone();
        let highlights = self.highlights.clone();
        let search = self.search;

        let revealed = self.revealed;
        let kind = self.kind;
        let cut = prefix_len(kind, &line).min(line.len());
        let (head, body) = line.split_at(cut);

        let (size, weight, color, line_height) = match kind {
            Kind::Heading(l) => {
                let (size, color, _) = heading_style(l);
                (size, FontWeight::BOLD, color, 1.25)
            }
            Kind::Quote if quote_depth(&line) > 1 => {
                (TEXT_BASE, FontWeight::NORMAL, c().text_faint, 1.55)
            }
            Kind::Task(true) | Kind::Quote => (TEXT_BASE, FontWeight::NORMAL, c().text_muted, 1.55),
            Kind::Footnote => (TEXT_SM, FontWeight::NORMAL, c().text_muted, 1.55),
            Kind::Code | Kind::FenceOpen | Kind::FenceClose | Kind::Table | Kind::TableRule => {
                (12., FontWeight::NORMAL, c().text_primary, 1.55)
            }
            Kind::Image => (TEXT_XS, FontWeight::NORMAL, c().text_muted, 1.55),
            // one line of it is the whole row, so the caret fits inside it
            Kind::Blank => (9., FontWeight::NORMAL, c().text_primary, BLANK_ROW / 9.),
            _ => (TEXT_BASE, FontWeight::NORMAL, c().text_primary, 1.55),
        };

        let mut spans = Vec::new();
        let mut decos = Vec::new();
        match kind {
            Kind::Code => code_spans(&line, &mut spans),
            Kind::FenceOpen | Kind::FenceClose => spans.push(marker(&line, revealed)),
            Kind::Rule => spans.push(marker(&line, revealed)),
            // a folded table row is drawn as cells over its (invisible) text
            Kind::Table | Kind::TableRule if !revealed => {
                spans.push(Span::new(line.clone()).color(Color::TRANSPARENT))
            }
            Kind::Table | Kind::TableRule => {
                spans.push(Span::new(line.clone()).color(c().text_primary))
            }
            Kind::Image if !revealed => {
                // the caption is the alt; the syntax around it folds away
                let (alt, _) = synapso_core::doc::image_line(line.trim()).unwrap_or_default();
                match line.find(&alt) {
                    Some(at) if !alt.is_empty() => {
                        spans.push(marker(&line[..at], false));
                        spans.push(Span::new(alt.clone()).color(c().text_muted));
                        spans.push(marker(&line[at + alt.len()..], false));
                    }
                    _ => spans.push(marker(&line, false)),
                }
            }
            Kind::Image => spans.push(Span::new(line.clone()).color(c().text_faint)),
            Kind::Blank => spans.push(Span::new(line.clone())),
            _ => {
                if !head.is_empty() {
                    spans.push(marker(head, revealed));
                }
                let struck = kind == Kind::Task(true);
                // h6 reads in capitals (same bytes, so every offset still holds)
                let upper = body.to_uppercase();
                let body = if kind == Kind::Heading(6) && !revealed && upper.len() == body.len() {
                    upper.as_str()
                } else {
                    body
                };
                inline_spans(
                    body,
                    revealed,
                    color,
                    struck,
                    &mut spans,
                    &mut decos,
                    head.encode_utf16().count(),
                );
            }
        }
        if spans.is_empty() {
            spans.push(Span::new(""));
        }

        let app = use_consume::<App>();
        let follow_text = line.clone();
        let floor = click_floor(kind, &line, revealed, size * line_height);
        let para = paragraph()
            .holder(holder.read().clone())
            .on_mouse_down(move |e: Event<MouseEventData>| {
                if e.button == Some(MouseButton::Right) {
                    let ed = editable.editor().read();
                    let text = ed.text().to_string();
                    let quote = match ed.selection() {
                        TextSelection::Range { from, to } => {
                            let (a, b) = ((*from).min(*to), (*from).max(*to));
                            text[byte_at(&text, a)..byte_at(&text, b)].to_string()
                        }
                        TextSelection::Cursor(_) => String::new(),
                    };
                    drop(ed);
                    let rel = app.note.read().clone();
                    let mut m = app.note_menu;
                    m.set(Some((
                        e.global_location.x as f32,
                        e.global_location.y as f32,
                        rel,
                        quote,
                    )));
                    return;
                }
                editable.process_event(EditableEvent::Down {
                    location: e.element_location,
                    editor_line: EditorLine::Paragraph(index),
                    holder: &holder.read(),
                });
                // the press maps on the folded layout — the one the pointer is
                // over — so the caret is the clicked character. What moves
                // next is the text, not the pointer: hold the drag back.
                UNFOLDED_AT.set((!revealed).then_some((e.global_location.x, e.global_location.y)));
                if !revealed {
                    let col = editable.editor().read().cursor_col();
                    if let Some(target) = synapso_core::inline::mention_at(&follow_text, col) {
                        follow(app, &target);
                        return;
                    }
                    // typing inside a marker nobody saw would break the line's kind
                    let fixed = click_col(kind, &follow_text, col);
                    if fixed != col && editable.editor().read().get_selection().is_none() {
                        let mut ed = editable.editor_mut().write();
                        let at = ed.cursor_pos();
                        ed.move_cursor_to(at - col + fixed);
                    }
                }
                let ed = editable.editor().read();
                let (row, col) = (ed.cursor_row(), ed.cursor_col());
                drop(ed);
                let mut c = caret.write();
                c.row = row;
                c.col = col;
            })
            .on_mouse_move(move |e: Event<MouseEventData>| {
                if !drags(
                    UNFOLDED_AT.get(),
                    (e.global_location.x, e.global_location.y),
                ) {
                    return;
                }
                UNFOLDED_AT.set(None);
                editable.process_event(EditableEvent::Move {
                    location: e.element_location,
                    editor_line: EditorLine::Paragraph(index),
                    holder: &holder.read(),
                });
            })
            .maybe(floor.is_some(), |p| {
                p.min_height(Size::px(floor.unwrap_or_default()))
            })
            .cursor_index(self.cursor)
            .cursor_color(c().accent)
            .cursor_mode(CursorMode::Expanded)
            .highlights((!highlights.is_empty()).then_some(highlights))
            .highlight_color(if search { c().search_hl } else { c().selection })
            .width(Size::fill())
            .font_size(size)
            .font_weight(weight)
            .line_height(line_height)
            .letter_spacing(if let Kind::Heading(l) = kind {
                heading_style(l).2
            } else {
                0.
            })
            .max_lines(
                if matches!(
                    kind,
                    Kind::Code
                        | Kind::FenceOpen
                        | Kind::FenceClose
                        | Kind::Table
                        | Kind::TableRule
                        | Kind::Image
                ) {
                    Some(1)
                } else {
                    None
                },
            )
            .spans_iter(spans.into_iter());
        // chips, washes and dashed underlines go behind the text
        let para = rect()
            .width(Size::fill())
            .background(rich::painter(holder.read().clone(), decos))
            .child(para);

        // the rows around the line: what makes a line look like its block
        let row = rect().width(Size::fill());
        match kind {
            Kind::Heading(l) => row
                .padding((
                    if self.prev == Kind::Blank && index > 0 {
                        4.
                    } else {
                        0.
                    },
                    0.,
                    if l == 1 { 6. } else { 2. },
                    0.,
                ))
                .child(para),
            Kind::Blank => row.height(Size::px(BLANK_ROW)).child(para),
            Kind::Task(done) => {
                let indent = line.len() - line.trim_start().len();
                let toggle = move |_| {
                    let mut ed = editable.editor_mut().write();
                    let start = ed.line_to_char(index);
                    let at = ed.char_to_utf16_cu(start) + indent + 3;
                    ed.remove(at..at + 1);
                    ed.insert(if done { " " } else { "x" }, at);
                    drop(ed);
                    *touched.write() += 1;
                };
                row.horizontal()
                    .cross_align(Alignment::Center)
                    .padding((3., 0., 3., nest(&line)))
                    .spacing(8.)
                    .maybe_child(guide(&line))
                    .child(checkbox(done, toggle))
                    .child(para)
            }
            Kind::Bullet => row
                .horizontal()
                .padding((2., 0., 2., nest(&line)))
                .spacing(8.)
                .maybe_child(guide(&line))
                .child(
                    label()
                        .text(if depth(&line) % 2 == 1 { "◦" } else { "•" })
                        .font_size(TEXT_BASE)
                        .line_height(1.55)
                        .color(c().text_faint),
                )
                .child(para),
            Kind::Quote => {
                // each `>` past the first steps in again, on a quieter rule
                let d = quote_depth(&line);
                let edge = |strong: bool| {
                    Border::new()
                        .fill(if strong {
                            c().border_strong
                        } else {
                            c().border_subtle
                        })
                        .width(BorderWidth {
                            left: 2.,
                            ..Default::default()
                        })
                };
                let mut el = rect()
                    .width(Size::fill())
                    .padding((0., 0., 0., 12.))
                    .border(edge(d == 1))
                    .child(para)
                    .into_element();
                for k in (1..d).rev() {
                    el = rect()
                        .width(Size::fill())
                        .padding((0., 0., 0., 12.))
                        .border(edge(k == 1))
                        .child(el)
                        .into_element();
                }
                row.padding((2., 0., 2., 0.)).child(el)
            }
            Kind::Footnote => {
                let n = line
                    .trim_start()
                    .trim_start_matches("[^")
                    .split(']')
                    .next()
                    .unwrap_or("")
                    .to_string();
                row.horizontal()
                    .padding((2., 0., 2., 0.))
                    .spacing(8.)
                    .child(
                        label()
                            .width(Size::px(18.))
                            .text(format!("{n}."))
                            .font_size(TEXT_SM)
                            .color(c().text_faint)
                            .text_align(TextAlign::End),
                    )
                    .child(rect().width(Size::flex(1.)).child(para))
                    .child(label().text("↩").font_size(TEXT_SM).color(c().text_link))
                    .content(Content::Flex)
            }
            Kind::Code | Kind::FenceOpen | Kind::FenceClose => {
                let first = kind == Kind::FenceOpen;
                let last = kind == Kind::FenceClose;
                let sides = BorderWidth {
                    left: 1.,
                    right: 1.,
                    top: if first { 1. } else { 0. },
                    bottom: if last { 1. } else { 0. },
                };
                let r = 4.;
                let lang = if first && !revealed {
                    line.trim()
                        .trim_start_matches(['`', '~'])
                        .trim()
                        .to_string()
                } else {
                    String::new()
                };
                row.background(c().surface_inset)
                    .border(Border::new().fill(c().border_subtle).width(sides))
                    .corner_radius({
                        let (t, b) = (if first { r } else { 0. }, if last { r } else { 0. });
                        (t, t, b, b)
                    })
                    .padding((0., 16.))
                    .height(Size::px(snap(if (first || last) && !revealed {
                        12.
                    } else {
                        CODE_ROW
                    })))
                    .overflow(Overflow::Clip)
                    .child(para)
                    .maybe_child((!lang.is_empty()).then(|| {
                        label()
                            .position(Position::new_absolute().right(10.).top(2.))
                            .text(lang)
                            .font_size(TEXT_XS)
                            .color(c().text_faint)
                    }))
            }
            Kind::Para => row.child(para),
            Kind::Ordered => {
                let n = line
                    .trim_start()
                    .split('.')
                    .next()
                    .unwrap_or("1")
                    .to_string();
                let n = match n.parse::<u32>() {
                    Ok(k) if depth(&line) % 2 == 1 && (1..=26).contains(&k) => {
                        char::from(b'a' + (k - 1) as u8).to_string()
                    }
                    _ => n,
                };
                row.horizontal()
                    .padding((2., 0., 2., nest(&line)))
                    .spacing(6.)
                    .maybe_child(guide(&line))
                    .child(
                        label()
                            .width(Size::px(18.))
                            .text(format!("{n}."))
                            .font_size(TEXT_BASE)
                            .line_height(1.55)
                            .color(c().text_faint)
                            .text_align(TextAlign::End),
                    )
                    .child(para)
            }
            Kind::Rule => row
                .height(Size::px(22.))
                .main_align(Alignment::Center)
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(1.))
                        .background(c().border_strong),
                )
                .child(
                    rect()
                        .position(Position::new_absolute().left(0.).top(0.))
                        .width(Size::fill())
                        .child(para),
                ),
            Kind::Table => {
                let t = self
                    .table
                    .clone()
                    .unwrap_or_else(|| rich::table_row(std::slice::from_ref(&line), |_| true, 0));
                let r = 4.;
                // a rule between body rows; none under the header (its rule row is that line)
                let top = if t.header {
                    1.
                } else if self.prev == Kind::TableRule {
                    0.
                } else {
                    1.
                };
                let cells = (!revealed).then(|| {
                    // ⚠️ an absolute child still gives its height to the row's
                    // flow content ("fill" came out 3px), and it starts inside
                    // the 4px top padding: size it to the row and lift it by the
                    // padding so the cells centre in the whole row
                    rect()
                        .position(Position::new_absolute().left(0.).top(-4.))
                        .width(Size::fill())
                        .height(Size::px(TABLE_ROW))
                        .horizontal()
                        .content(Content::Flex)
                        .children(t.widths.iter().enumerate().map(|(k, w)| {
                            let text = t.cells.get(k).cloned().unwrap_or_default();
                            rect()
                                .width(Size::flex((*w + 2) as f32))
                                .height(Size::fill())
                                .main_align(Alignment::Center)
                                .padding((0., 10.))
                                .child(
                                    label()
                                        .width(Size::fill())
                                        .text(text)
                                        .font_size(12.)
                                        .max_lines(1)
                                        .text_align(t.aligns[k])
                                        .font_weight(if t.header {
                                            FontWeight::MEDIUM
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .color(if t.header {
                                            c().text_muted
                                        } else {
                                            c().text_primary
                                        }),
                                )
                                .into_element()
                        }))
                });
                row.height(Size::px(TABLE_ROW))
                    .background(if t.header {
                        c().surface_inset
                    } else {
                        Color::TRANSPARENT
                    })
                    .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                        left: 1.,
                        right: 1.,
                        top,
                        bottom: if t.last { 1. } else { 0. },
                    }))
                    .corner_radius(match (t.header, t.last) {
                        (true, true) => (r, r, r, r),
                        (true, false) => (r, r, 0., 0.),
                        (false, true) => (0., 0., r, r),
                        _ => (0., 0., 0., 0.),
                    })
                    .padding((4., 10., 0., 10.))
                    .overflow(Overflow::Clip)
                    .child(para)
                    .maybe_child(cells)
            }
            Kind::TableRule if !revealed => row
                .height(Size::px(1.))
                .background(c().border_strong)
                .overflow(Overflow::Clip)
                .child(para),
            Kind::TableRule => row
                .height(Size::px(TABLE_ROW))
                .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                    left: 1.,
                    right: 1.,
                    ..Default::default()
                }))
                .padding((4., 10., 0., 10.))
                .child(para),
            Kind::Image => {
                let (_, src) = synapso_core::doc::image_line(line.trim()).unwrap_or_default();
                let path = app.root().join(&src);
                let picture = if path.exists() {
                    ImageViewer::new(path)
                        .width(Size::fill())
                        .height(Size::px(IMAGE_H))
                        .aspect_ratio(AspectRatio::Min)
                        .into_element()
                } else {
                    // the design's hatched placeholder, with the file it wanted
                    let hatch = RenderCallback::new(|ctx| {
                        let (w, h) = (ctx.size.width, ctx.size.height);
                        let mut paint = freya_engine::prelude::Paint::default();
                        paint.set_anti_alias(true);
                        paint.set_color(freya_engine::prelude::SkColor::from(c().surface_panel));
                        ctx.canvas
                            .draw_rect(freya_engine::prelude::SkRect::from_wh(w, h), &paint);
                        paint.set_color(freya_engine::prelude::SkColor::from(c().surface_raised));
                        paint.set_style(freya_engine::prelude::PaintStyle::Stroke);
                        paint.set_stroke_width(8. * std::f32::consts::SQRT_2 / 2. * 1.414);
                        let mut k = -h;
                        while k < w {
                            ctx.canvas.draw_line((k, h), (k + h, 0.), &paint);
                            k += 16. * std::f32::consts::SQRT_2;
                        }
                    });
                    rect()
                        .width(Size::fill())
                        .height(Size::px(IMAGE_H))
                        .center()
                        .background(hatch)
                        .border(Border::new().fill(c().border_subtle).width(1.))
                        .corner_radius(6.)
                        .overflow(Overflow::Clip)
                        .child(
                            rect()
                                .padding((2., 6.))
                                .background(c().surface_overlay)
                                .border(Border::new().fill(c().border_subtle).width(1.))
                                .corner_radius(3.)
                                .child(
                                    label()
                                        .text(src.clone())
                                        .font_size(TEXT_XS)
                                        .color(c().text_muted),
                                ),
                        )
                        .into_element()
                };
                row.spacing(4.).child(picture).child(para)
            }
        }
    }
}

fn checkbox(done: bool, on_press: impl FnMut(Event<PressEventData>) + 'static) -> impl IntoElement {
    rect()
        .width(Size::px(13.))
        .height(Size::px(13.))
        .corner_radius(3.)
        .center()
        .background(if done {
            c().accent_dim
        } else {
            c().surface_inset
        })
        .border(
            Border::new()
                .fill(if done { c().accent } else { c().border_strong })
                .width(1.),
        )
        .on_press(on_press)
        .maybe_child(done.then(|| label().text("✓").font_size(9.).color(c().accent)))
}

/// UTF-16 code units → byte offset in `text`.
fn byte_at(text: &str, u16: usize) -> usize {
    let mut units = 0;
    for (i, ch) in text.char_indices() {
        if units >= u16 {
            return i;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// Byte offset → UTF-16 code units.
fn u16_at(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

/// The keys a note answers itself (the rules live in `behave`/`format`,
/// copied from the app): ⏎ carries a list on, ⇧⏎ leaves a block, ⇥ nests or
/// moves through a table, ⌘⏎ flips a task, ⌘B/I/E/⇧X/⇧K format, ⌘⌥1–3
/// headings, ⌘⇧7/8/9 lists, ⌘⇧. quote.
fn sheet_key(e: &KeyboardEventData, editable: &UseEditable) -> Option<crate::format::Edit> {
    use crate::{behave, format};
    use synapso_core::doc::Kind as DKind;
    let ctrl = crate::ctrl(e);
    let shift = e.modifiers.contains(Modifiers::SHIFT);
    let alt = e.modifiers.contains(Modifiers::ALT);
    let ed = editable.editor().read();
    let text = ed.text().to_string();
    let (from, to) = match ed.selection() {
        TextSelection::Cursor(p) => (*p, *p),
        TextSelection::Range { from, to } => ((*from).min(*to), (*from).max(*to)),
    };
    drop(ed);
    let caret = byte_at(&text, to);
    let sel = byte_at(&text, from)..caret;
    match (&e.key, ctrl, shift, alt) {
        (Key::Named(NamedKey::Enter), false, false, false) => behave::enter(&text, caret),
        (Key::Named(NamedKey::Enter), false, true, false) => behave::exit(&text, caret),
        (Key::Named(NamedKey::Enter), true, false, false) => behave::toggle_task(&text, caret),
        (Key::Named(NamedKey::Tab), false, _, false) => {
            behave::tab(&text, caret, !shift).or_else(|| {
                // nothing structural: two spaces, never a tab character
                (!shift).then(|| format::Edit {
                    range: caret..caret,
                    text: "  ".into(),
                    caret: caret + 2,
                })
            })
        }
        (_, true, false, false) if e.code == Code::KeyB => {
            Some(format::wrap(&text, sel, format::Format::Bold))
        }
        (_, true, false, false) if e.code == Code::KeyI => {
            Some(format::wrap(&text, sel, format::Format::Italic))
        }
        (_, true, false, false) if e.code == Code::KeyE => {
            Some(format::wrap(&text, sel, format::Format::Code))
        }
        (_, true, true, false) if e.code == Code::KeyX => {
            Some(format::wrap(&text, sel, format::Format::Strike))
        }
        (_, true, true, false) if e.code == Code::KeyK => Some(format::link(&text, sel)),
        (_, true, false, true) if e.code == Code::Digit1 => {
            format::set_kind(&text, caret, &DKind::Heading(1))
        }
        (_, true, false, true) if e.code == Code::Digit2 => {
            format::set_kind(&text, caret, &DKind::Heading(2))
        }
        (_, true, false, true) if e.code == Code::Digit3 => {
            format::set_kind(&text, caret, &DKind::Heading(3))
        }
        (_, true, true, false) if e.code == Code::Digit7 => {
            format::set_kind(&text, caret, &DKind::Ordered)
        }
        (_, true, true, false) if e.code == Code::Digit8 => {
            format::set_kind(&text, caret, &DKind::Bullet)
        }
        (_, true, true, false) if e.code == Code::Digit9 => {
            format::set_kind(&text, caret, &DKind::Task(false))
        }
        (_, true, true, false) if e.code == Code::Period => {
            format::set_kind(&text, caret, &DKind::Quote)
        }
        _ => None,
    }
}

/// One replacement into the editor's rope, the caret where the rule put it.
fn apply(editable: &mut UseEditable, edit: &crate::format::Edit) {
    let text = editable.editor().read().text().to_string();
    let (start, end) = (
        edit.range.start.min(text.len()),
        edit.range.end.min(text.len()),
    );
    let (u_start, u_end) = (u16_at(&text, start), u16_at(&text, end));
    let after = format!("{}{}{}", &text[..start], edit.text, &text[end..]);
    let caret = u16_at(&after, edit.caret.min(after.len()));
    let mut ed = editable.editor_mut().write();
    if u_end > u_start {
        ed.remove(u_start..u_end);
    }
    if !edit.text.is_empty() {
        ed.insert(&edit.text, u_start);
    }
    ed.clear_selection();
    ed.move_cursor_to(caret);
}

/// How far a nested list item steps in: 18px per two spaces of indent.
fn nest(line: &str) -> f32 {
    depth(line) as f32 * 18.
}

/// A list item's nesting: one per two spaces of indent.
fn depth(line: &str) -> usize {
    (line.len() - line.trim_start().len()) / 2
}

/// The thin rule a nested item hangs from, under its parent's marker
/// (absolute children sit in the row's padded area, so it reaches back).
fn guide(line: &str) -> Option<Element> {
    (depth(line) > 0).then(|| {
        rect()
            .position(Position::new_absolute().left(-13.).top(0.))
            .width(Size::px(1.))
            .height(Size::fill())
            .background(c().border_subtle)
            .into_element()
    })
}

/// Case-insensitive hits of `query` in `line`, as UTF-16 ranges.
fn find_in(line: &str, query: &str) -> Vec<(usize, usize)> {
    if query.trim().is_empty() {
        return vec![];
    }
    let hay = line.to_lowercase();
    let q = query.to_lowercase();
    // lowercasing can change byte lengths; only trust same-length lines
    if hay.len() != line.len() {
        return vec![];
    }
    hay.match_indices(&q)
        .map(|(at, m)| (u16_at(line, at), u16_at(line, at + m.len())))
        .collect()
}

/// ⌘F: floats at the top right of the note. ⏎/↓ next, ↑ previous, Esc closes.
#[derive(PartialEq)]
struct FindBar {
    total: usize,
}

impl Component for FindBar {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut query = use_state(|| app.find.read().clone().unwrap_or_default());
        // mirror the field into the shared query
        let q = query.read().clone();
        if app.find.read().as_deref() != Some(q.as_str()) {
            let mut f = app.find;
            f.set(Some(q.clone()));
            let mut at = app.find_at;
            at.set(0);
        }
        let total = self.total;
        let at = *app.find_at.read();
        let step = move |d: i32| {
            let mut a = app.find_at;
            let n = total.max(1) as i32;
            let cur = *a.read() as i32;
            a.set(((cur + d).rem_euclid(n)) as usize);
        };
        let summary = if q.trim().is_empty() {
            "type to find".to_string()
        } else if total == 0 {
            "no matches".to_string()
        } else {
            format!("{}/{}", at % total + 1, total)
        };
        let _ = &mut query;
        rect()
            .position(Position::new_absolute().right(16.).top(74.))
            .layer(Layer::Relative(4))
            .width(Size::px(320.))
            .horizontal()
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            .spacing(8.)
            .padding((6., 12.))
            .background(c().surface_overlay)
            .border(Border::new().fill(c().border_strong).width(1.))
            .corner_radius(6.)
            .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
            .font_size(TEXT_SM)
            .child(label().text("⌕").color(c().text_faint))
            .child(
                rect().width(Size::flex(1.)).child(
                    Field::new(
                        query,
                        move |_| step(1),
                        move |_| {
                            let mut f = app.find;
                            f.set(None);
                        },
                    )
                    .placeholder("find in note")
                    .on_arrow(step),
                ),
            )
            .child(
                label()
                    .text(summary)
                    .font_size(TEXT_XS)
                    .color(c().text_muted),
            )
            .child(
                label()
                    .text("‹")
                    .color(c().text_muted)
                    .on_press(move |_| step(-1)),
            )
            .child(
                label()
                    .text("›")
                    .color(c().text_muted)
                    .on_press(move |_| step(1)),
            )
    }
}

/// What a `[[canvas: …]]` mention names.
#[derive(Debug, PartialEq)]
enum CanvasLink<'a> {
    /// The board's rel.
    Board(String),
    /// A card on a board: `id:3`, or a caption (`board::card_id` reads it).
    Card { board: String, card: &'a str },
}

/// The inside of a `[[canvas: …]]` mention, the scheme in any case and with
/// space around its colon — the spelling contract notes were written under.
fn canvas_mention(target: &str) -> Option<&str> {
    let (scheme, rest) = target.split_once(':')?;
    scheme
        .trim()
        .eq_ignore_ascii_case("canvas")
        .then(|| rest.trim())
}

/// Read a canvas mention (alias already off) the way the Floem shell did.
/// `board` finds a board's rel by its name; `open` is the board on screen.
fn canvas_link<'a>(
    target: &'a str,
    open: &str,
    board: impl Fn(&str) -> Option<String>,
) -> Option<CanvasLink<'a>> {
    // the chrome prints `roadmap.canvas`, copied mentions say `roadmap`
    let named = |name: &str| {
        let name = name.trim();
        // ASCII lowering keeps every byte where it was, so the cut is too
        let lower = name.to_ascii_lowercase();
        board(
            lower
                .strip_suffix(".canvas")
                .map_or(name, |s| &name[..s.len()]),
        )
    };
    let target = target.trim();
    // ⚠️ a whole board name wins before the `#` is read: card captions may
    // hold a hash (`sprint #47`), and the older caption-only form names them
    if let Some(rel) = named(target) {
        return Some(CanvasLink::Board(rel));
    }
    let qualified = target
        .split_once('#')
        .and_then(|(name, card)| Some((named(name)?, card.trim())));
    let (board, card) = match qualified {
        // `board#` names no card: an empty caption would pick any stroke
        Some((board, "")) => return Some(CanvasLink::Board(board)),
        Some(found) => found,
        None if !open.is_empty() => (open.to_string(), target),
        None => return None,
    };
    Some(CanvasLink::Card { board, card })
}

/// A note mention split into the file it names and the section inside it:
/// `note`, `note#Heading`, `note|label`, `note#Heading|label`.
///
/// ⚠️ the alias comes off FIRST. `[[note#Heading|label]]` puts the `|` AFTER
/// the `#`, so splitting on whichever comes first would hand `Heading|label`
/// to the section lookup.
fn note_target(target: &str) -> (&str, Option<&str>) {
    let named = target.split_once('|').map_or(target, |(head, _)| head);
    match named.split_once('#') {
        Some((base, section)) => (base.trim(), Some(section.trim())),
        None => (named.trim(), None),
    }
}

/// Where the caret lands for `[[note#Heading]]` or `[[note#^id]]` in this
/// buffer, in the editor's UTF-16 units: `sheet::section_offset` answers in
/// bytes, a heading's words or the line wearing the anchor.
///
/// ⚠️ the raw buffer's offset, and not `Lines::heading_block`: the sheet is
/// one row per line of the file, and a block index counts paragraphs with
/// the frontmatter gone.
fn landing(text: &str, section: &str) -> Option<usize> {
    synapso_core::sheet::section_offset(text, section).map(|at| u16_at(text, at))
}

/// How long a landing holds its line in place after the pane's width last
/// moved: past the panes' 320 ms glide (`main.rs`).
const SETTLE_MS: u64 = 500;

/// The row a landing scrolls to the top on this render, if any: a fresh
/// landing, or the one `pin` holds when the pane's width has moved since it
/// was scrolled for. ⚠️ only then: a render for any other reason (a key, the
/// caret, a toast) must leave a scroll the user made alone.
fn held_row(landed: Option<usize>, pin: Option<(usize, f32)>, width: f32) -> Option<usize> {
    landed.or_else(|| {
        pin.filter(|(_, at)| (at - width).abs() > 0.5)
            .map(|(row, _)| row)
    })
}

/// What a link to a section the note doesn't have says, in the words of
/// "nothing named X": the note still opens, and this is why it opened at
/// the top.
fn no_section(note: &str, section: &str) -> String {
    let what = if section.starts_with('^') {
        "block"
    } else {
        "section"
    };
    format!("{note} has no {what} {section}")
}

/// Open what a `[[mention]]` names: `note`, `note#Heading`, `note#^id`,
/// `canvas: board`, and `canvas: board#id:3` — the board, with that card
/// brought into view.
fn follow(app: App, target: &str) {
    let target = target.split('|').next().unwrap_or(target);
    if let Some(inside) = canvas_mention(target) {
        let open = app.canvas.read().clone();
        let link = canvas_link(inside, &open, |name| {
            app.index
                .read()
                .docs
                .iter()
                .find(|d| {
                    d.kind == crate::index::DocKind::Canvas && d.stem.eq_ignore_ascii_case(name)
                })
                .map(|d| d.rel.clone())
        });
        let mut reveal = app.reveal;
        match link {
            Some(CanvasLink::Board(rel)) => {
                reveal.set(None);
                app.open(&rel);
            }
            Some(CanvasLink::Card { board, card }) => {
                // the board takes it once it is on screen with a size
                reveal.set(Some((board.clone(), card.to_string())));
                app.open(&board);
            }
            None => app.toast(format!("nothing named {inside}"), "", None),
        }
        return;
    }
    let (name, section) = note_target(target);
    let found = {
        let index = app.index.read();
        index
            .docs
            .iter()
            .find(|d| d.stem.eq_ignore_ascii_case(name) && d.kind != crate::index::DocKind::Canvas)
            .or_else(|| index.by_stem(name))
            .map(|d| d.rel.clone())
    };
    match found {
        Some(rel) => app.open_at(&rel, section),
        None => app.toast(format!("nothing named {name}"), "", None),
    }
}

/// A picture line's height on the sheet.
const IMAGE_H: f32 = 180.;

/// `12. ` → the bytes of its marker, number and dot and space.
fn ordered_marker(t: &str) -> Option<usize> {
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    (digits > 0 && digits < 10 && t[digits..].starts_with(". ")).then_some(digits + 2)
}

/// The note (or the selected words, as a quote card) onto the open board.
fn send_to_canvas(app: App, rel: &str, quote: &str) {
    let board = app.canvas.read().clone();
    if board.is_empty() {
        app.toast("no canvas open", "open one from the tree first", None);
        return;
    }
    crate::board::flush_board();
    let mut scene = synapso_core::board::Scene::load(app.root().join(&board));
    if scene.broken.is_some() {
        return;
    }
    let at = synapso_core::board::corner(&scene.canvas.nodes).unwrap_or_default();
    let at = kurbo::Point::new(at.x, at.y + 40.);
    if quote.trim().is_empty() {
        scene.attach_ref(None, at, rel, None);
    } else {
        scene.add_excerpt(at, rel, quote.trim());
    }
    if scene.save().is_ok() {
        let mut rev = app.board_rev;
        *rev.write() += 1;
        let mut v = app.view;
        if *v.read() == crate::app::View::Notes {
            v.set(crate::app::View::Split);
        }
        app.toast(format!("sent to {}", crate::base_name(&board)), "", None);
    }
}

/// The note's right-click menu, over everything.
#[derive(PartialEq)]
pub struct NoteMenu {}

impl Component for NoteMenu {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut size = use_state(|| None::<(f32, f32)>);
        let Some((x, y, rel, quote)) = app.note_menu.read().clone() else {
            return rect().into_element();
        };
        let mut menu = app.note_menu;
        let label_text = if quote.trim().is_empty() {
            "send note to canvas"
        } else {
            "send selection to canvas"
        };
        let item = |text: &'static str, f: Box<dyn Fn()>| {
            rect()
                .width(Size::fill())
                .padding((5., 10.))
                .corner_radius(3.)
                .on_press(move |_| {
                    let mut menu = app.note_menu;
                    menu.set(None);
                    f();
                })
                .child(label().text(text).font_size(TEXT_SM))
                .into_element()
        };
        let (r1, q1) = (rel.clone(), quote.clone());
        rect()
            .position(Position::new_absolute().left(0.).top(0.))
            .expanded()
            .layer(Layer::Overlay)
            .on_press(move |_| menu.set(None))
            .child(
                rect()
                    .position({
                        let (x, y) = crate::overlays::menu_at(
                            (x, y),
                            size.read().unwrap_or_default(),
                            crate::overlays::window_area(),
                        );
                        Position::new_absolute().left(x).top(y)
                    })
                    // hidden until measured: it can only flip knowing its size
                    .opacity(if size.read().is_some() { 1. } else { 0. })
                    .on_sized(move |e: Event<SizedEventData>| {
                        size.set_if_modified(Some((e.area.width(), e.area.height())))
                    })
                    .padding(4.)
                    .width(Size::px(220.))
                    .background(c().surface_overlay)
                    .border(Border::new().fill(c().border_strong).width(1.))
                    .corner_radius(4.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .on_press(|e: Event<PressEventData>| e.stop_propagation())
                    .child(item(
                        label_text,
                        Box::new(move || send_to_canvas(app, &r1, &q1)),
                    ))
                    .child(item(
                        "find in note",
                        Box::new(move || {
                            let mut f = app.find;
                            f.set(Some(String::new()));
                        }),
                    )),
            )
            .into_element()
    }
}
#[cfg(test)]
mod fmt_menu_tests {
    use super::*;

    #[test]
    fn scaled_multiplies_alpha_once() {
        let half = scaled(Color::from_argb(100, 10, 20, 30), 0.5);
        assert_eq!(half.a(), 49);
        assert_eq!(scaled(Color::from_argb(255, 1, 2, 3), 0.).a(), 0);
    }

    #[test]
    fn menus_are_at_least_170_wide_and_fit_their_widest_row() {
        for (i, (name, _)) in MENUS.iter().enumerate() {
            assert!(menu_width(i) >= 170., "{name}");
        }
        // "Strikethrough" + 16 gap + ⌘⇧X chip + row padding still fits
        assert!(menu_width(1) >= 13. * 6.6 + 16. + 10. + 12. + 24.);
    }

    fn editor(text: &str, selection: TextSelection) -> RopeEditor {
        RopeEditor::new(
            text.into(),
            selection,
            2,
            EditorHistory::new(std::time::Duration::ZERO),
        )
    }

    #[test]
    fn a_click_under_the_text_takes_the_caret_to_the_end() {
        let text = "one\ntwo words\nthree";
        // from the middle of the text, with words selected
        let mut ed = editor(text, TextSelection::new_range((2, 8)));
        to_end(&mut ed, false);
        assert_eq!(*ed.selection(), TextSelection::Cursor(text.len()));
        // ⇧: from the anchor, through to the end
        let mut ed = editor(text, TextSelection::new_range((2, 8)));
        to_end(&mut ed, true);
        assert_eq!(*ed.selection(), TextSelection::new_range((2, text.len())));
        let mut ed = editor(text, TextSelection::new_cursor(5));
        to_end(&mut ed, true);
        assert_eq!(*ed.selection(), TextSelection::new_range((5, text.len())));
    }

    #[test]
    fn the_room_under_the_last_line_reaches_the_bottom_of_the_view() {
        assert_eq!(tail(600., 200.), 400.);
        // a long note: just the pad, past the end of its text
        assert_eq!(tail(600., 590.), PAD_BOTTOM);
        assert_eq!(tail(600., 2000.), PAD_BOTTOM);
        // whole pixels, so a note that fits never scrolls by a rounding error
        assert_eq!(tail(600.5, 200.25), 400.);
    }

    #[test]
    fn a_click_on_a_folded_line_lands_after_its_hidden_syntax() {
        let at = |line: &str, col| click_col(classify(line)[0], line, col);
        // an empty task is all syntax: wherever the click resolves in it,
        // typing continues the task
        assert_eq!(at("- [ ] ", 3), 6);
        assert_eq!(at("- [ ] ", 6), 6);
        assert_eq!(at("  - [ ] ", 0), 8);
        assert_eq!(at("- ", 1), 2);
        assert_eq!(at("## ", 0), 3);
        // words are clicked where they are
        assert_eq!(at("- [ ] abc", 7), 7);
        assert_eq!(at("plain text", 0), 0);
        assert_eq!(at("```rust", 2), 2);
    }

    #[test]
    fn an_empty_line_is_as_tall_with_the_caret_on_it_as_without() {
        for line in ["", "   "] {
            for after_blank in [false, true] {
                let editing = measure_line(Kind::Blank, line, true, after_blank, 600.);
                let reading = measure_line(Kind::Blank, line, false, after_blank, 600.);
                assert_eq!(editing, reading, "{line:?}");
            }
        }
    }

    #[test]
    fn a_press_that_unfolds_its_line_is_a_click_until_the_pointer_travels() {
        // a press on the line already being edited: every move drags
        assert!(drags(None, (10., 10.)));
        let press = Some((100., 50.));
        assert!(!drags(press, (100., 50.)));
        // trackpad jitter while the text shifts under the pointer
        assert!(!drags(press, (102., 51.)));
        assert!(!drags(press, (100., 46.)));
        // a real drag
        assert!(drags(press, (108., 50.)));
        assert!(drags(press, (100., 40.)));
    }

    #[test]
    fn a_drop_goes_in_at_the_nearer_edge_of_the_line_under_it() {
        // PAD_TOP, then lines 20, 40 and 20 tall
        let heights = [20., 40., 20.];
        assert_eq!(gap_at(&heights, 0.), 0, "above the first line");
        assert_eq!(gap_at(&heights, PAD_TOP + 9.), 0);
        assert_eq!(gap_at(&heights, PAD_TOP + 11.), 1);
        assert_eq!(gap_at(&heights, PAD_TOP + 20. + 19.), 1);
        assert_eq!(gap_at(&heights, PAD_TOP + 20. + 21.), 2);
        assert_eq!(gap_at(&heights, PAD_TOP + 75.), 3);
        assert_eq!(gap_at(&heights, 5000.), 3, "under the last line");
        assert_eq!(gap_at(&[], 50.), 0, "an empty note");
    }

    #[test]
    fn dropped_pictures_are_lines_of_their_own() {
        let files = ["assets/a.png".to_string(), "assets/b.png".to_string()];
        let one = &files[..1];
        let put = |text: &str, gap: usize, files: &[String]| {
            let e = picture_drop(text, gap, files);
            let after = format!(
                "{}{}{}",
                &text[..e.range.start],
                e.text,
                &text[e.range.end..]
            );
            (after, e.caret)
        };
        assert_eq!(put("a\nb", 0, one).0, "![](assets/a.png)\na\nb");
        assert_eq!(
            put("a\nb", 1, &files).0,
            "a\n![](assets/a.png)\n![](assets/b.png)\nb"
        );
        // under a last line with no newline: one is added before, not after
        assert_eq!(put("a\nb", 2, one).0, "a\nb\n![](assets/a.png)\n");
        assert_eq!(put("a\n", 2, one).0, "a\n![](assets/a.png)\n");
        assert_eq!(put("", 0, one).0, "![](assets/a.png)\n");
        // past the end is the end
        assert_eq!(put("a", 9, one).0, "a\n![](assets/a.png)\n");
        // every inserted line is a picture to the sheet
        for line in put("a\nb", 1, &files).0.split('\n').skip(1).take(2) {
            assert!(synapso_core::doc::image_line(line).is_some(), "{line}");
        }
        // the caret: on the line that was under them
        let (after, caret) = put("a\nb", 1, one);
        assert_eq!(&after[caret..], "b");
    }

    #[test]
    fn a_canvas_mention_names_a_board_or_a_card_on_one() {
        assert_eq!(canvas_mention("canvas: main#id:1"), Some("main#id:1"));
        assert_eq!(canvas_mention(" Canvas : main"), Some("main"));
        assert_eq!(canvas_mention("auth-flow#Token"), None);
        assert_eq!(canvas_mention("note: main"), None);

        let boards = |name: &str| {
            ["main", "Roadmap"]
                .iter()
                .find(|b| b.eq_ignore_ascii_case(name))
                .map(|b| format!("boards/{b}.canvas"))
        };
        let link = |target, open| canvas_link(target, open, boards);
        let card = |board: &str, card| {
            Some(CanvasLink::Card {
                board: board.into(),
                card,
            })
        };
        // what ⌘C on a card writes: the board, and the card by id
        assert_eq!(link("main#id:1", ""), card("boards/main.canvas", "id:1"));
        assert_eq!(
            link(" main # id:1 ", ""),
            card("boards/main.canvas", "id:1")
        );
        assert_eq!(
            link("main#token store", ""),
            card("boards/main.canvas", "token store")
        );
        assert_eq!(
            link("main#", ""),
            Some(CanvasLink::Board("boards/main.canvas".into()))
        );
        // a board by its name, its file name, any case
        let roadmap = Some(CanvasLink::Board("boards/Roadmap.canvas".into()));
        assert_eq!(link("roadmap", ""), roadmap);
        assert_eq!(link("Roadmap.canvas", "boards/main.canvas"), roadmap);
        // no board before the `#`: a caption on the board on screen, hash and all
        let open = "boards/main.canvas";
        assert_eq!(link("sprint #47", open), card(open, "sprint #47"));
        assert_eq!(link("token store", open), card(open, "token store"));
        assert_eq!(link("token store", ""), None, "and no board on screen");
    }

    /// `[[note#Heading|label]]` splits into the file and the section — and the
    /// alias has to come off BEFORE the hash, or the section carries it.
    #[test]
    fn a_note_mention_splits_into_its_file_and_its_section() {
        assert_eq!(note_target("auth-flow"), ("auth-flow", None));
        assert_eq!(note_target("auth-flow|the flow"), ("auth-flow", None));
        assert_eq!(note_target("auth-flow#Token"), ("auth-flow", Some("Token")));
        // ⚠️ the case that breaks if the split order is wrong
        assert_eq!(
            note_target("auth-flow#Token|the token bit"),
            ("auth-flow", Some("Token")),
            "the alias must not ride along into the section"
        );
        // a heading with spaces in it stays whole, trimmed at both ends
        assert_eq!(
            note_target("  auth-flow # Token store |x"),
            ("auth-flow", Some("Token store"))
        );
        // `#` with nothing after it names no section (`App::open_at` drops it)
        assert_eq!(note_target("auth-flow#"), ("auth-flow", Some("")));
    }

    /// `[[note#Heading]]` parks the caret on the heading's words, counted in
    /// the editor's UTF-16 units: the byte offset past an `é` or a `☕` is not
    /// where the caret goes.
    #[test]
    fn a_section_link_lands_on_its_heading() {
        let note = "café ☕\n\n## Token store\nwords";
        let at = landing(note, "token store").expect("the heading is there");
        assert_eq!(&note[byte_at(note, at)..], "Token store\nwords");
        assert_eq!(landing(note, "nowhere"), None);
    }

    /// `[[note#^id]]` lands on the line wearing the anchor, and never on a
    /// `^` inside a fence, which is code.
    #[test]
    fn an_anchor_link_lands_on_its_line() {
        let note = "café\n\nthe gateway holds it ^gate-1\n```\nx ^gate-2\n```\n";
        let at = landing(note, "^gate-1").expect("the anchored line");
        assert!(note[byte_at(note, at)..].starts_with("the gateway"));
        assert_eq!(landing(note, "^gate-2"), None, "not inside a fence");
    }

    /// A landing scrolls once when it lands and again each time the pane's
    /// width moves under it (the glide in from the canvas), and never for a
    /// render that changed nothing of the width.
    #[test]
    fn a_landing_follows_the_width_and_nothing_else() {
        assert_eq!(held_row(Some(7), None, 0.), Some(7), "a fresh landing");
        assert_eq!(
            held_row(Some(7), Some((3, 500.)), 500.),
            Some(7),
            "replaces"
        );
        assert_eq!(held_row(None, Some((7, 0.)), 240.), Some(7), "the glide");
        assert_eq!(
            held_row(None, Some((7, 600.)), 600.),
            None,
            "the user's scroll"
        );
        assert_eq!(held_row(None, None, 600.), None);
    }

    /// A link to a section the note doesn't have says so, naming which kind
    /// it looked for: a heading, or a line's `^anchor`.
    #[test]
    fn a_missing_section_or_block_says_which() {
        assert_eq!(
            no_section("auth-flow", "Token"),
            "auth-flow has no section Token"
        );
        assert_eq!(
            no_section("auth-flow", "^gate-1"),
            "auth-flow has no block ^gate-1"
        );
    }

    /// A pipe line on its own is drawn as the paragraph it is; the grid waits
    /// for a second row, above or below.
    #[test]
    fn a_lone_pipe_line_is_drawn_as_words() {
        assert_eq!(classify("| abc | xyz |"), vec![Kind::Para]);
        assert_eq!(classify("|test|"), vec![Kind::Para]);
        assert_eq!(classify("|hmmm"), vec![Kind::Para]);
        assert_eq!(classify("|---|"), vec![Kind::Para], "a lone divider too");
        assert_eq!(
            classify("text\n| a | b |\n\n| c |"),
            vec![Kind::Para, Kind::Para, Kind::Blank, Kind::Para]
        );
        // the second row makes both the table, divider or not
        assert_eq!(classify("| a | b |\n| c"), vec![Kind::Table, Kind::Table]);
        assert_eq!(
            classify("| a | b |\n|---|---|"),
            vec![Kind::Table, Kind::TableRule]
        );
        // inside a fence it is code, whatever its neighbours are
        assert_eq!(classify("```\n|a|\n```")[1], Kind::Code);
    }

    /// ⌘V with a picture goes in as a line of its own at the caret: on the
    /// caret's line when that is blank, under it when it holds words — a
    /// picture never splits a sentence. The caret lands on the line after it.
    #[test]
    fn a_pasted_picture_is_a_line_of_its_own_at_the_caret() {
        let pics = ["![](assets/a.png)".to_string()];
        let put = |text: &str, row: usize| {
            let e = picture_paste(text, row, &pics);
            let out = format!(
                "{}{}{}",
                &text[..e.range.start],
                e.text,
                &text[e.range.end..]
            );
            (out.clone(), out[e.caret..].to_string())
        };
        assert_eq!(
            put("one\n\ntwo", 1),
            ("one\n![](assets/a.png)\n\ntwo".into(), "\ntwo".into())
        );
        assert_eq!(
            put("one sentence\ntwo", 0),
            ("one sentence\n![](assets/a.png)\ntwo".into(), "two".into())
        );
        assert_eq!(put("one", 0).0, "one\n![](assets/a.png)\n");
        assert_eq!(put("", 0).0, "![](assets/a.png)\n");

        // text that is nothing but pictures — a picture card copied off the
        // board — is pictures; anything else is the editor's own paste
        assert_eq!(
            picture_lines("![](assets/a.png)\n ![shot](b.png) \n"),
            Some(vec!["![](assets/a.png)".into(), "![shot](b.png)".into()])
        );
        assert_eq!(picture_lines("see ![](a.png)"), None);
        assert_eq!(picture_lines("[[canvas: main#id:3|a]]"), None);
        assert_eq!(picture_lines("\n"), None);
    }

    /// ⚠️ the regression this guards: a rule, all hidden syntax, folded to a
    /// paragraph of 0.1px glyphs on a 22px row, and a click on it went nowhere.
    #[test]
    fn a_line_that_is_only_syntax_is_clickable_across_its_whole_row() {
        for line in ["---", "***", "```rust", "```"] {
            let kind = classify(line)[0];
            for revealed in [false, true] {
                assert_eq!(
                    click_floor(kind, line, revealed, 20.),
                    Some(measure_line(kind, line, revealed, false, 400.)),
                    "{line:?} revealed={revealed}"
                );
            }
        }
        assert_eq!(
            click_floor(classify("- [ ] ")[0], "- [ ] ", false, 20.),
            Some(20.)
        );
        assert_eq!(click_floor(Kind::Blank, "", false, 20.), None);
    }
}
