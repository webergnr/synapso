//! The folder tree (design 5c): ⌘⇧E slides it over the note without
//! reflowing anything, and opening a note, Esc or a click elsewhere closes
//! it. "pin" docks it beside the rail instead (5a with the rail outside).
//! Folders are groups: each row wears its resolved colour line, and a note
//! without its own colour shows its folder's.
use std::collections::HashSet;

use freya::animation::*;
use freya::prelude::*;

use crate::{
    Spine,
    app::App,
    base_name,
    field::Field,
    index::{DocKind, Index},
    theme::*,
};

/// Docked, and floating: the floating panel is a little wider.
pub const WIDTH: f32 = 220.;
pub const FLOAT_WIDTH: f32 = 260.;

#[derive(PartialEq)]
pub struct Tree {
    pub floating: bool,
}

/// The tree when it isn't pinned: over the note, beside the rail, sliding in.
#[derive(PartialEq)]
pub struct FloatingTree {}

impl Component for FloatingTree {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let on = *app.tree.read() && !*app.tree_pinned.read();
        let anim = use_animation_transition(crate::use_tracked(on), |from, to| {
            let v = |b: bool| if b { 1. } else { 0. };
            AnimNum::new(v(from), v(to))
                .time(180)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let mut area = use_state(Area::default);
        let t = anim.read().value();
        if t <= 0.001 && !on {
            return rect().into_element();
        }
        // ⚠️ a Size::func sees physical pixels; the status bar is 24 logical
        let scale = *Platform::get().current_window().scale_factor.read() as f32;
        let left = if *app.view.read() == crate::app::View::Notes {
            0.
        } else {
            34.
        };
        rect()
            .position(Position::new_absolute().left(left - 12. * (1. - t)).top(0.))
            .width(Size::px(FLOAT_WIDTH))
            .height(Size::func_data(
                move |ctx| Some(ctx.parent - 24. * scale),
                &scale.to_bits(),
            ))
            .opacity(t)
            // above the sheet's own line layers, below every popover
            .layer(Layer::Relative(50))
            .on_sized(move |e: Event<SizedEventData>| area.set(e.area))
            // a click anywhere else closes it, and still does what it was for
            .on_global_pointer_down(move |e: Event<PointerEventData>| {
                if !*app.tree.peek() || *app.tree_pinned.peek() {
                    return;
                }
                if app.color_pick.peek().is_some()
                    || app.tree_menu.peek().is_some()
                    || app.drag_doc.peek().is_some()
                {
                    return;
                }
                let p = e.global_location();
                let a = *area.peek();
                let (x, y) = (p.x as f32, p.y as f32);
                if !(x >= a.min_x() && x <= a.max_x() && y >= a.min_y() && y <= a.max_y()) {
                    let mut tree = app.tree;
                    tree.set(false);
                }
            })
            .child(Tree { floating: true })
            .into_element()
    }
}

/// A new note from the tree: it opens, and the name is typed in its row, so
/// the drawer must still be there. ⚠️ `new_note` opens the note, and opening
/// closes an unpinned drawer.
fn new_note_in(app: App, folder: String) {
    crate::new_note(app, Some(folder));
    let mut tree = app.tree;
    tree.set(true);
}

#[derive(Clone, PartialEq)]
enum Row {
    Folder {
        rel: String,
        name: String,
        depth: usize,
        open: bool,
    },
    Doc {
        rel: String,
        depth: usize,
        kind: DocKind,
    },
}

fn rows(index: &Index, collapsed: &HashSet<String>) -> Vec<Row> {
    fn walk(
        index: &Index,
        folder: &str,
        depth: usize,
        collapsed: &HashSet<String>,
        out: &mut Vec<Row>,
    ) {
        let (folders, docs) = index.children(folder);
        for f in folders {
            let open = !collapsed.contains(&f.rel);
            out.push(Row::Folder {
                rel: f.rel.clone(),
                name: f.name.clone(),
                depth,
                open,
            });
            if open {
                walk(index, &f.rel, depth + 1, collapsed, out);
            }
        }
        for d in docs {
            out.push(Row::Doc {
                rel: d.rel.clone(),
                depth,
                kind: d.kind,
            });
        }
    }
    let mut out = vec![];
    walk(index, "", 0, collapsed, &mut out);
    out
}

impl Component for Tree {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let collapsed = use_state(HashSet::<String>::new);
        let index = app.index.read();
        let list = rows(&index, &collapsed.read());
        drop(index);
        let mut tree = app.tree;
        let mut pinned = app.tree_pinned;
        let is_pinned = *pinned.read();
        let floating = self.floating;

