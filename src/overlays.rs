//! ⌘K (commands and file names), ⌘P (documents, with a folder column that
//! scopes the list — design 1b) and ⌘⇧F (text across the vault). One panel:
//! a field, a list, ↑↓ to move, ⏎ to run, Esc or a click outside to leave.
use freya::animation::*;
use freya::prelude::*;

use crate::{
    app::{App, Overlay, View},
    base_name,
    field::Field,
    index::{DocKind, Index},
    theme::*,
};

/// The window bar's height where the app draws one (design `macChrome` and
/// `winChrome`), which the design keeps above the scrim: its buttons and its
/// drag still work with an overlay open.
const BAR: f32 = if crate::titlebar::shown() {
    crate::titlebar::HEIGHT
} else {
    0.
};

#[derive(PartialEq)]
pub struct Overlays {}

impl Component for Overlays {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let overlay = app.overlay.read().clone();
        let open = overlay != Overlay::None;
        if std::env::var_os("SYNAPSO_DEBUG").is_some() {
            eprintln!("overlays render {overlay:?}");
        }
        let anim = use_animation_transition(crate::use_tracked(open), |from, to| {
            let v = |b: bool| if b { 1. } else { 0. };
            AnimNum::new(v(from), v(to))
                .time(160)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        let t = anim.read().value();
        if !open {
            return rect().into_element();
        }
        let mut close = app.overlay;
        // ⌘P is the three columns of `design/Workspace.dc.html`'s picker panel
        let width = match overlay {
            Overlay::Picker => 920.,
            Overlay::Palette => 580.,
            _ => 760.,
        };
        rect()
            .position(Position::new_absolute().left(0.).top(BAR))
            .width(Size::percent(100.))
            .height(Size::percent(100.))
            .layer(Layer::Overlay)
            .background(scrim(t))
            .on_press(move |_| close.set(Overlay::None))
            .child(
                rect()
                    .width(Size::fill())
                    // the panel stays 90 from the window's top
                    .padding((90. - BAR - 12. * (1. - t), 0., 0., 0.))
                    .cross_align(Alignment::Center)
                    .child(
                        rect()
                            .width(Size::px(width))
                            .opacity(t)
                            .background(c().surface_overlay)
                            .border(Border::new().fill(c().border_strong).width(1.))
                            .corner_radius(6.)
                            .shadow(Shadow::new().y(12.).blur(32.).color(c().shadow(1.2)))
                            .overflow(Overflow::Clip)
                            .on_press(|e: Event<PressEventData>| e.stop_propagation())
                            .child(match overlay {
                                Overlay::Picker => Picker {
                                    key: 1,
                                    link: false,
                                }
                                .into_element(),
                                Overlay::Link => Picker { key: 4, link: true }.into_element(),
                                Overlay::Palette => Palette { key: 2 }.into_element(),
                                Overlay::Search => Search { key: 3 }.into_element(),
                                Overlay::Trash => Trash { key: 5 }.into_element(),
                                Overlay::None => rect().into_element(),
                            }),
                    ),
            )
            .into_element()
    }
}

/// The scrim, faded in by `t`. ⚠️ `with_a` scales the alpha the colour
/// already has, so it takes the fade alone.
fn scrim(t: f32) -> Color {
    c().scrim.with_a((255. * t) as u8)
}

/// A list row's height in any of the three.
const ROW_H: f32 = 26.;
/// ⌘P's body: the folder column and the list, each padded 6 inside it.
const LIST_H: f32 = 320.;

/// A list row in any of the three.
fn row(
    selected: bool,
    children: Vec<Element>,
    on: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    row_on(selected, c().surface_raised, children, on)
}

/// ⌘K's row (Workspace, the palette): the selection is accent-dim with a
/// 2px accent bar at its left edge, and the rest read muted.
fn palette_row(
    selected: bool,
    mut children: Vec<Element>,
    on: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    if selected {
        children.insert(
            0,
            rect()
                .position(Position::new_absolute().left(-12.).top(4.))
                .width(Size::px(2.))
                .height(Size::px(18.))
                .corner_radius(1.)
                .background(c().accent)
                .into_element(),
        );
    }
    row_on(selected, c().accent_dim, children, on)
}

fn row_on(
    selected: bool,
    selected_bg: Color,
    children: Vec<Element>,
    on: impl FnMut(Event<PressEventData>) + 'static,
) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::px(ROW_H))
        .horizontal()
        .cross_align(Alignment::Center)
        .content(Content::Flex)
        .padding((0., 12.))
        .spacing(8.)
        .corner_radius(3.)
        .background(if selected {
            selected_bg
        } else {
            Color::TRANSPARENT
        })
        .on_press(on)
        .children(children)
        .into_element()
}

