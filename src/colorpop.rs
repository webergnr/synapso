//! The colour tooltip that opens from any colour line (design 4): one row of
//! small swatches beside the line. The first, ↑, hands the colour back to the
//! folder (drawn as the folder's colour in outline); then the eight group
//! colours; the last is the rainbow, which swaps the swatches for a strip —
//! hue left to right, lightness top to bottom, chroma locked to the palette's
//! so a custom colour sits well next to the others.
//!
//! Hover previews (everything that inherits recolours live), click applies,
//! a click anywhere else cancels. Keys: 0–8 pick, 9 opens the strip, where
//! the arrows move, ⏎ applies and Esc goes back.
use freya::animation::*;
use freya::prelude::*;

use crate::{
    app::{App, ColorPick},
    index::{self, Tone},
    theme::*,
};

/// The strip's lightness range (OKLCH L), top to bottom, and its chroma.
const L_TOP: f32 = 0.86;
const L_BOTTOM: f32 = 0.56;
const CHROMA: f32 = 0.13;
const STRIP_W: f32 = 128.;
const STRIP_H: f32 = 14.;
const SWATCH: f32 = 14.;

#[derive(PartialEq)]
pub struct ColorPop {}

impl Component for ColorPop {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let pick = app.color_pick.read().clone();
        let open = pick.is_some();
        let anim = use_animation_transition(crate::use_tracked(open), |from, to| {
            let v = |b: bool| if b { 1. } else { 0. };
            AnimNum::new(v(from), v(to))
                .time(140)
                .function(Function::Quad)
                .ease(Ease::Out)
        });
        // what the pointer is over: a swatch (None = ↑), the rainbow, or nothing
        let mut hovered = use_state(|| None::<Hover>);
        let t = anim.read().value();
        let Some(pick) = pick else {
            return rect().into_element();
        };
        let key = pick.key.clone();
        let index = app.index.read();
        let own = index.own.get(&sidecar_key(&key)).copied();
        let parent = index::parent_folder(&key);
        let parent_tone = (!parent.is_empty()).then(|| index.tone(&parent));
        let parent_name = (!parent.is_empty()).then(|| crate::base_name(&parent).to_string());
        drop(index);

        let back_label = match &parent_name {
            Some(p) => format!("↑ {p}"),
            None => "none".to_string(),
        };
        let name_of = |tone: Option<Tone>| match tone {
            None => back_label.clone(),
            Some(t) => tone_name(t).to_string(),
        };
        let caption = match (&pick.pad, *hovered.read()) {
            (Some((h, l)), _) if pick.strip => {
                format!("h {} · {}", h.round() as i32, (l * 100.).round() as i32)
            }
            (None, _) if pick.strip => "any color".to_string(),
            (_, Some(Hover::Rainbow)) => "any color".to_string(),
            (_, Some(Hover::Swatch(tone))) => name_of(tone),
            _ => name_of(own),
        };

        let row = if pick.strip {
            strip(app, &pick)
        } else {
            let mut items: Vec<Element> =
                vec![swatch(app, &key, None, parent_tone, own.is_none(), hovered)];
            for i in 0..8 {
                items.push(swatch(
                    app,
                    &key,
                    Some(Tone::Slot(i)),
                    None,
                    own == Some(Tone::Slot(i)),
                    hovered,
                ));
            }
            items.push(rainbow(app, matches!(own, Some(Tone::Custom(_))), hovered));
            rect()
                .horizontal()
                .spacing(6.)
                .on_pointer_leave(move |_| {
                    hovered.set(None);
                    preview(app, None);
                })
                .children(items)
                .into_element()
        };

