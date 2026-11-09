//! The vertical tab rail: one tab per open document, its name turned 90°,
//! its spine in the document's resolved colour. The folder button on top
//! brings the tree back (⌘B).
use freya::animation::*;
use freya::prelude::*;

use crate::{Spine, app::App, base_name, icons, theme::*};

#[derive(PartialEq)]
pub struct Rail {}

impl Component for Rail {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let tabs = app.tabs.read().clone();
        let mut plus_hover = use_state(|| false);
        let mut tree = app.tree;
        let tree_open = *tree.read();
        let mut folder_hover = use_state(|| false);
        rect()
            .width(Size::px(34.))
            .height(Size::fill())
            .background(c().bg_base)
            .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                right: 1.,
                ..Default::default()
            }))
            .content(Content::Flex)
            .child(
                rect()
                    .width(Size::px(33.))
                    .height(Size::px(34.))
                    .center()
                    .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                        bottom: 1.,
                        ..Default::default()
                    }))
                    .background(if tree_open {
                        c().accent_dim
                    } else if *folder_hover.read() {
                        c().surface_raised
                    } else {
                        Color::TRANSPARENT
                    })
                    .on_pointer_enter(move |_| folder_hover.set(true))
                    .on_pointer_leave(move |_| folder_hover.set(false))
                    .on_press(move |_| tree.set(!tree_open))
                    .child(icons::icon(
                        icons::FOLDER,
                        14.,
                        if tree_open {
                            c().accent
                        } else {
                            c().text_muted
                        },
                    )),
            )
            .child(
                // many open notes scroll with the wheel instead of spilling out
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.))
                    .overflow(Overflow::Clip)
                    .child(
                        ScrollView::new()
                            .width(Size::fill())
                            .height(Size::fill())
                            .show_scrollbar(false)
                            .child(rect().width(Size::fill()).children(tabs.into_iter().map(
                                |rel| {
                                    rect()
                                        .margin((0., 0., 4., 0.))
                                        .child(Tab { rel })
                                        .into_element()
                                },
                            ))),
                    ),
            )
            .child(
                rect()
                    .width(Size::px(33.))
                    .height(Size::px(34.))
                    .margin((0., 0., 6., 0.))
                    .center()
                    .background(if *plus_hover.read() {
                        c().surface_raised
                    } else {
                        c().bg_base
                    })
                    .on_pointer_enter(move |_| plus_hover.set(true))
                    .on_pointer_leave(move |_| plus_hover.set(false))
                    .on_press(move |_| crate::new_note(app, None))
                    .child(
                        label()
                            .text("+")
                            .font_size(TEXT_MD)
                            .color(if *plus_hover.read() {
                                c().text_primary
                            } else {
                                c().text_faint
                            }),
                    ),
            )
    }
}

/// The 16px slot at a tab's end: × while hovered, the unsaved dot when
/// dirty, else nothing — the slot keeps its room so the name never shifts.
/// The board switcher's rows end in one too.
pub fn end_slot(app: App, rel: String, hov: bool, dirty: bool, color: Color) -> impl IntoElement {
    let mut x_hover = use_state(|| false);
    rect()
        .width(Size::px(16.))
        .height(Size::px(16.))
        .center()
        .corner_radius(3.)
        .background(if hov && *x_hover.read() {
            c().surface_overlay
        } else {
            Color::TRANSPARENT
        })
        .on_pointer_enter(move |_| x_hover.set(true))
        .on_pointer_leave(move |_| x_hover.set(false))
        .maybe_child(hov.then(|| {
            let rel = rel.clone();
            rect()
                .expanded()
                .center()
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    app.close_tab(&rel);
                })
                .child(icons::icon(
                    icons::CLOSE,
                    8.,
                    if *x_hover.read() {
                        c().text_primary
                    } else {
                        c().text_muted
                    },
                ))
        }))
        .maybe_child((dirty && !hov).then(|| {
            rect()
                .width(Size::px(5.))
                .height(Size::px(5.))
                .corner_radius(999.)
                .background(color)
        }))
}

#[derive(PartialEq)]
struct Tab {
    rel: String,
}

/// Mono 11px at .02em tracking: the label's run is exactly this long, so the
/// rotated tab can be sized before layout.
fn run(name: &str) -> f32 {
    name.chars().count() as f32 * (TEXT_SM * 0.6 + TEXT_SM * 0.02)
}

fn tone_bg((a, h): (bool, bool)) -> Color {
    if a {
        c().surface_panel
    } else if h {
        c().surface_raised
    } else {
        c().bg_base
    }
}