fn header(prompt: &'static str, field: Field, trailing: Option<Element>) -> impl IntoElement {
    // ⌘K's › is the accent; the other prompts are faint (Workspace)
    let prompt_color = if prompt == "›" {
        c().accent
    } else {
        c().text_faint
    };
    rect()
        .width(Size::fill())
        .height(Size::px(40.))
        .horizontal()
        .cross_align(Alignment::Center)
        .content(Content::Flex)
        .padding((0., 16.))
        .spacing(8.)
        .border(Border::new().fill(c().border_subtle).width(BorderWidth {
            bottom: 1.,
            ..Default::default()
        }))
        .child(label().text(prompt).color(prompt_color))
        .maybe_child(trailing)
        .child(rect().width(Size::flex(1.)).child(field.size(TEXT_BASE)))
        .child(crate::keys::chip("esc"))
}

fn footer(hints: &'static str) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .padding((6., 16.))
        .border(Border::new().fill(c().border_subtle).width(BorderWidth {
            top: 1.,
            ..Default::default()
        }))
        .child(label().text(hints).font_size(TEXT_XS).color(c().text_faint))
}

/// Fuzzy enough: every query word appears in the haystack.
fn matches(hay: &str, q: &str) -> bool {
    let hay = hay.to_lowercase();
    q.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

fn ago(t: Option<std::time::SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let m = t.elapsed().unwrap_or_default().as_secs() / 60;
    match m {
        0 => "just now".into(),
        1..=59 => format!("{m}m ago"),
        60..=1439 => format!("{}h ago", m / 60),
        _ => format!("{}d ago", m / 1440),
    }
}

// --- ⌘P ------------------------------------------------------------------

#[derive(PartialEq)]
struct Picker {
    key: u8,
    /// Answering a `[[`: the choice is written into the note, not opened.
    link: bool,
}

impl Component for Picker {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let query = use_state(String::new);
        let mut sel = use_state(|| 0usize);
        let mut scroll = use_scroll_controller(ScrollConfig::default);
        // the folder column scopes the list; it starts on the open note's folder
        let link = self.link;
        let mut folder = use_state(|| {
            if link {
                String::new()
            } else {
                crate::index::parent_folder(&app.note.read())
            }
        });
        let index = app.index.read();
        let q = query.read().clone();
        let scope = folder.read().clone();
        let pick = |scope: &str| -> Vec<(String, String)> {
            index
                .docs
                .iter()
                .filter(|d| d.rel.starts_with(scope))
                .filter(|d| q.is_empty() || matches(&d.rel, &q))
                .map(|d| (d.rel.clone(), ago(d.modified)))
                .collect()
        };
        // nothing here but something elsewhere: show that, not "create"
        let mut docs = pick(&scope);
        let widened = docs.is_empty() && !q.is_empty() && !scope.is_empty();
        if widened {
            docs = pick("");
        }
        let folders: Vec<(String, String, usize, usize)> =
            std::iter::once((String::new(), "all".to_string(), 0usize, index.docs.len()))
                .chain(index.folders.iter().map(|f| {
                    let n = index
                        .docs
                        .iter()
                        .filter(|d| d.rel.starts_with(&f.rel))
                        .count();
                    (f.rel.clone(), f.name.clone(), f.depth, n)
                }))
                .collect();
        let tones: Vec<_> = folders
            .iter()
            .map(|(rel, ..)| {
                if rel.is_empty() {
                    crate::index::Tone::Neutral
                } else {
                    index.tone(rel)
                }
            })
            .collect();
        let doc_tones: Vec<_> = docs.iter().map(|(rel, _)| index.tone(rel)).collect();
        drop(index);

        let offer = docs.is_empty() && !q.trim().is_empty();
        let count = docs.len() + offer as usize;
        let s = (*sel.read()).min(count.saturating_sub(1));
        // ⌘P previews the row under the cursor; the `[[` picker has no room
        let shown = docs
            .get(s)
            .filter(|_| !link)
            .and_then(|(rel, _)| app.index.read().doc(rel).cloned());
        // and its list is the design's 430, the preview past its edge
        let (list_w, edge) = if link {
            (Size::fill(), 0.)
        } else {
            (Size::px(430.), 1.)
        };
        let edge = Border::new().fill(c().border_subtle).width(BorderWidth {
            right: edge,
            ..Default::default()
        });
        let docs_submit = docs.clone();
        let scope_submit = scope.clone();
        let q_submit = q.clone();
        let mut overlay = app.overlay;

        let submit = move |_v: String| {
            // the row on screen: a narrower filter clamps the selection
            if let Some((rel, _)) = docs_submit.get(s) {
                if link {
                    let mut ins = app.insert;
                    ins.set(Some(mention_for(rel)));
                } else {
                    app.open(rel);
                }
            } else if offer {
                create_named(app, &scope_submit, &q_submit, DocKind::Note);
            }
            overlay.set(Overlay::None);
        };
        let field = Field::new(query, submit, move |_| overlay.set(Overlay::None))
            .placeholder("filter…")
            .on_arrow(move |d: i32| {
                let n = count.max(1) as i32;
                // the live selection, clamped as the render clamps it: two
                // presses before the next frame both count
                let cur = (*sel.peek()).min(count.saturating_sub(1)) as i32;
                let next = (cur + d).rem_euclid(n) as usize;
                sel.set(next);
                // the list follows the highlight, the wrap-arounds included
                let top = next as f32 * ROW_H;
                let (_, y): (i32, i32) = scroll.into();
                let (from, view) = (-y as f32, LIST_H - 12.);
                if top < from {
                    scroll.scroll_to_y(-(top as i32));
                } else if top + ROW_H > from + view {
                    scroll.scroll_to_y(-((top + ROW_H - view) as i32));
                }
            });
        // 1b: the scope rides in the field as a raised chip
        let chip = (!scope.is_empty()).then(|| {
            rect()
                .horizontal()
                .spacing(6.)
                .cross_align(Alignment::Center)
                .padding((0., 6.))
                .corner_radius(3.)
                .background(c().surface_raised)
                .border(Border::new().fill(c().border_subtle).width(1.))
                .child(
                    rect()
                        .width(Size::px(2.))
                        .height(Size::px(10.))
                        .background(c().tone(app.index.read().tone(&scope))),
                )
                .child(
                    label()
                        .text(format!("{} /", scope.trim_end_matches('/')))
                        .font_size(TEXT_SM)
                        .color(c().text_primary),
                )
                .into_element()
        });

        rect()
            .width(Size::fill())
            .child(header("⌕", field, chip))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(LIST_H))
                    .horizontal()
                    .child(
                        // the folder column
                        rect()
                            .width(Size::px(160.))
                            .height(Size::fill())
                            .padding(6.)
                            // 1b: the column is a quieter panel than the list
                            .background(c().surface_panel)
                            .border(Border::new().fill(c().border_subtle).width(BorderWidth { right: 1., ..Default::default() }))
                            .child(
                                ScrollView::new().height(Size::fill()).child(
                                    rect()
                                        .width(Size::fill())
                                        .children(folders.into_iter().zip(tones).map(|((rel, name, depth, n), tone)| {
                                            let active = rel == scope;
                                            let r = rel.clone();
                                            rect()
                                                .width(Size::fill())
                                                .height(Size::px(24.))
                                                .horizontal()
                                                .content(Content::Flex)
                                                .cross_align(Alignment::Center)
                                                .padding((0., 8., 0., 8. + depth as f32 * 12.))
                                                .spacing(6.)
                                                .corner_radius(3.)
                                                .background(if active { c().surface_raised } else { Color::TRANSPARENT })
                                                .on_press(move |_| {
                                                    folder.set(r.clone());
                                                    sel.set(0);
                                                    scroll.scroll_to_y(0);
                                                })
                                                .child(rect().width(Size::px(2.)).height(Size::px(12.)).background(if rel.is_empty() { c().border_strong } else { c().tone(tone) }))
                                                .child(rect().width(Size::flex(1.)).child(label().text(name).font_size(TEXT_SM).color(if active { c().text_primary } else { c().text_muted }).max_lines(1)))
                                                .child(label().text(n.to_string()).font_size(TEXT_XS).color(c().text_faint))
                                                .into_element()
                                        }))
                                        .child(
                                            paragraph()
                                                .margin((6., 8.))
                                                .font_size(TEXT_SM)
                                                .span(Span::new("+ ").color(c().accent))
                                                .span(Span::new("folder").color(c().text_faint))
                                                .on_press(move |_| {
                                                    let parent = folder.read().clone();
                                                    crate::new_folder(app, &parent);
                                                    let mut t = app.tree;
                                                    t.set(true);
                                                    overlay.set(Overlay::None);
                                                }),
                                        ),
                                ),
                            ),
                    )
                    .child(
                        rect().width(list_w).height(Size::fill()).padding(6.).border(edge).child(
                            ScrollView::new_controlled(scroll).height(Size::fill()).child(
                                rect()
                                    .width(Size::fill())
                                    .children(docs.iter().zip(doc_tones).enumerate().map(|(i, ((rel, when), tone))| {
                                        let r = rel.clone();
                                        let canvas = rel.ends_with(".canvas");
                                        row(
                                            i == s,
                                            vec![
                                                rect().width(Size::px(2.)).height(Size::px(13.)).background(c().tone(tone)).into_element(),
                                                rect()
                                                    .width(Size::flex(1.))
                                                    .child(
                                                        paragraph()
                                                            .max_lines(1)
                                                            .font_size(TEXT_SM)
                                                            .span(Span::new(if canvas { "◇ " } else { "" }).color(c().text_faint))
                                                            .span(Span::new(base_name(rel).to_string()).color(c().text_primary))
                                                            .span(Span::new(format!("  {}", crate::index::parent_folder(rel))).color(c().text_faint)),
                                                    )
                                                    .into_element(),
                                                label().text(when.clone()).font_size(TEXT_XS).color(c().text_faint).into_element(),
                                            ],
                                            move |_| {
                                                if link {
                                                    let mut ins = app.insert;
                                                    ins.set(Some(mention_for(&r)));
                                                } else {
                                                    app.open(&r);
                                                }
                                                overlay.set(Overlay::None);
                                            },
                                        )
                                    }))
                                    .maybe_child(offer.then(|| {
                                        let q2 = q.clone();
                                        let scope2 = scope.clone();
                                        row(
                                            s == 0,
                                            vec![
                                                label().text("+").color(c().accent).into_element(),
                                                label()
                                                    .text(format!("new note: {}{}.md", scope, q.trim()))
                                                    .font_size(TEXT_SM)
                                                    .color(c().text_primary)
                                                    .into_element(),
                                            ],
                                            move |_| {
                                                create_named(app, &scope2, &q2, DocKind::Note);
                                                overlay.set(Overlay::None);
                                            },
                                        )
                                    })),
                            ),
                        ),
                    )
                    .maybe_child((!link).then(|| preview(app, shown))),
            )
            .child(footer(if widened {
                "nothing in this folder — showing the whole vault"
            } else {
                "↑↓ move · ⏎ open · click a folder to scope · typing a name that isn't there offers to create it"
            }))
    }
}

