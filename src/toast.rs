//! The one toast: rises and fades in at the bottom right, counts its age,
//! fades out after a few seconds. An undoable action carries "undo" (⌘Z
//! keeps working after it's gone).
use std::time::Duration;

use freya::animation::*;
use freya::prelude::*;

use crate::{app::App, theme::*};

/// How long an undoable toast holds its undo (design 1f), in seconds.
const UNDO_SECS: u64 = 5;

#[derive(PartialEq)]
pub struct ToastHost {}

impl Component for ToastHost {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let toast = app.toast.read().clone();
        let mut shown = use_state(|| false);
        let mut since = use_state(|| 0u64);
        let seen = use_state(|| 0u32);
        let mut seen_w = seen;
        let anim = use_animation_transition(use_memo(move || *shown.read()), |from, to| {
            let v = |b: bool| if b { 1. } else { 0. };
            AnimNum::new(v(from), v(to))
                .time(280)
                .function(Function::Expo)
                .ease(Ease::Out)
        });
        // 1f: a 2px bar under an undo toast runs out with the undo
        let id = toast.as_ref().map(|t| t.id).unwrap_or(0);
        let left = use_animation_with_dependencies(&id, |conf, _| {
            conf.on_creation(OnCreation::Run).on_change(OnChange::Rerun);
            AnimNum::new(1., 0.)
                .time(UNDO_SECS * 1000)
                .function(Function::Linear)
        });
        if let Some(t) = &toast
            && *seen.read() != t.id
        {
            seen_w.set(t.id);
            shown.set(true);
            since.set(0);
            let id = t.id;
            let hold = if t.undo.is_some() { UNDO_SECS } else { 3 };
            spawn(async move {
                for s in 1..=hold {
                    freya::prelude::timer(Duration::from_secs(1)).await;
                    if *seen.read() != id {
                        return;
                    }
                    since.set(s);
                }
                shown.set(false);
            });
        }
        let t = anim.read().value();
        let Some(toast) = toast else {
            return rect().into_element();
        };
        let undo = toast.undo.clone();
        let detail = if toast.detail == "just now" && *since.read() > 0 {
            format!("{}s ago", *since.read())
        } else {
            toast.detail.clone()
        };
        let has_undo = undo.is_some();
        let (mark, mark_color) = if has_undo {
            ("⌫", c().text_faint)
        } else {
            ("✓", c().accent)
        };
        let left = left.read().value();
        rect()
            .position(
                Position::new_absolute()
                    .right(186.)
                    .bottom(38. - 10. * (1. - t)),
            )
            .layer(Layer::Overlay)
            .opacity(t)
            .background(c().surface_overlay)
            .border(Border::new().fill(c().border_subtle).width(1.))
            .corner_radius(6.)
            .overflow(Overflow::Clip)
            // the bar takes the row's width, not the window's
            .content(Content::fit())
            .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
            .font_size(TEXT_SM)
            .child(
                rect()
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(8.)
                    // 1f pads an undo toast 8/12, the saved one is 8/16
                    .padding((8., if has_undo { 12. } else { 16. }))
                    .child(label().text(mark).color(mark_color))
                    .child(label().text(toast.text.clone()))
                    .maybe_child((!detail.is_empty()).then(|| {
                        label()
                            .text(detail)
                            .font_size(TEXT_XS)
                            .color(c().text_faint)
                    }))
                    .maybe_child(undo.map(|u| UndoChip { undo: u })),
            )
            .maybe_child(has_undo.then(|| {
                rect()
                    .width(Size::fill_minimum())
                    .height(Size::px(2.))
                    .background(c().surface_inset)
                    .child(
                        // absolute, so it can't widen the toast it measures
                        rect()
                            .position(Position::new_absolute())
                            .width(Size::percent(100. * left))
                            .height(Size::fill())
                            .background(c().accent),
                    )
            }))
            .into_element()
    }
}

/// 1f's undo: "undo" and its shortcut, faint; the word and the border go
/// accent on hover.
#[derive(PartialEq)]
struct UndoChip {
    undo: crate::app::Undo,
}

impl Component for UndoChip {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let hov = *hover.read();
        let u = self.undo.clone();
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .padding((1., 6.))
            .corner_radius(3.)
            .border(
                Border::new()
                    .fill(if hov { c().accent } else { c().border_strong })
                    .width(1.),
            )
            .font_size(TEXT_XS)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |_| app.undo(&u))
            .child(
                label()
                    .text("undo")
                    .color(if hov { c().accent } else { c().text_primary }),
            )
            .child(label().text(crate::keys::show("⌘Z")).color(c().text_faint))
    }
}