        rect()
            .width(Size::px(if floating { FLOAT_WIDTH } else { WIDTH }))
            .height(Size::fill())
            .background(c().surface_panel)
            .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                right: 1.,
                ..Default::default()
            }))
            .maybe(floating, |r| {
                r.shadow(Shadow::new().x(8.).blur(24.).color(c().shadow(1.)))
            })
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(34.))
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .content(Content::Flex)
                    .padding((0., 8., 0., 12.))
                    .spacing(4.)
                    .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                        bottom: 1.,
                        ..Default::default()
                    }))
                    .child(
                        label()
                            .text("VAULT")
                            .font_size(TEXT_XS)
                            // --tracking-wide, .08em
                            .letter_spacing(0.8)
                            .color(c().text_faint),
                    )
                    .child(rect().width(Size::flex(1.)))
                    .child(IconBtn {
                        src: crate::icons::FILE_PLUS,
                        on: EventHandler::new(move |_| new_note_in(app, String::new())),
                    })
                    .child(IconBtn {
                        src: crate::icons::FOLDER_PLUS,
                        on: EventHandler::new(move |_| crate::new_folder(app, "")),
                    })
                    .child(
                        label()
                            .text(if is_pinned { "unpin" } else { "pin" })
                            .font_size(TEXT_XS)
                            .color(if is_pinned {
                                c().accent
                            } else {
                                c().text_muted
                            })
                            .on_press(move |e: Event<PressEventData>| {
                                e.stop_propagation();
                                pinned.set(!is_pinned);
                            }),
                    )
                    .child(hide_btn("‹", move || tree.set(false))),
            )
            .maybe_child(
                app.tree_menu
                    .read()
                    .clone()
                    .map(|(rel, x, y)| TreeMenu { rel, x, y }.into_element()),
            )
            .child(
                ScrollView::new()
                    .width(Size::fill())
                    .height(Size::fill())
                    .child(
                        rect().width(Size::fill()).padding((6., 0.)).children(
                            list.into_iter()
                                .map(|row| TreeRow { row, collapsed }.into_element()),
                        ),
                    ),
            )
    }
}

/// A 22×20 icon button in the tree's header.
#[derive(PartialEq)]
struct IconBtn {
    src: (&'static str, &'static [u8]),
    on: EventHandler<()>,
}

impl Component for IconBtn {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let on = self.on.clone();
        let hov = *hover.read();
        rect()
            .width(Size::px(22.))
            .height(Size::px(20.))
            .center()
            .corner_radius(3.)
            .background(if hov {
                c().surface_raised
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on.call(())
            })
            .child(crate::icons::icon(
                self.src,
                14.,
                if hov {
                    c().text_primary
                } else {
                    c().text_muted
                },
            ))
    }
}

fn hide_btn(text: &'static str, mut f: impl FnMut() + 'static) -> impl IntoElement {
    label()
        .text(text)
        .font_size(TEXT_XS)
        .color(c().text_faint)
        .on_press(move |e: Event<PressEventData>| {
            e.stop_propagation();
            f()
        })
}

#[derive(PartialEq)]
struct TreeRow {
    row: Row,
    collapsed: State<HashSet<String>>,
}