/// ⌘P's third column, `design/Workspace.dc.html`'s picker preview: what the
/// row under the cursor is before ⏎ commits to it — its colour and name, its
/// opening (a board's shapes), when it was edited. A note's comes off the
/// index; a board is read off its file, the one row under the cursor per ↑↓.
fn preview(app: App, doc: Option<crate::index::DocEntry>) -> Element {
    let pane = rect()
        .width(Size::fill())
        .height(Size::fill())
        .padding(16.)
        .spacing(12.)
        .content(Content::Flex)
        .background(c().surface_panel);
    let Some(doc) = doc else {
        return pane.into_element();
    };
    let color = c().tone(app.index.read().tone(&doc.rel));
    let rel = doc.rel.clone();
    let body = if doc.kind == DocKind::Canvas {
        // a board shows its shapes, the way its minimap draws them
        let nodes = synapso_core::vault::Canvas::load(&doc.path)
            .map(|board| board.nodes)
            .unwrap_or_default();
        crate::board::shapes(nodes).into_element()
    } else {
        label()
            .text(doc.excerpt)
            .font_size(TEXT_XS)
            .line_height(1.55)
            .color(c().text_muted)
            .into_element()
    };
    pane.child(
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(8.)
            .child(
                rect()
                    .width(Size::px(2.))
                    .height(Size::px(16.))
                    .background(color)
                    .shadow(Shadow::new().blur(8.).color(color)),
            )
            .child(
                label()
                    .text(base_name(&doc.rel).to_string())
                    .max_lines(1)
                    .font_size(TEXT_MD)
                    .font_weight(FontWeight::MEDIUM),
            ),
    )
    // ⚠️ the opening without the title the design prints over it: the index
    // keeps a note's body only (`Lines::body_preview`), and the name is
    // already on the line above
    .child(
        rect()
            .width(Size::fill())
            .height(Size::flex(1.))
            .padding(12.)
            .corner_radius(4.)
            .background(c().surface_inset)
            .border(Border::new().fill(c().border_subtle).width(1.))
            .overflow(Overflow::Clip)
            .child(body),
    )
    // the design's meta rows on a 64px label column: `edited` is the one the
    // index can answer (`size` and `canvas` would read the disk)
    .child(
        rect()
            .horizontal()
            .spacing(12.)
            .font_size(TEXT_XS)
            .child(
                rect()
                    .width(Size::px(64.))
                    .child(label().text("edited").color(c().text_muted)),
            )
            .child(label().text(ago(doc.modified)).color(c().text_faint)),
    )
    .child(
        rect()
            .width(Size::fill())
            .horizontal()
            .main_align(Alignment::End)
            .child(
                rect()
                    .background(c().accent)
                    .corner_radius(4.)
                    .padding((6., 12.))
                    .on_press(move |_| {
                        app.open(&rel);
                        let mut o = app.overlay;
                        o.set(Overlay::None);
                    })
                    .child(
                        label()
                            .text("open ↵")
                            .font_size(TEXT_SM)
                            .font_weight(FontWeight::MEDIUM)
                            .color(c().on_accent),
                    ),
            ),
    )
    .into_element()
}