impl Component for Tab {
    fn render_key(&self) -> DiffKey {
        (&self.rel).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let rel = self.rel.clone();
        let is_active = *app.note.read() == rel
            || (*app.canvas.read() == rel && *app.view.read() == crate::app::View::Canvas);
        let mut hover = use_state(|| false);
        let tone = app.index.read().tone(&rel);
        let color = c().tone(tone);
        let dirty = app.dirty.read().contains(&rel);

        let bg = use_animation_transition(
            crate::use_tracked((is_active, *hover.read())),
            |from, to| AnimColor::new(tone_bg(from), tone_bg(to)).time(140),
        );

        let name = base_name(&rel).to_string();
        let hov = *hover.read();
        let text = run(&name);
        let long = text + 6. + 16. + 32.;
        let open_rel = rel.clone();
        let close_rel = rel.clone();

        rect()
            .width(Size::px(33.))
            .height(Size::px(long))
            .background(&*bg.read())
            // ⚠️ over/out, not enter/leave: those are exclusive, so the tab would
            // count as left the moment the cursor reaches the × — which then
            // unmounts, and the press lands on the tab instead of closing it
            .on_pointer_over(move |_| hover.set(true))
            .on_pointer_out(move |_| hover.set(false))
            .on_press(move |_| app.open(&open_rel))
            .child(
                rect()
                    .position(Position::new_absolute().left(-3.).top(0.))
                    .layer(Layer::Relative(2))
                    .child(Spine {
                        key: rel.clone(),
                        tone,
                        height: Size::px(long),
                        glow: is_active,
                    }),
            )
            .child(
                // laid out across, then turned: reads bottom to top
                rect()
                    .position(
                        Position::new_absolute()
                            .left(33. / 2. - long / 2.)
                            .top(long / 2. - 33. / 2.),
                    )
                    .width(Size::px(long))
                    .height(Size::px(33.))
                    .rotation(-90.)
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .padding((0., 16.))
                    .spacing(6.)
                    .child(
                        label()
                            .text(name)
                            .font_size(TEXT_SM)
                            .font_weight(FontWeight::MEDIUM)
                            .letter_spacing(TEXT_SM * 0.02)
                            .color(if is_active {
                                c().text_primary
                            } else {
                                c().text_muted
                            })
                            .max_lines(1),
                    )
                    // room only: the real slot below stays upright, because
                    // hit-testing ignores rotation and would miss a turned ×
                    .child(rect().width(Size::px(16.)).height(Size::px(16.))),
            )
            .child(
                rect()
                    .position(Position::new_absolute().left((33. - 16.) / 2.).top(16.))
                    .child(end_slot(app, close_rel, hov, dirty, color)),
            )
    }
}

/// Notes-only mode: the tabs run across the top, each with its colour on
/// the top edge, and a rule in the live note's colour under them.
#[derive(PartialEq)]
pub struct TopTabs {}

impl Component for TopTabs {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let tabs = app.tabs.read().clone();
        let note = app.note.read().clone();
        let live = c().tone(app.index.read().tone(&note));
        let mut tree = app.tree;
        let tree_open = *tree.read();
        rect()
            .width(Size::fill())
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(36.))
                    .horizontal()
                    .background(c().bg_base)
                    .child(
                        rect()
                            .width(Size::px(36.))
                            .height(Size::fill())
                            .center()
                            .background(if tree_open {
                                c().accent_dim
                            } else {
                                Color::TRANSPARENT
                            })
                            .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                                right: 1.,
                                ..Default::default()
                            }))
                            .on_press(move |_| tree.set(!tree_open))
                            .child(icons::icon(
                                icons::FOLDER,
                                14.,
                                if tree_open {
                                    c().accent
                                } else {
                                    c().text_muted
                                },
                            )),
                    )
                    .children(tabs.into_iter().map(|rel| TopTab { rel }.into_element()))
                    .child(
                        rect()
                            .width(Size::px(34.))
                            .height(Size::fill())
                            .center()
                            .on_press(move |_| crate::new_note(app, None))
                            .child(label().text("+").font_size(TEXT_MD).color(c().text_faint)),
                    ),
            )
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::px(2.))
                    .background(live)
                    .opacity(0.9)
                    .shadow(Shadow::new().blur(10.).color(live)),
            )
    }
}

#[derive(PartialEq)]
struct TopTab {
    rel: String,
}

impl Component for TopTab {
    fn render_key(&self) -> DiffKey {
        (&self.rel).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let rel = self.rel.clone();
        let active = *app.note.read() == rel;
        let tone = app.index.read().tone(&rel);
        let color = c().tone(tone);
        let dirty = app.dirty.read().contains(&rel);
        let hov = *hover.read();
        let open = rel.clone();
        let close = rel.clone();
        rect()
            .height(Size::fill())
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .padding((0., 6., 0., 16.))
            .background(if active {
                c().surface_panel
            } else if hov {
                c().surface_raised
            } else {
                c().bg_base
            })
            // ⚠️ over/out, not enter/leave: those are exclusive, so the tab would
            // count as left the moment the cursor reaches the × — which then
            // unmounts, and the press lands on the tab instead of closing it
            .on_pointer_over(move |_| hover.set(true))
            .on_pointer_out(move |_| hover.set(false))
            .on_press(move |_| app.open(&open))
            // ⚠️ a border, not an absolute child: that starts inside the left
            // padding, and its fill is the room left in the row, so each
            // line began 16px late and ran on to the window's edge
            .border(
                Border::new()
                    .fill(color.with_af32(if active { 1. } else { 0.4 }))
                    .width(BorderWidth {
                        top: 2.,
                        ..Default::default()
                    }),
            )
            .child(
                label()
                    .text(base_name(&rel).to_string())
                    .font_size(TEXT_SM)
                    .font_weight(FontWeight::MEDIUM)
                    .color(if active {
                        c().text_primary
                    } else {
                        c().text_muted
                    }),
            )
            .child(end_slot(app, close, hov, dirty, color))
    }
}