        let mut close = app.color_pick;
        let key_for_keys = pick.key.clone();
        rect()
            .position(Position::new_absolute().left(0.).top(0.))
            // ⚠️ the window, not `expanded()`: fill is what the parent column
            // has left after the panes and the status bar, which is 0px, and
            // a 0px scrim never hears the click outside that closes it
            .width(Size::window_percent(100.))
            .height(Size::window_percent(100.))
            .layer(Layer::Overlay)
            .on_global_key_down(move |e: Event<KeyboardEventData>| {
                let Some(p) = app.color_pick.read().clone() else {
                    return;
                };
                if p.strip {
                    let (h, l) = p.pad.unwrap_or((180., 0.74));
                    let step = if e.modifiers.contains(Modifiers::SHIFT) {
                        1.
                    } else {
                        10.
                    };
                    let pad = match e.key {
                        Key::Named(NamedKey::ArrowLeft) => Some(((h - step).rem_euclid(360.), l)),
                        Key::Named(NamedKey::ArrowRight) => Some(((h + step).rem_euclid(360.), l)),
                        Key::Named(NamedKey::ArrowUp) => Some((h, (l + 0.02).min(L_TOP))),
                        Key::Named(NamedKey::ArrowDown) => Some((h, (l - 0.02).max(L_BOTTOM))),
                        Key::Named(NamedKey::Enter) => {
                            apply_later(app, key_for_keys.clone(), Some(custom(h, l)));
                            return;
                        }
                        _ => None,
                    };
                    if let Some(pad) = pad {
                        set_pad(app, Some(pad));
                    }
                    return;
                }
                let slot = match e.code {
                    Code::Digit0 => Some(None),
                    Code::Digit1 => Some(Some(0)),
                    Code::Digit2 => Some(Some(1)),
                    Code::Digit3 => Some(Some(2)),
                    Code::Digit4 => Some(Some(3)),
                    Code::Digit5 => Some(Some(4)),
                    Code::Digit6 => Some(Some(5)),
                    Code::Digit7 => Some(Some(6)),
                    Code::Digit8 => Some(Some(7)),
                    Code::Digit9 => {
                        open_strip(app);
                        return;
                    }
                    _ => None,
                };
                if let Some(slot) = slot {
                    apply_later(app, key_for_keys.clone(), slot.map(Tone::Slot));
                }
            })
            .on_press(move |_| {
                index::set_preview(None);
                close.set(None)
            })
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(pick.x + 10.)
                            .top(pick.y - 12.),
                    )
                    .opacity(t)
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(8.)
                    .padding((6., 8.))
                    .background(c().surface_overlay)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(6.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .on_press(|e: Event<PressEventData>| e.stop_propagation())
                    .child(row)
                    .child(
                        rect().min_width(Size::px(64.)).child(
                            label()
                                .text(caption)
                                .font_size(TEXT_XS)
                                .color(c().text_muted),
                        ),
                    ),
            )
            .into_element()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Hover {
    Swatch(Option<Tone>),
    Rainbow,
}

/// The sidecar key: folders keep their `/`, documents their own key.
fn sidecar_key(key: &str) -> String {
    index::color_key(key)
}

/// Recolour everything that reads `key` as if it were `tone`, without saving.
fn preview(app: App, p: Option<(String, Option<Tone>)>) {
    index::set_preview(p);
    // repaint everything that reads a colour
    let mut i = app.index;
    let _ = i.write();
}

fn apply(app: App, key: &str, tone: Option<Tone>) {
    index::set_preview(None);
    let root = app.root();
    if index::set_tone(&root, key, tone).is_ok() {
        app.reindex();
    }
    let mut p = app.color_pick;
    p.set(None);
}

/// From a key: the editor hears the same key after us and skips it only
/// while the tooltip is open, so close it once this event is over.
fn apply_later(app: App, key: String, tone: Option<Tone>) {
    spawn(async move { apply(app, &key, tone) });
}

fn open_strip(app: App) {
    let mut p = app.color_pick;
    let Some(cur) = p.read().clone() else { return };
    // start where the note's own custom colour is, if it has one
    let own = app.index.read().own.get(&sidecar_key(&cur.key)).copied();
    let pad = match own {
        Some(Tone::Custom(v)) => Some(oklch_of(Color::new(v))),
        _ => None,
    };
    p.set(Some(ColorPick {
        strip: true,
        pad,
        ..cur
    }));
    if let Some((h, l)) = pad {
        preview(
            app,
            Some((
                sidecar_key(&p.read().as_ref().unwrap().key),
                Some(custom(h, l)),
            )),
        );
    }
}