/// Create `folder/name.ext` (making folders on the way for `a/b/name`),
/// refusing a name that exists.
pub fn create_named(app: App, folder: &str, name: &str, kind: DocKind) {
    let root = app.root();
    let name = name
        .trim()
        .trim_end_matches(".md")
        .trim_end_matches(".canvas");
    let (sub, stem) = name
        .rsplit_once('/')
        .map(|(a, b)| (format!("{a}/"), b))
        .unwrap_or((String::new(), name));
    let stem = synapso_core::vault::sanitize(stem);
    if stem.is_empty() {
        return;
    }
    let ext = if kind == DocKind::Canvas {
        "canvas"
    } else {
        "md"
    };
    let dir = root.join(folder).join(&sub);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{stem}.{ext}"));
    if path.exists() {
        app.toast(
            format!("{stem}.{ext} already exists"),
            "names that already exist are refused",
            None,
        );
        return;
    }
    let body = if kind == DocKind::Canvas {
        "{}".to_string()
    } else {
        format!("# {stem}\n\n")
    };
    if synapso_core::vault::write(&path, &body).is_ok() {
        app.reindex();
        let rel = crate::index::rel_of(&root, &path);
        app.open(&rel);
    }
}

// --- ⌘K ------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Cmd {
    Theme,
    Notes,
    Split,
    Canvas,
    Graph,
    NewNote,
    NewCanvas,
    NewFolder,
    Folders,
    Save,
    Rename,
    Delete,
    Reload,
    Search,
    Trash,
}

