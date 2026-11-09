//! The 24px status bar: the notes / split / canvas switch whose highlight
//! slides between them, and beside it the graph toggle (⌘G).
use freya::animation::*;
use freya::prelude::*;

use crate::{
    app::{App, View},
    base_name,
    theme::*,
};

/// A switch cell's height: one 10px line at `--leading-normal` (1.55), as
/// the mock's cells and graph button are tall.
const CELL_H: f32 = 15.5;

const MODES: [(View, &str); 3] = [
    (View::Notes, "notes"),
    (View::Split, "split"),
    (View::Canvas, "canvas"),
];
/// Mono 10px: 6px a cell, plus 6px padding a side.
fn width_of(s: &str) -> f32 {
    s.len() as f32 * 6. + 12.
}

#[derive(PartialEq)]
pub struct StatusBar {}

impl Component for StatusBar {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let caret = *app.caret.read();
        let mut view = app.view;
        let sep = || label().text("·").color(c().text_faint);
        let current = *view.read();
        let slot = |v: View| {
            let mut x = 0.;
            for (m, name) in MODES {
                if m == v {
                    return (x, width_of(name));
                }
                x += width_of(name);
            }
            (0., 0.)
        };
        let pill = use_animation_transition(use_memo(move || *view.read()), move |from, to| {
            let (a, b) = (slot(from), slot(to));
            (
                AnimNum::new(a.0, b.0)
                    .time(260)
                    .function(Function::Expo)
                    .ease(Ease::Out),
                AnimNum::new(a.1, b.1)
                    .time(260)
                    .function(Function::Expo)
                    .ease(Ease::Out),
            )
        });
        let (px, pw) = {
            let p = pill.read();
            (p.0.value(), p.1.value())
        };
        let graph = current == View::Graph;
        let note = app.note.read().clone();
        let saving = !app.dirty.read().is_empty();
        let dark = *app.dark.read();
        let name = if note.is_empty() {
            "—".to_string()
        } else {
            base_name(&note).to_string()
        };
        // the caret re-renders this bar on every key: count the graph only
        // when it shows, and once per link rather than per note × link
        let middle = if graph {
            let index = app.index.read();
            let linked: std::collections::HashSet<&str> = index
                .links
                .iter()
                .flat_map(|l| [l.from.as_str(), l.to.as_str()])
                .collect();
            let notes = index
                .docs
                .iter()
                .filter(|d| d.kind == crate::index::DocKind::Note);
            let n_notes = notes.clone().count();
            let unlinked = notes.filter(|d| !linked.contains(d.rel.as_str())).count();
            let n_links = index.links.len();
            format!("{n_notes} notes · {n_links} links · {unlinked} unlinked")
        } else {
            format!("{} words", caret.words)
        };

        rect()
            .width(Size::fill())
            .height(Size::px(24.))
            .horizontal()
            .cross_align(Alignment::Center)
            .padding((0., 12.))
            .spacing(8.)
            .content(Content::Flex)
            .background(c().surface_panel)
            .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                top: 1.,
                ..Default::default()
            }))
            .font_size(TEXT_XS)
            .color(c().text_muted)
            .child(
                rect()
                    .background(c().accent_dim)
                    .corner_radius(3.)
                    .padding((0., 6.))
                    .child(
                        label()
                            .text(if graph { "GRAPH" } else { "MD" })
                            .color(c().accent)
                            .font_weight(FontWeight::MEDIUM),
                    ),
            )
            .child(label().text(name))
            .child(sep())
            .child(label().text(middle))
            .maybe_child((!graph).then(sep))
            .maybe_child(
                (!graph)
                    .then(|| label().text(format!("Ln {}, Col {}", caret.row + 1, caret.col + 1))),
            )
            .child(rect().width(Size::flex(1.)))
            .child(
                rect()
                    .horizontal()
                    .background(c().surface_inset)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(3.)
                    // ⚠️ Freya draws a border inside the box, over its padding:
                    // 1px of border plus the mock's 1px gap around the pill. An
                    // absolute child counts from inside that padding already.
                    .padding(2.)
                    .child(
                        rect()
                            .position(Position::new_absolute().left(px).top(0.))
                            .width(Size::px(pw))
                            .height(Size::px(CELL_H))
                            .corner_radius(2.)
                            .background(c().surface_raised),
                    )
                    .children(MODES.iter().map(|(m, name)| {
                        let m = *m;
                        rect()
                            .width(Size::px(width_of(name)))
                            .height(Size::px(CELL_H))
                            .center()
                            .on_press(move |_| {
                                crate::note::flush_note();
                                view.set(m)
                            })
                            .child(label().text(*name).color(if m == current {
                                c().text_primary
                            } else {
                                c().text_faint
                            }))
                            .into_element()
                    })),
            )
            .child(GraphToggle { on: graph, dark })
            .child(sep())
            // no board in the graph view, so no board zoom (graph spec §2.4)
            .maybe_child(
                (!graph).then(|| {
                    label().text(format!("canvas {:.0}%", *app.canvas_zoom.read() * 100.))
                }),
            )
            .maybe_child((!graph).then(sep))
            .child(
                label()
                    .text("●")
                    .color(if saving { c().doc[1] } else { c().accent }),
            )
            .child(label().text(if saving { "saving…" } else { "synced" }))
            .child(sep())
            .child(
                label()
                    .text(if dark { "◐ dark" } else { "◑ light" })
                    .on_press(move |_| crate::toggle_theme(app)),
            )
    }
}

/// The status bar's graph button: outlined, accent while the graph is up.
#[derive(PartialEq)]
struct GraphToggle {
    on: bool,
    /// ⚠️ the theme is a prop so a flip re-renders this: the bar sits
    /// outside the root's theme key, and equal props skip the render.
    dark: bool,
}

impl Component for GraphToggle {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let on = self.on;
        let fg = if on {
            c().accent
        } else if *hover.read() {
            c().text_primary
        } else {
            c().text_faint
        };
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(4.)
            .padding((0., 6.))
            .corner_radius(3.)
            .border(
                Border::new()
                    .fill(if on {
                        c().accent
                    } else if *hover.read() {
                        c().border_strong
                    } else {
                        c().border_subtle
                    })
                    .width(1.),
            )
            .background(if on {
                c().accent_dim
            } else {
                Color::TRANSPARENT
            })
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |_| app.toggle_graph())
            .height(Size::px(CELL_H + 2.))
            .child(crate::icons::icon(crate::icons::GRAPH, 11., fg))
            .child(label().text("graph").color(fg))
    }
}