/// Back from the strip to the swatches (Esc, ‹).
pub fn close_strip(app: App) -> bool {
    let mut p = app.color_pick;
    let Some(cur) = p.read().clone() else {
        return false;
    };
    if !cur.strip {
        return false;
    }
    p.set(Some(ColorPick {
        strip: false,
        pad: None,
        ..cur
    }));
    preview(app, None);
    true
}

fn set_pad(app: App, pad: Option<(f32, f32)>) {
    let mut p = app.color_pick;
    let Some(cur) = p.read().clone() else { return };
    let key = sidecar_key(&cur.key);
    p.set(Some(ColorPick { pad, ..cur }));
    preview(app, pad.map(|(h, l)| (key, Some(custom(h, l)))));
}

fn swatch(
    app: App,
    key: &str,
    tone: Option<Tone>,
    folder: Option<Tone>,
    selected: bool,
    mut hovered: State<Option<Hover>>,
) -> Element {
    let k = sidecar_key(key);
    let key = key.to_string();
    let fill = match tone {
        Some(t) => c().tone(t),
        None => Color::TRANSPARENT,
    };
    // ↑ is drawn as the folder's colour in outline, so you see what you'd get
    let edge = match (tone, folder) {
        (None, Some(f)) => Border::new().fill(c().tone(f)).width(1.),
        (None, None) => Border::new().fill(c().text_faint).width(1.),
        _ => Border::new(),
    };
    rect()
        .width(Size::px(SWATCH))
        .height(Size::px(SWATCH))
        .corner_radius(3.)
        .background(fill)
        .border(edge)
        .maybe(selected, |r| {
            r.border(
                Border::new()
                    .fill(c().text_primary)
                    .width(1.5)
                    .alignment(BorderAlignment::Outer),
            )
        })
        .cursor(CursorIcon::Pointer)
        .on_pointer_enter(move |_| {
            hovered.set(Some(Hover::Swatch(tone)));
            preview(app, Some((k.clone(), tone)));
        })
        .on_press(move |e: Event<PressEventData>| {
            e.stop_propagation();
            apply(app, &key, tone)
        })
        .into_element()
}

pub fn hue_stops() -> Vec<GradientStop> {
    (0..=8)
        .map(|i| GradientStop::new(oklch(i as f32 * 45., 0.74, CHROMA), i as f32 * 12.5))
        .collect()
}

fn rainbow(app: App, selected: bool, mut hovered: State<Option<Hover>>) -> Element {
    rect()
        .width(Size::px(SWATCH))
        .height(Size::px(SWATCH))
        .corner_radius(3.)
        .background(LinearGradient::new().angle(-90.).stops(hue_stops()))
        .maybe(selected, |r| {
            r.border(
                Border::new()
                    .fill(c().text_primary)
                    .width(1.5)
                    .alignment(BorderAlignment::Outer),
            )
        })
        .cursor(CursorIcon::Pointer)
        .on_pointer_enter(move |_| {
            hovered.set(Some(Hover::Rainbow));
            preview(app, None);
        })
        .on_press(move |e: Event<PressEventData>| {
            e.stop_propagation();
            open_strip(app);
        })
        .into_element()
}

/// Hue left to right, lightness top to bottom; hover previews, click applies.
fn strip(app: App, pick: &ColorPick) -> Element {
    let key = pick.key.clone();
    hue_strip(
        pick.pad,
        move |pad| set_pad(app, pad),
        move || {
            // copy it out: the read must end before apply writes the pick
            let pad = app.color_pick.read().as_ref().and_then(|p| p.pad);
            if let Some((h, l)) = pad {
                apply(app, &key, Some(custom(h, l)));
            }
        },
        move || {
            close_strip(app);
        },
    )
}