const CMDS: [(Cmd, &str, &str, &str); 15] = [
    (Cmd::NewNote, "cmd", "new note", "⌘N"),
    (Cmd::NewCanvas, "cmd", "new canvas", ""),
    (Cmd::NewFolder, "cmd", "new folder", ""),
    (Cmd::Notes, "view", "notes only", "⌘1"),
    (Cmd::Split, "view", "split", "⌘2"),
    (Cmd::Canvas, "view", "canvas only", "⌘3"),
    (Cmd::Graph, "view", "graph", "⌘G"),
    (Cmd::Folders, "view", "show / hide folders", "⌘⇧E"),
    (Cmd::Theme, "cmd", "switch theme", "⌘⇧L"),
    (Cmd::Search, "cmd", "search the vault", "⌘⇧F"),
    (Cmd::Save, "cmd", "save now", "⌘S"),
    (Cmd::Rename, "cmd", "rename this note…", ""),
    (Cmd::Delete, "cmd", "delete this note", ""),
    (Cmd::Reload, "cmd", "reload vault", ""),
    (Cmd::Trash, "cmd", "trash…", ""),
];

fn run(app: App, cmd: Cmd) {
    let mut view = app.view;
    let mut overlay = app.overlay;
    overlay.set(Overlay::None);
    match cmd {
        Cmd::Theme => crate::toggle_theme(app),
        Cmd::Notes => view.set(View::Notes),
        Cmd::Split => view.set(View::Split),
        Cmd::Canvas => view.set(View::Canvas),
        Cmd::Graph => app.toggle_graph(),
        Cmd::NewNote => crate::new_note(app, None),
        Cmd::NewCanvas => crate::new_canvas(app, None),
        Cmd::NewFolder => {
            let parent = crate::index::parent_folder(&app.note.read());
            crate::new_folder(app, &parent);
            let mut t = app.tree;
            t.set(true);
        }
        Cmd::Folders => {
            let mut t = app.tree;
            let open = *t.read();
            t.set(!open);
        }
        Cmd::Save => {
            crate::note::flush_note();
        }
        Cmd::Rename => {
            let mut naming = app.naming;
            naming.set(Some(app.note.read().clone()));
        }
        Cmd::Delete => {
            let n = app.note.read().clone();
            if !n.is_empty() {
                app.trash(&[n]);
            }
        }
        Cmd::Reload => app.reindex(),
        Cmd::Search => overlay.set(Overlay::Search),
        Cmd::Trash => overlay.set(Overlay::Trash),
    }
}

#[derive(PartialEq)]
struct Palette {
    key: u8,
}

impl Component for Palette {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let query = use_state(String::new);
        let mut sel = use_state(|| 0usize);
        let q = query.read().clone();
        let cmds: Vec<_> = CMDS
            .iter()
            .filter(|(_, _, name, _)| q.is_empty() || matches(name, &q))
            .copied()
            .collect();
        let index = app.index.read();
        let docs: Vec<(String, DocKind)> = if q.is_empty() {
            vec![]
        } else {
            index
                .docs
                .iter()
                .filter(|d| matches(&d.rel, &q))
                .take(12)
                .map(|d| (d.rel.clone(), d.kind))
                .collect()
        };
        drop(index);
        let count = cmds.len() + docs.len();
        let s = (*sel.read()).min(count.saturating_sub(1));
        let mut overlay = app.overlay;
        let cmds2 = cmds.clone();
        let docs2 = docs.clone();
        let submit = move |_v: String| {
            // the row on screen: a narrower filter clamps the selection (⌘P's rule)
            let i = s;
            if let Some((cmd, ..)) = cmds2.get(i) {
                run(app, *cmd);
            } else if let Some((rel, _)) = docs2.get(i - cmds2.len().min(i)) {
                go(app, rel);
                overlay.set(Overlay::None);
            }
        };
        let field = Field::new(query, submit, move |_| overlay.set(Overlay::None))
            .placeholder("type a command or a file name")
            .on_arrow(move |d: i32| {
                let n = count.max(1) as i32;
                // the live selection, clamped as the render clamps it: two
                // presses before the next frame both count
                let cur = (*sel.peek()).min(count.saturating_sub(1)) as i32;
                sel.set(((cur + d).rem_euclid(n)) as usize);
            });
        let name_color = |on: bool| if on { c().text_primary } else { c().text_muted };
        let kind_col = |k: &str| {
            label()
                .width(Size::px(46.))
                .text(k.to_string())
                .font_size(TEXT_XS)
                .color(c().text_faint)
                .into_element()
        };
        let mut rows: Vec<Element> = cmds
            .iter()
            .enumerate()
            .map(|(i, (cmd, kind, name, keys))| {
                let cmd = *cmd;
                palette_row(
                    i == s,
                    vec![
                        kind_col(kind),
                        rect()
                            .width(Size::flex(1.))
                            .child(
                                label()
                                    .text(*name)
                                    .font_size(TEXT_SM)
                                    .color(name_color(i == s)),
                            )
                            .into_element(),
                    ]
                    .into_iter()
                    .chain((!keys.is_empty()).then(|| crate::keys::chip(keys).into_element()))
                    .collect(),
                    move |_| run(app, cmd),
                )
            })
            .collect();
        for (j, (rel, kind)) in docs.iter().enumerate() {
            let r = rel.clone();
            let on = cmds.len() + j == s;
            rows.push(palette_row(
                on,
                vec![
                    kind_col(if *kind == DocKind::Canvas {
                        "canvas"
                    } else {
                        "doc"
                    }),
                    rect()
                        .width(Size::flex(1.))
                        .child(
                            label()
                                .text(rel.clone())
                                .font_size(TEXT_SM)
                                .color(name_color(on)),
                        )
                        .into_element(),
                ],
                move |_| {
                    go(app, &r);
                    overlay.set(Overlay::None);
                },
            ));
        }
        rect()
            .width(Size::fill())
            .child(header("›", field, None))
            .child(
                rect()
                    .width(Size::fill())
                    .max_height(Size::px(380.))
                    .padding(6.)
                    .child(
                        ScrollView::new()
                            .height(Size::auto())
                            .max_height(Size::px(368.))
                            .child(rect().width(Size::fill()).children(rows)),
                    ),
            )
            .child(footer("↑↓ navigate   ↵ run   esc dismiss"))
    }
}