impl Component for TreeRow {
    fn render_key(&self) -> DiffKey {
        match &self.row {
            Row::Folder { rel, .. } | Row::Doc { rel, .. } => rel.into(),
        }
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let mut collapsed = self.collapsed;
        let index = app.index.read();
        let naming = app.naming.read().clone();
        let mut name_value = use_state(String::new);
        let last_click = use_state(|| {
            (
                std::time::Instant::now() - std::time::Duration::from_secs(5),
                String::new(),
            )
        });
        let hov = *hover.read();

        let (rel, depth) = match &self.row {
            Row::Folder { rel, depth, .. } | Row::Doc { rel, depth, .. } => (rel.clone(), *depth),
        };
        let tone = index.tone(&rel);
        let own = index.own.contains_key(&crate::index::color_key(&rel));
        // ⚠️ only while the drawer is out: a closing one still draws for its
        // slide, and a field in it would take the keyboard from the note's
        let is_naming = naming.as_deref() == Some(rel.as_str()) && *app.tree.read();
        if is_naming && name_value.read().is_empty() {
            let stem = base_name(&rel)
                .trim_end_matches(".md")
                .trim_end_matches(".canvas")
                .to_string();
            name_value.set(stem);
        }
        let active = *app.note.read() == rel || *app.canvas.read() == rel;
        // open in a tab but not the one in front: the design's dot
        let open_mark = !active && app.tabs.read().contains(&rel);
        drop(index);

        let name_el = if is_naming {
            let r1 = rel.clone();
            let r2 = rel.clone();
            let field = Field::new(
                name_value,
                move |v: String| {
                    let mut naming = app.naming;
                    match crate::rename(app, &r1, &v) {
                        Ok(_) => naming.set(None),
                        Err(msg) => app.toast(msg, "pick another name", None),
                    }
                },
                move |_| {
                    // ⚠️ only its own: the field also cancels as it goes, and
                    // by then another rename may have begun (⌘N)
                    let mut naming = app.naming;
                    if naming.peek().as_deref() == Some(r2.as_str()) {
                        naming.set(None);
                    }
                },
            )
            .select_all()
            .blur_cancels();
            // 5c's name input: an inset well with an accent border
            rect()
                .width(Size::fill())
                .padding((1., 6.))
                .corner_radius(3.)
                .background(c().surface_inset)
                .border(Border::new().fill(c().accent).width(1.))
                .child(field)
                .into_element()
        } else {
            match &self.row {
                // a long name ends in … at its cell, as the design's do
                Row::Folder { name, .. } => label()
                    .text(name.clone())
                    .width(Size::fill())
                    .font_size(TEXT_SM)
                    .color(c().text_primary)
                    .max_lines(1)
                    .text_overflow(TextOverflow::Ellipsis)
                    .into_element(),
                Row::Doc { rel, kind, .. } => {
                    let base = base_name(rel);
                    let (stem, ext) = base.rsplit_once('.').unwrap_or((base, ""));
                    let glyph = if *kind == DocKind::Canvas { "◇ " } else { "" };
                    paragraph()
                        .width(Size::fill())
                        .max_lines(1)
                        .text_overflow(TextOverflow::Ellipsis)
                        .font_size(TEXT_SM)
                        .span(Span::new(glyph).color(c().text_faint))
                        .span(Span::new(stem.to_string()).color(if active || hov {
                            c().text_primary
                        } else {
                            c().text_muted
                        }))
                        .span(Span::new(format!(".{ext}")).color(c().text_faint))
                        .into_element()
                }
            }
        };

        // one 10px column for folders and notes alike, so the lines align
        let chevron = rect()
            .width(Size::px(10.))
            .center()
            .maybe_child(match &self.row {
                Row::Folder { open, .. } => Some(
                    label()
                        .text(if *open { "▾" } else { "▸" })
                        .font_size(9.)
                        .color(c().text_faint),
                ),
                Row::Doc { .. } => None,
            });

        let rel_click = rel.clone();
        let row = self.row.clone();
        let rel_plus = rel.clone();
        let rel_trash = rel.clone();
        let is_folder = matches!(self.row, Row::Folder { .. });

        rect()
            .width(Size::fill())
            .height(Size::px(24.))
            .horizontal()
            .content(Content::Flex)
            .cross_align(Alignment::Center)
            // the right end clears the scrollbar (8px expanded + its margin), so
            // the trash and "+" never sit under it when the tree scrolls
            .padding((0., 14., 0., 12. + depth as f32 * 14.))
            .spacing(6.)
            .background(if active || hov {
                c().surface_raised
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_pointer_down({
                let rel = rel.clone();
                let is_doc = matches!(self.row, Row::Doc { .. });
                move |e: Event<PointerEventData>| {
                    if let PointerEventData::Mouse(m) = &*e
                        && m.button == Some(MouseButton::Right)
                    {
                        let g = e.global_location();
                        let mut menu = app.tree_menu;
                        menu.set(Some((rel.clone(), g.x as f32, g.y as f32)));
                        return;
                    }
                    let mut last = last_click;
                    let (t0, r0) = last.read().clone();
                    if t0.elapsed().as_millis() < 400 && r0 == rel {
                        // a double-click names it in place
                        let mut naming = app.naming;
                        naming.set(Some(rel.clone()));
                        let mut d = app.drag_doc;
                        d.set(None);
                        return;
                    }
                    last.set((std::time::Instant::now(), rel.clone()));
                    // pressing a note may become a drag onto the board
                    if is_doc {
                        let mut d = app.drag_doc;
                        d.set(Some(rel.clone()));
                    }
                }
            })
            .on_press(move |_| match &row {
                Row::Folder { rel, .. } => {
                    let mut set = collapsed.write();
                    if !set.remove(rel) {
                        set.insert(rel.clone());
                    }
                }
                Row::Doc { .. } => app.open(&rel_click),
            })
            .child(chevron)
            .child(
                rect()
                    .opacity(spine_alpha(is_folder, own, tone))
                    .child(Spine {
                        key: rel.clone(),
                        tone,
                        height: Size::px(12.),
                        glow: false,
                    }),
            )
            .child(rect().width(Size::flex(1.)).child(name_el))
            .maybe_child(open_mark.then(|| label().text("●").font_size(7.).color(c().tone(tone))))
            // the hovered row's trash, a folder's taking what it holds along:
            // a note's in the "+" column (the mock), a folder's beside its "+"
            .maybe_child((!is_naming).then(|| TrashBtn {
                shown: hov,
                keeps_room: !is_folder,
                on: EventHandler::new(move |_| app.trash(std::slice::from_ref(&rel_trash))),
            }))
            .maybe_child((is_folder && !is_naming).then(|| PlusBtn {
                on: EventHandler::new(move |_| new_note_in(app, rel_plus.clone())),
            }))
    }
}

/// How strongly a row's line shows (design 5c): its own colour full, an
/// inherited one dimmer, a folder with no colour at all a neutral grey.
fn spine_alpha(folder: bool, own: bool, tone: crate::index::Tone) -> f32 {
    match (own, folder) {
        (true, _) => 1.,
        (false, true) if tone == crate::index::Tone::Neutral => 0.6,
        (false, true) => 0.45,
        (false, false) => 0.5,
    }
}

/// A folder row's "+": a new note inside it. Always there, as in 5c.
#[derive(PartialEq)]
struct PlusBtn {
    on: EventHandler<()>,
}

impl Component for PlusBtn {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let on = self.on.clone();
        let hov = *hover.read();
        rect()
            .width(Size::px(BTN))
            .height(Size::px(BTN))
            .center()
            .corner_radius(3.)
            .background(if hov {
                c().surface_overlay
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                on.call(())
            })
            .child(label().text("+").font_size(TEXT_SM).color(if hov {
                c().accent
            } else {
                c().text_faint
            }))
    }
}

/// The row buttons' target: wider and taller than their glyphs (the mock's
/// 16), so a pointer near the icon still lands on it.
const BTN: f32 = 20.;

/// A hovered row's trash: faint, red under the pointer.
#[derive(PartialEq)]
struct TrashBtn {
    /// The row is hovered. The button's own hover keeps it up too: the row
    /// may hear the pointer leave as it moves onto the button.
    shown: bool,
    /// A note's column is there hovered or not, so the dots line up with
    /// the folders' "+"; a folder's opens only as it shows.
    keeps_room: bool,
    on: EventHandler<()>,
}

impl Component for TrashBtn {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let on = self.on.clone();
        let hov = *hover.read();
        let shown = self.shown || hov;
        rect()
            .width(Size::px(if shown || self.keeps_room { BTN } else { 0. }))
            .height(Size::px(BTN))
            .center()
            .corner_radius(3.)
            .background(if hov {
                c().surface_overlay
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |e: Event<PressEventData>| {
                e.stop_propagation();
                if shown {
                    on.call(())
                }
            })
            .maybe_child(shown.then(|| {
                crate::icons::icon(
                    crate::icons::TRASH,
                    11.,
                    if hov { c().danger } else { c().text_faint },
                )
            }))
    }
}

/// The tree's right-click menu. `x`, `y`: the click, window coordinates.
#[derive(PartialEq)]
struct TreeMenu {
    rel: String,
    x: f32,
    y: f32,
}

impl Component for TreeMenu {
    // a new click is a new menu, measured afresh: a folder's has a row more
    fn render_key(&self) -> DiffKey {
        (&(&self.rel, self.x.to_bits(), self.y.to_bits())).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut size = use_state(|| None::<(f32, f32)>);
        let rel = self.rel.clone();
        let folder = rel.ends_with('/');
        let (x, y) = (self.x, self.y);
        let close = move || {
            let mut m = app.tree_menu;
            m.set(None);
        };
        let mut rows: Vec<MenuRow> = vec![];
        if folder {
            let r = rel.clone();
            rows.push((
                "new note here",
                "",
                Box::new(move || new_note_in(app, r.clone())),
            ));
            let r = rel.clone();
            rows.push((
                "new folder here",
                "",
                Box::new(move || crate::new_folder(app, &r)),
            ));
        } else {
            let r = rel.clone();
            rows.push(("open", "", Box::new(move || app.open(&r))));
        }
        let r = rel.clone();
        rows.push((
            "rename",
            "dbl-click",
            Box::new(move || {
                let mut naming = app.naming;
                naming.set(Some(r.clone()));
            }),
        ));
        let r = rel.clone();
        rows.push((
            "colour…",
            "",
            Box::new(move || {
                let mut pick = app.color_pick;
                pick.set(Some(crate::app::ColorPick::at(r.clone(), x + 4., y)));
            }),
        ));
        let r = rel.clone();
        rows.push((
            "move to trash",
            "⌫",
            Box::new(move || app.trash(std::slice::from_ref(&r))),
        ));
        let rows: Vec<_> = rows
            .into_iter()
            .map(|(l, k, f)| (l, k, std::rc::Rc::new(f)))
            .collect();
        // ⚠️ global, not absolute: this sits inside the tree, which starts
        // right of the rail and under the window bar, and the click was taken
        // from the window's corner — absolute put it that much lower and right
        let window = crate::overlays::window_area();
        let (mx, my) = crate::overlays::menu_at((x, y), size.read().unwrap_or_default(), window);
        rect()
            .position(Position::new_global().left(0.).top(0.))
            .width(Size::px(0.))
            .height(Size::px(0.))
            .layer(Layer::Overlay)
            .child(
                // a click anywhere else closes it
                rect()
                    .position(Position::new_global().left(0.).top(0.))
                    .width(Size::px(window.width()))
                    .height(Size::px(window.height()))
                    .on_press(move |_| close()),
            )
            .child(
                rect()
                    .position(Position::new_global().left(mx).top(my))
                    // hidden until measured: it can only flip knowing its size
                    .opacity(if size.read().is_some() { 1. } else { 0. })
                    .on_sized(move |e: Event<SizedEventData>| {
                        size.set_if_modified(Some((e.area.width(), e.area.height())))
                    })
                    // the design's menu (5c, the board's right-click)
                    .min_width(Size::px(204.))
                    .padding(4.)
                    .background(c().surface_overlay)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(6.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .children(rows.into_iter().map(|(l, k, f)| {
                        MenuItem {
                            text: l,
                            key: k,
                            on: EventHandler::new(move |_| {
                                close();
                                f();
                            }),
                        }
                        .into_element()
                    })),
            )
    }
}

/// A menu row that lights up under the pointer, as the design's do.
#[derive(PartialEq)]
struct MenuItem {
    text: &'static str,
    key: &'static str,
    on: EventHandler<()>,
}

impl Component for MenuItem {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let on = self.on.clone();
        let danger = self.text == "move to trash";
        rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::Flex)
            .padding((6., 8.))
            .corner_radius(3.)
            .background(if *hover.read() {
                c().surface_raised
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |_| on.call(()))
            .child(rect().width(Size::flex(1.)).child(
                label().text(self.text).font_size(TEXT_SM).color(if danger {
                    c().danger
                } else {
                    c().text_primary
                }),
            ))
            .child(
                label()
                    .text(crate::keys::show(self.key))
                    .font_size(TEXT_XS)
                    .color(c().text_faint),
            )
    }
}

/// A context-menu row: its label, its shortcut, and what it does.
type MenuRow = (&'static str, &'static str, Box<dyn Fn()>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Tone;

    #[test]
    fn a_line_shows_as_strongly_as_its_colour_is_its_own() {
        assert_eq!(spine_alpha(true, true, Tone::Slot(0)), 1.);
        assert_eq!(spine_alpha(false, true, Tone::Slot(0)), 1.);
        assert_eq!(spine_alpha(true, false, Tone::Slot(0)), 0.45);
        assert_eq!(spine_alpha(true, false, Tone::Neutral), 0.6);
        assert_eq!(spine_alpha(false, false, Tone::Slot(0)), 0.5);
        assert_eq!(spine_alpha(false, false, Tone::Neutral), 0.5);
    }
}