/// The rainbow strip, for whoever holds the colour: the marker sits at `pad`
/// (hue, lightness), `hover` hears the pointer (`None` when it leaves),
/// `pick` a click and `back` the ‹. The canvas's style bar uses it too.
pub fn hue_strip(
    pad: Option<(f32, f32)>,
    hover: impl Fn(Option<(f32, f32)>) + 'static,
    pick: impl Fn() + 'static,
    back: impl Fn() + 'static,
) -> Element {
    let hover = std::rc::Rc::new(hover);
    let leave = hover.clone();
    let marker = pad.map(|(h, l)| {
        let x = h / 360. * STRIP_W;
        let y = (L_TOP - l) / (L_TOP - L_BOTTOM) * STRIP_H;
        rect()
            .position(Position::new_absolute().left(x - 4.).top(y - 4.))
            .width(Size::px(8.))
            .height(Size::px(8.))
            .corner_radius(4.)
            .background(oklch(h, l, CHROMA))
            .border(Border::new().fill(Color::WHITE).width(1.5))
            .shadow(
                Shadow::new()
                    .blur(1.)
                    .spread(1.)
                    .color(Color::from_af32rgb(0.5, 0, 0, 0)),
            )
    });
    let at = |e: &Event<PointerEventData>| {
        let p = e.element_location();
        let fx = (p.x as f32 / STRIP_W).clamp(0., 1.);
        let fy = (p.y as f32 / STRIP_H).clamp(0., 1.);
        (
            (fx * 359.).round(),
            ((L_TOP - fy * (L_TOP - L_BOTTOM)) * 100.).round() / 100.,
        )
    };
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(6.)
        .child(
            label()
                .text("‹")
                .font_size(TEXT_SM)
                .color(c().text_faint)
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    back();
                }),
        )
        .child(
            rect()
                .width(Size::px(STRIP_W))
                .height(Size::px(STRIP_H))
                .corner_radius(3.)
                .cursor(CursorIcon::Crosshair)
                .background(LinearGradient::new().angle(-90.).stops(hue_stops()))
                .on_pointer_move(move |e: Event<PointerEventData>| hover(Some(at(&e))))
                .on_pointer_leave(move |_| leave(None))
                .on_press(move |e: Event<PressEventData>| {
                    e.stop_propagation();
                    pick();
                })
                .child(
                    // lighter at the top, darker at the bottom
                    rect().expanded().corner_radius(3.).background(
                        LinearGradient::new()
                            .stop((Color::from_af32rgb(0.28, 255, 255, 255), 0.))
                            .stop((Color::from_af32rgb(0., 255, 255, 255), 50.))
                            .stop((Color::from_af32rgb(0.32, 0, 0, 0), 100.)),
                    ),
                )
                .maybe_child(marker),
        )
        .into_element()
}

/// A point on the strip as the colour it stands for.
pub fn strip_color(h: f32, l: f32) -> Color {
    oklch(h, l, CHROMA)
}

fn custom(h: f32, l: f32) -> Tone {
    let c = strip_color(h, l);
    Tone::Custom(0xFF00_0000 | (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32)
}

/// OKLCH → sRGB, clamped into gamut.
pub fn oklch(h: f32, l: f32, chroma: f32) -> Color {
    let (a, b) = (chroma * h.to_radians().cos(), chroma * h.to_radians().sin());
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    let r = 4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_;
    let g = -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_;
    let bl = -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_;
    let enc = |x: f32| {
        let x = x.clamp(0., 1.);
        let v = if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1. / 2.4) - 0.055
        };
        (v * 255.).round() as u8
    };
    Color::from_rgb(enc(r), enc(g), enc(bl))
}

/// sRGB → (hue, lightness) in OKLCH, to put the strip's marker on a saved colour.
pub fn oklch_of(c: Color) -> (f32, f32) {
    let dec = |v: u8| {
        let x = v as f32 / 255.;
        if x <= 0.040_45 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (dec(c.r()), dec(c.g()), dec(c.b()));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let ll = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    (
        bb.atan2(a).to_degrees().rem_euclid(360.).round(),
        (ll.clamp(L_BOTTOM, L_TOP) * 100.).round() / 100.,
    )
}

pub fn tone_name(t: Tone) -> &'static str {
    match t {
        Tone::Slot(i) => SLOT_NAMES[i % 8],
        Tone::Neutral => "neutral",
        Tone::Custom(_) => "custom",
    }
}