// --- ⌘⇧F -----------------------------------------------------------------

#[derive(PartialEq)]
struct Search {
    key: u8,
}

/// One hit: the file, the line number and the line, split around the match.
#[derive(Clone, PartialEq)]
struct Hit {
    rel: String,
    line: usize,
    before: String,
    matched: String,
    after: String,
    count: usize,
}

fn search(index: &Index, q: &str) -> Vec<Hit> {
    let q = q.trim().to_lowercase();
    if q.len() < 2 {
        return vec![];
    }
    let mut out = vec![];
    for d in index.docs.iter().filter(|d| d.kind == DocKind::Note) {
        let Ok(text) = synapso_core::vault::read(&d.path) else {
            continue;
        };
        let mut first: Option<Hit> = None;
        let mut count = 0;
        for (n, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if let Some(at) = lower.find(&q) {
                count += lower.matches(&q).count();
                if first.is_none()
                    && line.is_char_boundary(at)
                    && line.is_char_boundary(at + q.len())
                {
                    first = Some(Hit {
                        rel: d.rel.clone(),
                        line: n,
                        before: line[..at]
                            .chars()
                            .rev()
                            .take(40)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect(),
                        matched: line[at..at + q.len()].to_string(),
                        after: line[at + q.len()..].chars().take(60).collect(),
                        count: 0,
                    });
                }
            }
        }
        if let Some(mut h) = first {
            h.count = count;
            out.push(h);
        }
    }
    out
}

impl Component for Search {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let query = use_state(String::new);
        let mut sel = use_state(|| 0usize);
        let q = query.read().clone();
        let hits = search(&app.index.read(), &q);
        let total: usize = hits.iter().map(|h| h.count).sum();
        let count = hits.len();
        let s = (*sel.read()).min(count.saturating_sub(1));
        let mut overlay = app.overlay;
        let hits2 = hits.clone();
        let submit = move |_v: String| {
            if let Some(h) = hits2.get(s) {
                app.open(&h.rel);
            }
            overlay.set(Overlay::None);
        };
        let field = Field::new(query, submit, move |_| overlay.set(Overlay::None))
            .placeholder("search every note")
            .on_arrow(move |d: i32| {
                let n = count.max(1) as i32;
                // the live selection, clamped as the render clamps it: two
                // presses before the next frame both count
                let cur = (*sel.peek()).min(count.saturating_sub(1)) as i32;
                sel.set(((cur + d).rem_euclid(n)) as usize);
            });
        let summary = if q.trim().len() < 2 {
            "type two letters or more".to_string()
        } else {
            format!("{total} matches in {count} notes")
        };
        let index = app.index.read();
        let tones: Vec<_> = hits.iter().map(|h| index.tone(&h.rel)).collect();
        drop(index);
        rect()
            .width(Size::fill())
            .child(header("⌕", field, None))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(340.))
                    .padding(6.)
                    .child(ScrollView::new().height(Size::fill()).child(
                        rect().width(Size::fill()).children(
                            hits.iter().zip(tones).enumerate().map(|(i, (h, tone))| {
                                let r = h.rel.clone();
                                rect()
                                    .width(Size::fill())
                                    .padding((6., 12.))
                                    .spacing(3.)
                                    .corner_radius(3.)
                                    .background(if i == s {
                                        c().surface_raised
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .on_press(move |_| {
                                        app.open(&r);
                                        overlay.set(Overlay::None);
                                    })
                                    .child(
                                        rect()
                                            .horizontal()
                                            .spacing(8.)
                                            .cross_align(Alignment::Center)
                                            .child(
                                                rect()
                                                    .width(Size::px(2.))
                                                    .height(Size::px(12.))
                                                    .background(c().tone(tone)),
                                            )
                                            .child(label().text(h.rel.clone()).font_size(TEXT_SM))
                                            .child(
                                                label()
                                                    .text(format!(
                                                        "{} ×  · line {}",
                                                        h.count,
                                                        h.line + 1
                                                    ))
                                                    .font_size(TEXT_XS)
                                                    .color(c().text_faint),
                                            ),
                                    )
                                    .child(
                                        paragraph()
                                            .max_lines(1)
                                            .font_size(TEXT_XS)
                                            .span(
                                                Span::new(format!("…{}", h.before))
                                                    .color(c().text_muted),
                                            )
                                            .span(
                                                Span::new(h.matched.clone())
                                                    .color(c().text_primary)
                                                    .font_weight(FontWeight::BOLD),
                                            )
                                            .span(Span::new(h.after.clone()).color(c().text_muted)),
                                    )
                                    .into_element()
                            }),
                        ),
                    )),
            )
            .child(footer("↑↓ move · ⏎ open · always case-insensitive"))
            .child(
                rect().padding((0., 14., 6., 14.)).child(
                    label()
                        .text(summary)
                        .font_size(TEXT_XS)
                        .color(c().text_faint),
                ),
            )
    }
}

/// What `[[` gets completed with: the name, or `canvas: name` for a board.
fn mention_for(rel: &str) -> String {
    let base = base_name(rel);
    match base.strip_suffix(".canvas") {
        Some(stem) => format!("canvas: {stem}]]"),
        None => format!("{}]]", base.trim_end_matches(".md")),
    }
}

/// ⌘K on a document: in the graph it becomes the focus ("find node"),
/// everywhere else it opens.
fn go(app: App, rel: &str) {
    if *app.view.read() == View::Graph {
        let mut gs = app.graph;
        let mut g = gs.write();
        if g.focus != rel {
            let prev = std::mem::replace(&mut g.focus, rel.to_string());
            g.history.push(prev);
        }
    } else {
        app.open(rel);
    }
}

// --- trash -----------------------------------------------------------------

#[derive(PartialEq)]
struct Trash {
    key: u8,
}

impl Component for Trash {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let root = app.root();
        let dir = root.join(crate::index::TRASH);
        let mut rev = use_state(|| 0u32);
        let _ = *rev.read();
        let items: Vec<(String, Option<std::time::SystemTime>)> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| {
                (
                    e.file_name().to_string_lossy().to_string(),
                    e.metadata().ok().and_then(|m| m.modified().ok()),
                )
            })
            .collect();
        let n = items.len();
        // hold to empty: the button fills while pressed; letting go early cancels
        let mut holding = use_state(|| false);
        let fill = use_animation_transition(crate::use_tracked(*holding.read()), |from, to| {
            let v = |b: bool| if b { 1. } else { 0. };
            AnimNum::new(v(from), v(to)).time(if to { 800 } else { 150 })
        });
        let f = fill.read().value();
        if *holding.read() && f >= 0.999 {
            holding.set(false);
            let _ = std::fs::remove_dir_all(&dir);
            *rev.write() += 1;
            app.toast(
                "trash emptied",
                format!(
                    "{n} {} deleted for good",
                    if n == 1 { "item" } else { "items" }
                ),
                None,
            );
        }
        let dir_restore = dir.clone();
        rect()
            .width(Size::fill())
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(40.))
                    .horizontal()
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .padding((0., 14.))
                    .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                        bottom: 1.,
                        ..Default::default()
                    }))
                    .child(
                        rect().width(Size::flex(1.)).child(
                            label()
                                .text("trash")
                                .font_size(TEXT_SM)
                                .color(c().text_muted),
                        ),
                    )
                    .child(
                        label()
                            .text(format!("{n} {}", if n == 1 { "item" } else { "items" }))
                            .font_size(TEXT_XS)
                            .color(c().text_faint),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .max_height(Size::px(300.))
                    .padding(6.)
                    .child(
                        ScrollView::new()
                            .height(Size::auto())
                            .max_height(Size::px(288.))
                            .child(rect().width(Size::fill()).children(items.into_iter().map(
                                |(name, t)| {
                                    let from = dir_restore.join(&name);
                                    let to = root.join(&name);
                                    rect()
                                        .width(Size::fill())
                                        .height(Size::px(28.))
                                        .horizontal()
                                        .content(Content::Flex)
                                        .cross_align(Alignment::Center)
                                        .padding((0., 10.))
                                        .child(
                                            rect().width(Size::flex(1.)).child(
                                                label()
                                                    .text(name.clone())
                                                    .font_size(TEXT_SM)
                                                    .color(c().text_muted)
                                                    .text_decoration(TextDecoration::LineThrough),
                                            ),
                                        )
                                        .child(
                                            label()
                                                .margin((0., 10.))
                                                .text(ago(t))
                                                .font_size(TEXT_XS)
                                                .color(c().text_faint),
                                        )
                                        .child(
                                            label()
                                                .text("restore")
                                                .font_size(TEXT_XS)
                                                .color(c().text_link)
                                                .on_press(move |_| {
                                                    // a restore never overwrites
                                                    if !to.exists()
                                                        && std::fs::rename(&from, &to).is_ok()
                                                    {
                                                        app.reindex();
                                                        *rev.write() += 1;
                                                    }
                                                }),
                                        )
                                        .into_element()
                                },
                            ))),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .horizontal()
                    .content(Content::Flex)
                    .cross_align(Alignment::Center)
                    .padding((10., 14.))
                    .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                        top: 1.,
                        ..Default::default()
                    }))
                    .child(
                        rect().width(Size::flex(1.)).child(
                            label()
                                .text("deletes for good")
                                .font_size(TEXT_XS)
                                .color(c().text_faint),
                        ),
                    )
                    .child(
                        rect()
                            .width(Size::px(130.))
                            .height(Size::px(26.))
                            .corner_radius(4.)
                            .border(Border::new().fill(c().danger).width(1.))
                            .overflow(Overflow::Clip)
                            // 1e: a danger-dim button that fills solid danger
                            // while held; past half the label turns to the base
                            .background(c().danger_dim)
                            .on_pointer_down(move |_| holding.set(true))
                            .on_global_pointer_up(move |_| holding.set(false))
                            .on_pointer_leave(move |_| holding.set(false))
                            .child(
                                rect()
                                    .position(Position::new_absolute().left(0.).top(0.))
                                    .width(Size::px(130. * f))
                                    .height(Size::fill())
                                    .background(c().danger),
                            )
                            .child(
                                rect()
                                    .width(Size::fill())
                                    .height(Size::fill())
                                    .center()
                                    .child(
                                        label()
                                            .text("hold to empty")
                                            .font_size(TEXT_SM)
                                            .font_weight(FontWeight::MEDIUM)
                                            .color(if f > 0.5 { c().bg_base } else { c().danger }),
                                    ),
                            ),
                    ),
            )
    }
}

/// How far a right-click menu keeps from the edge it is pushed against.
const EDGE: f32 = 4.;

/// Where a right-click menu `size` big puts its top-left corner: on the
/// click `at`; flipped up (its bottom edge on the click) or left (its right
/// edge on it) where it would run out of `bounds`; then kept `EDGE` inside.
/// All in one space, the menu's parent's. The tree's and the board's
/// menus come through here.
pub fn menu_at(at: (f32, f32), size: (f32, f32), bounds: Area) -> (f32, f32) {
    let place = |at: f32, len: f32, min: f32, max: f32| {
        let p = if at + len > max - EDGE { at - len } else { at };
        p.min(max - EDGE - len).max(min + EDGE)
    };
    (
        place(at.0, size.0, bounds.min_x(), bounds.max_x()),
        place(at.1, size.1, bounds.min_y(), bounds.max_y()),
    )
}

/// The window in logical pixels, the space of `global_location` and of a
/// `Position::new_global`. ⚠️ `root_size` is physical: 2x on a Retina screen.
pub fn window_area() -> Area {
    let w = Platform::get().current_window();
    Area::from_size(*w.root_size.read() / *w.scale_factor.read() as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scrim_lands_on_its_token() {
        crate::theme::set_dark(true);
        assert_eq!(scrim(1.), crate::theme::DARK.scrim);
        assert_eq!(scrim(0.).a(), 0);
    }

    #[test]
    fn a_menu_opens_at_its_click_and_turns_where_it_would_run_out() {
        let window = Area::new((0., 0.).into(), (800., 600.).into());
        let menu = (220., 90.);
        // room below and to the right: its corner on the click
        assert_eq!(menu_at((100., 100.), menu, window), (100., 100.));
        // near the bottom: it opens upward, its bottom edge on the click
        assert_eq!(menu_at((100., 560.), menu, window), (100., 470.));
        // near the right: it opens leftward, its right edge on the click
        assert_eq!(menu_at((700., 100.), menu, window), (480., 100.));
        assert_eq!(menu_at((700., 560.), menu, window), (480., 470.));
        // taller than the room either way: kept inside, off the edge
        assert_eq!(menu_at((100., 50.), (220., 580.), window), (100., EDGE));
        // a pane that starts right of the rail and under the bar: its own edges
        let pane = Area::new((300., 32.).into(), (240., 400.).into());
        assert_eq!(menu_at((310., 420.), menu, pane), (310., 330.));
        assert_eq!(menu_at((400., 100.), menu, pane), (300. + EDGE, 100.));
    }
}
