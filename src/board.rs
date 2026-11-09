//! The canvas pane: the core's `Scene` (every gesture, hit-test and verb of
//! the board) drawn with Skia in one pass, under the view's zoom and pan.
//! Freya elements only float on top: the tool rail, the chip, the minimap,
//! the context menu and the inline caption editor.
use std::{collections::HashMap, time::Instant};

use freya::animation::*;
use freya::prelude::*;
use freya_engine::prelude::{self as sk, Paint, PaintStyle, PathBuilder, PathEffect};
use synapso_core::{
    board::{self as geom, Event as SEvent, Outcome, Scene, Selection, Template, Tool, Under},
    vault::{self, Shape},
};

use crate::{
    app::App,
    field::{Field, Place},
    icons,
    index::{Index, Tone},
    tag,
    theme::*,
};

type KPoint = kurbo::Point;

#[derive(PartialEq)]
pub struct Board {
    pub rel: String,
    /// Bumped when the file was rewritten under the board: reload it.
    pub rev: u64,
}

/// A pane's "write now", saying why when the edit is still on screen only.
type Flush = Box<dyn Fn() -> Option<String>>;

thread_local! {
    static FLUSH: std::cell::RefCell<Option<Flush>> = const { std::cell::RefCell::new(None) };
    /// Nodes copied with ⌘C, for ⌘V on any board.
    static BOARD_CLIP: std::cell::RefCell<Vec<vault::Node>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Write the open board now, if it holds anything unsaved. Says why, when
/// what it holds is still on screen only (`refused`).
pub fn flush_board() -> Option<String> {
    FLUSH.with(|f| f.borrow().as_ref().and_then(|f| f()))
}

/// Why a save left the board's edit on screen only, if it did: a refused
/// write is kept only when the rescue copy holds what is on screen.
fn refused(scene: &Scene, result: Option<std::io::Result<()>>) -> Option<String> {
    let why = result?.err()?;
    let now = synapso_core::history::canvas_json(&scene.canvas);
    let rescued = scene
        .rescued
        .as_ref()
        .and_then(|p| vault::Canvas::load(p).ok());
    (rescued.map(|c| synapso_core::history::canvas_json(&c)) != Some(now)).then(|| why.to_string())
}

fn paint(color: Color) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(sk::SkColor::from(color));
    p
}

fn stroke(color: Color, w: f32) -> Paint {
    let mut p = paint(color);
    p.set_style(PaintStyle::Stroke).set_stroke_width(w);
    p
}

fn kcolor(c: peniko::Color) -> Color {
    let [r, g, b, a] = c.to_rgba8().to_u8_array();
    Color::from_argb(a, r, g, b)
}

/// A node's own colour: hex, or a JSON Canvas preset `"1"`–`"6"`.
fn node_color(node: &vault::Node) -> Option<Color> {
    node.color
        .as_deref()
        .and_then(synapso_core::color::canvas)
        .map(kcolor)
}

fn skr(r: kurbo::Rect) -> sk::SkRect {
    sk::SkRect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)
}

fn skp(p: KPoint) -> (f32, f32) {
    (p.x as f32, p.y as f32)
}

/// Text through Skia's paragraph, laid out at `width`, cached: a board
/// repaints every frame and the same captions come back every time.
#[allow(clippy::too_many_arguments)]
fn text(
    fc: &sk::FontCollection,
    body: &str,
    size: f32,
    color: Color,
    weight: i32,
    width: f32,
    align: sk::SkTextAlign,
    max_lines: Option<usize>,
) -> std::rc::Rc<sk::SkParagraph> {
    text_in(FONT, fc, body, size, color, weight, width, align, max_lines)
}

/// The face something written on the board is in: the hand (`HAND`), or
/// the app's (`FONT`) for an item set to mono (`fontFamily`).
fn face(mono: bool) -> &'static str {
    if mono { FONT } else { HAND }
}

/// [`text`] in the face of something written on the board rather than read
/// from a note (`face`). A glyph the hand lacks falls back to `FONT`.
#[allow(clippy::too_many_arguments)]
fn written(
    mono: bool,
    fc: &sk::FontCollection,
    body: &str,
    size: f32,
    color: Color,
    weight: i32,
    width: f32,
    align: sk::SkTextAlign,
    max_lines: Option<usize>,
) -> std::rc::Rc<sk::SkParagraph> {
    text_in(
        face(mono),
        fc,
        body,
        size,
        color,
        weight,
        width,
        align,
        max_lines,
    )
}

#[allow(clippy::too_many_arguments)]
fn text_in(
    family: &'static str,
    fc: &sk::FontCollection,
    body: &str,
    size: f32,
    color: Color,
    weight: i32,
    width: f32,
    align: sk::SkTextAlign,
    max_lines: Option<usize>,
) -> std::rc::Rc<sk::SkParagraph> {
    type Key = (&'static str, String, u32, u32, i32, u32, u8, usize, bool);
    thread_local! {
        static CACHE: std::cell::RefCell<HashMap<Key, std::rc::Rc<sk::SkParagraph>>> = std::cell::RefCell::new(HashMap::new());
    }
    let key: Key = (
        family,
        body.to_string(),
        size.to_bits(),
        u32::from(color),
        weight,
        width.max(1.).round().to_bits(),
        align as u8,
        max_lines.unwrap_or(0),
        c().dark,
    );
    if let Some(p) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return p;
    }
    let mut ts = sk::TextStyle::new();
    ts.set_font_families(&[family, FONT]);
    ts.set_font_size(size);
    ts.set_color(sk::SkColor::from(color));
    ts.set_font_style(sk::FontStyle::new(
        sk::Weight::from(weight),
        sk::Width::NORMAL,
        sk::Slant::Upright,
    ));
    let mut ps = sk::ParagraphStyle::new();
    ps.set_text_style(&ts);
    ps.set_text_align(align);
    if let Some(n) = max_lines {
        ps.set_max_lines(n);
        ps.set_ellipsis("…");
    }
    let mut b = sk::ParagraphBuilder::new(&ps, fc.clone());
    b.push_style(&ts);
    b.add_text(body);
    let mut p = b.build();
    p.layout(width.max(1.).round());
    let p = std::rc::Rc::new(p);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 4000 {
            c.clear();
        }
        c.insert(key, p.clone());
    });
    p
}

thread_local! {
    /// The font collection the board paints with — the app's, which holds
    /// the hand. Kept from the last paint, so the inline editor can measure.
    static FONTS: std::cell::RefCell<Option<sk::FontCollection>> = const { std::cell::RefCell::new(None) };
}

/// How wide the longest line and how tall the whole of `body` are, written
/// on the board (`written`), unwrapped, in world units. `None` before the
/// board's first paint.
fn measure(mono: bool, body: &str, size: f32, weight: i32) -> Option<(f32, f32)> {
    let fc = FONTS.with(|f| f.borrow().clone())?;
    // an empty line still has a line's height, and the caret needs it
    let body = if body.is_empty() { " " } else { body };
    let p = written(
        mono,
        &fc,
        body,
        size,
        Color::BLACK,
        weight,
        100_000.,
        sk::SkTextAlign::Left,
        None,
    );
    Some((p.longest_line(), p.height()))
}

/// Below this many pixels on screen a caption is noise: skip it.
const MIN_TEXT: f32 = 4.5;

fn align_of(a: vault::Align) -> sk::SkTextAlign {
    match a {
        vault::Align::Left => sk::SkTextAlign::Left,
        vault::Align::Center => sk::SkTextAlign::Center,
        vault::Align::Right => sk::SkTextAlign::Right,
    }
}

/// What one paint needs, copied out of the scene so the callback owns it.
struct Frame {
    canvas: vault::Canvas,
    view: geom::Viewport,
    selected: Vec<Selection>,
    editing: Option<Selection>,
    under: Under,
    preview: Option<(Shape, kurbo::Rect)>,
    marquee: Option<kurbo::Rect>,
    connecting: Option<(KPoint, KPoint)>,
    drawing: Option<Vec<KPoint>>,
    /// What the stroke being drawn wears: the pen's thickness and colour.
    pen: (f32, Option<Color>),
    edge_handles: Option<(KPoint, bool, KPoint, bool)>,
    /// The selected edge's bend handle, on screen.
    bend: Option<KPoint>,
    /// The alignments a Ctrl-move snapped to, world units.
    guides: Vec<(KPoint, KPoint)>,
    rotating: bool,
    /// File cards: resolved colour and excerpt, by node id — two cards on one
    /// note can quote two sections of it.
    cards: HashMap<String, (Tone, String)>,
    root: std::path::PathBuf,
}

fn frame_of(scene: &Scene, index: &Index) -> Frame {
    let mut cards = HashMap::new();
    for n in &scene.canvas.nodes {
        if let Some(f) = &n.file {
            let entry = index
                .doc(f)
                .or_else(|| index.by_stem(f.trim_end_matches(".md")));
            let tone = entry.map(|d| index.tone(&d.rel)).unwrap_or(Tone::Neutral);
            // a `#Heading` / `#^id` card quotes that section, else the opening
            let excerpt = entry
                .zip(n.section())
                .and_then(|(d, s)| section_card(&d.path, s))
                .or_else(|| entry.map(|d| d.excerpt.clone()))
                .unwrap_or_default();
            cards.insert(n.id.clone(), (tone, excerpt));
        }
    }
    Frame {
        canvas: scene.canvas.clone(),
        view: scene.view,
        selected: scene.selected.clone(),
        editing: scene.editing.clone(),
        under: scene.under.clone(),
        preview: scene.preview(),
        marquee: scene.marquee(),
        connecting: scene.connecting(),
        drawing: scene.drawing().map(|d| d.to_vec()),
        pen: (
            scene.pen,
            scene
                .pen_color
                .as_deref()
                .and_then(synapso_core::color::canvas)
                .map(kcolor),
        ),
        edge_handles: scene.edge_handles(),
        bend: scene.bend_handle(),
        guides: scene.guides().to_vec(),
        rotating: scene.is_rotating(),
        cards,
        root: index.root.clone(),
    }
}

fn shape_path(shape: Shape, r: kurbo::Rect) -> sk::SkPath {
    let mut b = PathBuilder::new();
    match shape {
        Shape::Ellipse => {
            b.add_oval(skr(r), None, None);
        }
        Shape::Triangle => {
            b.move_to((r.center().x as f32, r.y0 as f32));
            b.line_to((r.x1 as f32, r.y1 as f32));
            b.line_to((r.x0 as f32, r.y1 as f32));
            b.close();
        }
        _ => {
            b.add_rrect(sk::SkRRect::new_rect_xy(skr(r), 6., 6.), None, None);
        }
    }
    b.detach()
}

/// The line a shape's caption is centred on — the painter's and the editor's.
/// A triangle's sits low, where the shape is wide.
fn caption_y(shape: Shape, r: kurbo::Rect) -> f64 {
    if shape == Shape::Triangle {
        r.y0 + r.height() * 0.66
    } else {
        r.center().y
    }
}

/// The inline editor, on screen. Text with no box of its own around it is
/// typed straight onto the board (Weber, 2026-10-28): no well, no border,
/// in the face, size, colour and place it will be drawn in, so what is typed
/// is what stays. A card's file name, an edge's label and anything rotated
/// keep the boxed field.
struct Inline {
    /// On screen.
    at: kurbo::Rect,
    bare: bool,
    /// Bare text is laid out at its own size, as the painter lays it out,
    /// and the field scaled by the zoom: laid out at the zoomed size instead,
    /// the face's hinting spaced it differently, and the text jumped as the
    /// editor opened over it. A boxed field is at screen size: `1`.
    zoom: f32,
    /// The text size the field lays out at: world units when bare.
    size: f32,
    color: Color,
    weight: FontWeight,
    align: TextAlign,
    family: &'static str,
}

fn freya_align(a: sk::SkTextAlign) -> TextAlign {
    match a {
        sk::SkTextAlign::Center => TextAlign::Center,
        sk::SkTextAlign::Right => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

fn inline_editor(scene: &Scene, value: &str) -> Option<Inline> {
    let field = edit_field(scene)?;
    let editing = scene.editing.as_ref()?;
    let boxed = |family| Inline {
        at: field,
        bare: false,
        zoom: 1.,
        size: TEXT_SM,
        color: c().text_primary,
        weight: FontWeight::NORMAL,
        align: TextAlign::Left,
        family,
    };
    let Some(node) = editing.node().and_then(|id| scene.canvas.node(id)) else {
        // an edge's label: its face, in its little well
        let mono = match editing {
            Selection::Edge(id) => scene.canvas.edge(id).is_some_and(vault::Edge::mono),
            Selection::Node(_) => false,
        };
        return Some(boxed(face(mono)));
    };
    if node.radians() != 0. {
        return Some(boxed(face(node.mono())));
    }
    let r = scene.edit_rect()?;
    let z = scene.view.zoom as f32;
    let shape = node.shape();
    let align = |n: &vault::Node| freya_align(align_of(n.align().unwrap_or(shape.default_align())));
    Some(match shape {
        Shape::Label => Inline {
            at: r,
            bare: true,
            zoom: z,
            size: node.text_size(TEXT_SM),
            color: node_color(node).unwrap_or(c().text_primary),
            weight: if node.bold() {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            align: align(node),
            family: face(node.mono()),
        },
        Shape::Rect | Shape::Ellipse | Shape::Triangle => {
            // as the painter lays a caption out: 10 in from each side,
            // centred on the caption's line
            let size = node.text_size(TEXT_SM);
            let weight = if node.bold() { 700 } else { 500 };
            let lines = value.lines().count().max(1) as f32;
            let tall =
                measure(node.mono(), value, size, weight).map_or(size * 1.3 * lines, |m| m.1) * z;
            let pad = 10. * f64::from(z);
            let cy = caption_y(shape, r);
            Inline {
                at: kurbo::Rect::new(
                    r.x0 + pad,
                    cy - f64::from(tall) / 2.,
                    (r.x1 - pad).max(r.x0 + pad + 30. * f64::from(z)),
                    cy + f64::from(tall) / 2.,
                ),
                bare: true,
                zoom: z,
                size,
                color: c().text_primary,
                weight: if node.bold() {
                    FontWeight::BOLD
                } else {
                    FontWeight::MEDIUM
                },
                align: align(node),
                family: face(node.mono()),
            }
        }
        // a frame's name, above its top-left corner
        Shape::Frame => Inline {
            at: kurbo::Rect::from_origin_size(
                (r.x0 + 2. * f64::from(z), r.y0 - 17. * f64::from(z)),
                (r.width().max(80.), f64::from(TEXT_SM * z * 1.4)),
            ),
            bare: true,
            zoom: z,
            size: TEXT_SM,
            color: c().text_muted,
            weight: if node.bold() {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            },
            align: TextAlign::Left,
            family: face(node.mono()),
        },
        // a card's name is a file name: the app's face, in a well
        _ => boxed(FONT),
    })
}

/// Where the inline editor's field sits, board-local: across the box on the
/// caption's line, never narrower than a word.
///
/// ⚠️ `Scene::edit_rect` is already on screen. Run through the viewport a
/// second time, the field drifted off the shape by the pan and the zoom.
fn edit_field(scene: &Scene) -> Option<kurbo::Rect> {
    let r = scene.edit_rect()?;
    let node = scene
        .editing
        .as_ref()?
        .node()
        .and_then(|id| scene.canvas.node(id));
    let y = node.map_or(r.center().y, |n| caption_y(n.shape(), r));
    Some(kurbo::Rect::new(
        r.x0,
        (y - 10.).max(0.),
        r.x0 + r.width().max(80.),
        y + 10.,
    ))
}

fn draw_frame(cv: &sk::Canvas, fc: &sk::FontCollection, f: &Frame, w: f32, h: f32) {
    let p = c();
    cv.draw_rect(sk::SkRect::from_wh(w, h), &paint(p.canvas_bg));
    // the dot grid, never denser than at 100%
    let z = f.view.zoom as f32;
    let mut step = 22. * z;
    while step < 11. {
        step *= 2.;
    }
    let (ox, oy) = (
        (f.view.offset.x as f32).rem_euclid(step),
        (f.view.offset.y as f32).rem_euclid(step),
    );
    let dot = paint(p.canvas_dot);
    let mut y = oy;
    while y < h {
        let mut x = ox;
        while x < w {
            cv.draw_circle((x, y), 1., &dot);
            x += step;
        }
        y += step;
    }

    cv.save();
    cv.translate((f.view.offset.x as f32, f.view.offset.y as f32));
    cv.scale((z, z));

    let by_id: HashMap<&str, &vault::Node> =
        f.canvas.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let selected_edge = |id: &str| {
        f.selected
            .iter()
            .any(|s| matches!(s, Selection::Edge(e) if e == id))
    };
    let selected_node = |id: &str| {
        f.selected
            .iter()
            .any(|s| matches!(s, Selection::Node(n) if n == id))
    };

    let legible = |size: f32| size * z >= MIN_TEXT;

    // edges under the nodes, each name on its own edge's layer: a card over
    // the line covers the word too, the way it covers the line
    for e in &f.canvas.edges {
        let (Some(a), Some(b)) = (
            by_id.get(e.from_node.as_str()),
            by_id.get(e.to_node.as_str()),
        ) else {
            continue;
        };
        let route = e.route();
        let line = geom::route_edge(e, a, b);
        let (pa, pb) = (line.points[0], line.points[line.points.len() - 1]);
        let color = if selected_edge(&e.id) {
            p.accent
        } else {
            e.color
                .as_deref()
                .and_then(synapso_core::color::canvas)
                .map(kcolor)
                .unwrap_or(p.connector)
        };
        let width = e.width().max(1.);
        let mut s = stroke(color, width);
        if e.dashed() {
            s.set_path_effect(PathEffect::dash(&[4. * width, 4. * width], 0.));
        }
        let mut pb_ = PathBuilder::new();
        match line.curve {
            Some((c1, c2)) => {
                pb_.move_to(skp(pa));
                pb_.cubic_to(skp(c1), skp(c2), skp(pb));
            }
            None => {
                if let Some(first) = line.points.first() {
                    pb_.move_to(skp(*first));
                    for q in &line.points[1..] {
                        pb_.line_to(skp(*q));
                    }
                }
            }
        }
        cv.draw_path(&pb_.detach(), &s);
        let head = |to: KPoint, from: KPoint| {
            let along = to - from;
            let len = along.hypot();
            if len < 1e-3 {
                return;
            }
            let u = along / len;
            let side = kurbo::Vec2::new(-u.y, u.x);
            let (back, half) = (f64::from(width) * 5., f64::from(width) * 2.5);
            let base = to - u * back;
            let mut hb = PathBuilder::new();
            hb.move_to(skp(to));
            hb.line_to(skp(base + side * half));
            hb.line_to(skp(base - side * half));
            hb.close();
            cv.draw_path(&hb.detach(), &paint(color));
        };
        let n = line.points.len();
        if n >= 2 {
            if e.has_arrow() {
                head(line.points[n - 1], line.points[n - 2]);
            }
            if e.back_arrow() {
                head(line.points[0], line.points[1]);
            }
        }
        let editing = matches!(&f.editing, Some(Selection::Edge(id)) if id == &e.id);
        if let Some(l) = e.label.as_deref().filter(|l| !l.is_empty() && !editing)
            && legible(TEXT_XS)
        {
            let at = geom::label_anchor(&line.points, route);
            // the label wears its arrow's colour, as it's drawn right now
            let t = written(
                e.mono(),
                fc,
                l,
                TEXT_XS,
                color,
                400,
                400.,
                sk::SkTextAlign::Left,
                Some(1),
            );
            let w = t.longest_line() + 8.;
            let rect = sk::SkRect::from_xywh(at.x as f32 - w / 2., at.y as f32 - 8., w, 16.);
            cv.draw_rrect(sk::SkRRect::new_rect_xy(rect, 3., 3.), &paint(p.canvas_bg));
            t.paint(cv, (rect.left + 4., rect.top + 2.));
        }
    }

    // what's on screen, in world units
    let screen = kurbo::Rect::new(0., 0., f64::from(w), f64::from(h));
    let world_screen = kurbo::Rect::from_points(
        f.view.to_world(screen.origin()),
        f.view.to_world(KPoint::new(screen.x1, screen.y1)),
    );

    // nodes, in stacking order
    for node in &f.canvas.nodes {
        let r = node.rect();
        if !r.inflate(40., 40.).overlaps(world_screen) {
            continue;
        }
        let angle = node.radians();
        let turned = angle != 0.;
        if turned {
            cv.save();
            let cx = r.center();
            cv.rotate(
                angle.to_degrees(),
                Some(sk::SkPoint::new(cx.x as f32, cx.y as f32)),
            );
        }
        let editing = matches!(&f.editing, Some(Selection::Node(id)) if id == &node.id);
        let caption = if editing { "" } else { node.caption() };
        let shape = node.shape();
        let own = node_color(node);
        match shape {
            // a loose arrow end has no body: the arrow is what shows, and the
            // chrome below rings it when it's hovered or selected
            Shape::Point => {}
            Shape::Draw => {
                if let Some(points) = node.draw.as_deref().filter(|p| p.len() >= 2) {
                    let at = |q: &[f32; 2]| {
                        (
                            (r.x0 + f64::from(q[0]) * r.width()) as f32,
                            (r.y0 + f64::from(q[1]) * r.height()) as f32,
                        )
                    };
                    let mut b = PathBuilder::new();
                    b.move_to(at(&points[0]));
                    for q in &points[1..] {
                        b.line_to(at(q));
                    }
                    let mut s = stroke(own.unwrap_or(p.text_primary), node.stroke());
                    s.set_stroke_cap(sk::PaintCap::Round)
                        .set_stroke_join(sk::PaintJoin::Round);
                    cv.draw_path(&b.detach(), &s);
                }
            }
            Shape::File => {
                let file = node.file.clone().unwrap_or_default();
                if vault::is_image(&file) {
                    match picture(&f.root.join(&file)) {
                        Some(img) => {
                            let rr = sk::SkRRect::new_rect_xy(skr(r), 6., 6.);
                            cv.save();
                            cv.clip_rrect(rr, None, true);
                            cv.draw_image_rect(&img, None, skr(r), &paint(Color::WHITE));
                            cv.restore();
                            cv.draw_rrect(rr, &stroke(p.border_subtle, 1.));
                        }
                        None => image_placeholder(cv, fc, r, &file),
                    }
                } else {
                    let (tone, preview) = f
                        .cards
                        .get(&node.id)
                        .cloned()
                        .unwrap_or((Tone::Neutral, String::new()));
                    // a quote card shows its quote, not the note's opening
                    let excerpt = node
                        .excerpt
                        .clone()
                        .filter(|e| !e.trim().is_empty())
                        .map(|e| format!("“{}”", e.trim()))
                        .unwrap_or(preview);
                    doc_card(
                        cv,
                        fc,
                        r,
                        node,
                        &file,
                        tone,
                        &excerpt,
                        selected_node(&node.id),
                        legible(TEXT_XS),
                    );
                }
            }
            Shape::Frame => {
                let rr = sk::SkRRect::new_rect_xy(skr(r), 6., 6.);
                cv.draw_rrect(rr, &paint(p.surface_panel.with_a(60)));
                let mut s = stroke(own.unwrap_or(p.border_strong), 1.);
                s.set_path_effect(PathEffect::dash(&[5., 4.], 0.));
                cv.draw_rrect(rr, &s);
                if legible(TEXT_SM) {
                    let t = written(
                        node.mono(),
                        fc,
                        caption,
                        TEXT_SM,
                        p.text_muted,
                        if node.bold() { 700 } else { 400 },
                        r.width() as f32,
                        sk::SkTextAlign::Left,
                        Some(1),
                    );
                    t.paint(cv, (r.x0 as f32 + 2., r.y0 as f32 - 17.));
                }
            }
            Shape::Label if !legible(node.text_size(TEXT_SM)) => {}
            Shape::Label => {
                let size = node.text_size(TEXT_SM);
                let t = written(
                    node.mono(),
                    fc,
                    caption,
                    size,
                    own.unwrap_or(p.text_primary),
                    if node.bold() { 700 } else { 400 },
                    r.width().max(20.) as f32,
                    align_of(node.align().unwrap_or(shape.default_align())),
                    None,
                );
                cv.save();
                cv.clip_rect(skr(r), None, true);
                t.paint(cv, (r.x0 as f32, r.y0 as f32));
                cv.restore();
            }
            Shape::Rect | Shape::Ellipse | Shape::Triangle => {
                let path = shape_path(shape, r);
                // a soft drop shadow, like the design's --shadow-1
                let mut sh = paint(p.shadow(0.9));
                sh.set_mask_filter(sk::SkMaskFilter::blur(sk::SkBlurStyle::Normal, 1.5, None));
                cv.save();
                cv.translate((0., 1.));
                cv.draw_path(&path, &sh);
                cv.restore();
                cv.draw_path(&path, &paint(p.surface_raised));
                if let Some(col) = own {
                    cv.draw_path(&path, &paint(col.with_a(36)));
                }
                cv.draw_path(
                    &path,
                    &stroke(own.unwrap_or(p.border_strong), node.stroke().max(1.)),
                );
                let pad = 10.;
                let size = node.text_size(TEXT_SM);
                let width = (r.width() - 2. * pad).max(30.) as f32;
                if !legible(size) {
                    if turned {
                        cv.restore();
                    }
                    continue;
                }
                let t = written(
                    node.mono(),
                    fc,
                    caption,
                    size,
                    p.text_primary,
                    if node.bold() { 700 } else { 500 },
                    width,
                    align_of(node.align().unwrap_or(shape.default_align())),
                    None,
                );
                let cy = caption_y(shape, r);
                t.paint(cv, ((r.x0 + pad) as f32, cy as f32 - t.height() / 2.));
            }
        }
        if turned {
            cv.restore();
        }
    }

    // in-progress gestures
    if let Some((shape, r)) = f.preview {
        let path = shape_path(shape, r);
        cv.draw_path(&path, &paint(p.accent_dim));
        cv.draw_path(&path, &stroke(p.accent, 1.));
    }
    if let Some((a, b)) = f.connecting {
        let mut s = stroke(p.accent, 1.5);
        s.set_path_effect(PathEffect::dash(&[4., 4.], 0.));
        cv.draw_line(skp(a), skp(b), &s);
    }
    if let Some(points) = &f.drawing
        && points.len() >= 2
    {
        let mut b = PathBuilder::new();
        b.move_to(skp(points[0]));
        for q in &points[1..] {
            b.line_to(skp(*q));
        }
        let mut s = stroke(f.pen.1.unwrap_or(p.text_primary), f.pen.0);
        s.set_stroke_cap(sk::PaintCap::Round)
            .set_stroke_join(sk::PaintJoin::Round);
        cv.draw_path(&b.detach(), &s);
    }
    if let Some(r) = f.marquee {
        cv.draw_rect(skr(r), &paint(p.accent_dim));
        cv.draw_rect(skr(r), &stroke(p.accent, 1. / z));
    }
    // the alignments a Ctrl-move snapped to: hairlines in red, as tldraw
    // draws them, through every node that shares the line
    for (a, b) in &f.guides {
        cv.draw_line(skp(*a), skp(*b), &stroke(p.danger, 1. / z));
    }
    cv.restore();

    // selection chrome: screen-sized handles
    let view = f.view;
    for sel in &f.selected {
        let Selection::Node(id) = sel else { continue };
        let Some(node) = by_id.get(id.as_str()) else {
            continue;
        };
        let r = view.screen_rect(node.rect());
        // a loose end: a ring where it is, nothing to resize or turn
        if node.shape() == Shape::Point {
            cv.draw_circle(skp(r.center()), 5., &paint(p.surface_panel));
            cv.draw_circle(skp(r.center()), 5., &stroke(p.accent, 1.5));
            continue;
        }
        let angle = node.radians();
        cv.save();
        if angle != 0. {
            let cx = r.center();
            cv.rotate(
                angle.to_degrees(),
                Some(sk::SkPoint::new(cx.x as f32, cx.y as f32)),
            );
        }
        cv.draw_rect(skr(r.inflate(1., 1.)), &stroke(p.accent, 1.));
        if f.selected.len() == 1 {
            for (_, q) in geom::handle_points(r) {
                let hr = sk::SkRect::from_xywh(q.x as f32 - 3.5, q.y as f32 - 3.5, 7., 7.);
                cv.draw_rect(hr, &paint(p.surface_panel));
                cv.draw_rect(hr, &stroke(p.accent, 1.));
            }
            // the rotation grip
            let top = KPoint::new(r.center().x, r.y0);
            let grip = KPoint::new(top.x, top.y - geom::GRIP_GAP);
            let mut s = stroke(p.accent, 1.);
            s.set_alpha_f(0.6);
            cv.draw_line(skp(top), skp(grip), &s);
            cv.draw_circle(skp(grip), 4., &paint(p.surface_panel));
            cv.draw_circle(skp(grip), 4., &stroke(p.accent, 1.));
            if f.rotating {
                let deg = node.angle();
                let t = text(
                    fc,
                    &format!("{deg:.0}° · ⇧ snaps 15°"),
                    TEXT_XS,
                    p.text_primary,
                    400,
                    300.,
                    sk::SkTextAlign::Left,
                    Some(1),
                );
                let w = t.longest_line() + 12.;
                let br = sk::SkRect::from_xywh(r.x1 as f32 + 8., r.y0 as f32 - 22., w, 18.);
                cv.draw_rrect(
                    sk::SkRRect::new_rect_xy(br, 4., 4.),
                    &paint(p.surface_overlay),
                );
                t.paint(cv, (br.left + 6., br.top + 3.));
            }
        }
        cv.restore();
    }
    // hover outline
    if let Under::Node(id) = &f.under
        && !selected_node(id)
        && let Some(node) = by_id.get(id.as_str())
    {
        let r = view.screen_rect(node.rect());
        let mut s = stroke(p.text_faint, 1.);
        s.set_alpha_f(0.8);
        if node.shape() == Shape::Point {
            cv.draw_circle(skp(r.center()), 5., &s);
        } else {
            cv.draw_rrect(sk::SkRRect::new_rect_xy(skr(r.inflate(2., 2.)), 7., 7.), &s);
        }
    }
    if let Some((a, pa, b, pb)) = f.edge_handles {
        for (q, pinned) in [(a, pa), (b, pb)] {
            cv.draw_circle(
                skp(q),
                4.5,
                &paint(if pinned { p.accent } else { p.surface_panel }),
            );
            cv.draw_circle(skp(q), 4.5, &stroke(p.accent, 1.));
        }
    }
    // the selected edge's middle: a smaller diamond, dragged to bend it
    if let Some(m) = f.bend {
        let mut d = PathBuilder::new();
        let (x, y) = skp(m);
        d.move_to((x, y - 4.5));
        d.line_to((x + 4.5, y));
        d.line_to((x, y + 4.5));
        d.line_to((x - 4.5, y));
        d.close();
        let d = d.detach();
        cv.draw_path(&d, &paint(p.surface_panel));
        cv.draw_path(&d, &stroke(p.accent, 1.));
    }
}

fn image_placeholder(cv: &sk::Canvas, fc: &sk::FontCollection, r: kurbo::Rect, file: &str) {
    let p = c();
    let rr = sk::SkRRect::new_rect_xy(skr(r), 6., 6.);
    cv.save();
    cv.clip_rrect(rr, None, true);
    cv.draw_rect(skr(r), &paint(p.canvas_bg));
    let s = stroke(p.surface_raised, 8. * std::f32::consts::SQRT_2);
    let (w, h) = (r.width() as f32, r.height() as f32);
    let mut k = -h;
    while k < w + h {
        cv.draw_line(
            (r.x0 as f32 + k, r.y1 as f32),
            (r.x0 as f32 + k + h, r.y0 as f32),
            &s,
        );
        k += 16. * std::f32::consts::SQRT_2;
    }
    cv.restore();
    cv.draw_rrect(rr, &stroke(p.border_subtle, 1.));
    let name = crate::base_name(file);
    let t = text(
        fc,
        name,
        TEXT_XS,
        p.text_muted,
        400,
        300.,
        sk::SkTextAlign::Left,
        Some(1),
    );
    let tw = t.longest_line() + 12.;
    let br = sk::SkRect::from_xywh(
        r.center().x as f32 - tw / 2.,
        r.center().y as f32 - 9.,
        tw,
        18.,
    );
    cv.draw_rrect(
        sk::SkRRect::new_rect_xy(br, 3., 3.),
        &paint(p.surface_overlay),
    );
    cv.draw_rrect(
        sk::SkRRect::new_rect_xy(br, 3., 3.),
        &stroke(p.border_subtle, 1.),
    );
    t.paint(cv, (br.left + 6., br.top + 3.));
}

#[allow(clippy::too_many_arguments)]
fn doc_card(
    cv: &sk::Canvas,
    fc: &sk::FontCollection,
    r: kurbo::Rect,
    node: &vault::Node,
    file: &str,
    tone: Tone,
    excerpt: &str,
    selected: bool,
    legible: bool,
) {
    let p = c();
    let color = p.tone(tone);
    let rr = sk::SkRRect::new_rect_xy(skr(r), 6., 6.);
    let mut sh = paint(p.shadow(if selected { 1.2 } else { 0.8 }));
    sh.set_mask_filter(sk::SkMaskFilter::blur(
        sk::SkBlurStyle::Normal,
        if selected { 6. } else { 2. },
        None,
    ));
    cv.save();
    cv.translate((0., if selected { 3. } else { 1. }));
    cv.draw_rrect(rr, &sh);
    cv.restore();
    cv.draw_rrect(rr, &paint(p.surface_raised));
    cv.draw_rrect(rr, &stroke(p.border_subtle, 1.));
    // the spine, glowing in the note's resolved colour
    cv.save();
    cv.clip_rrect(rr, None, true);
    let mut glow = paint(color);
    glow.set_mask_filter(sk::SkMaskFilter::blur(sk::SkBlurStyle::Normal, 4., None));
    cv.draw_rect(
        sk::SkRect::from_xywh(r.x0 as f32, r.y0 as f32, 2., r.height() as f32),
        &glow,
    );
    cv.draw_rect(
        sk::SkRect::from_xywh(r.x0 as f32, r.y0 as f32, 2., r.height() as f32),
        &paint(color),
    );
    cv.restore();
    if !legible {
        return;
    }
    let (x, w) = (r.x0 as f32 + 14., r.width() as f32 - 26.);
    let name = crate::base_name(file);
    let section = node.section().map(|s| format!(" #{s}")).unwrap_or_default();
    let title = text(
        fc,
        &format!("{name}{section}"),
        TEXT_SM,
        p.text_primary,
        500,
        w - 34.,
        sk::SkTextAlign::Left,
        Some(1),
    );
    title.paint(cv, (x, r.y0 as f32 + 8.));
    // the `doc` tag
    let kind = if file.ends_with(".canvas") {
        "board"
    } else {
        "doc"
    };
    let tg = text(
        fc,
        kind,
        TEXT_XS,
        p.text_faint,
        400,
        60.,
        sk::SkTextAlign::Left,
        Some(1),
    );
    let tw = tg.longest_line() + 8.;
    let tr = sk::SkRect::from_xywh(r.x1 as f32 - 12. - tw, r.y0 as f32 + 8., tw, 14.);
    cv.draw_rrect(
        sk::SkRRect::new_rect_xy(tr, 3., 3.),
        &stroke(p.border_subtle, 1.),
    );
    tg.paint(cv, (tr.left + 4., tr.top + 1.));
    let folder = crate::index::parent_folder(file);
    let mut y = r.y0 as f32 + 26.;
    if !folder.is_empty() {
        let f = text(
            fc,
            &folder,
            TEXT_XS,
            p.text_faint,
            400,
            w,
            sk::SkTextAlign::Left,
            Some(1),
        );
        f.paint(cv, (x, y));
        y += 16.;
    }
    let lines = (((r.y1 as f32 - 10.) - y) / 15.5).floor().max(1.) as usize;
    let body = text(
        fc,
        excerpt,
        TEXT_XS,
        p.text_muted,
        400,
        w,
        sk::SkTextAlign::Left,
        Some(lines),
    );
    cv.save();
    cv.clip_rect(
        sk::SkRect::from_xywh(x, y, w, r.y1 as f32 - 8. - y),
        None,
        true,
    );
    body.paint(cv, (x, y));
    cv.restore();
}

/// Tool rail entries: the design's nine, keyed as the app keys them.
/// A tool: what it is, its name, its key and its icon.
type ToolEntry = (
    Tool,
    &'static str,
    &'static str,
    (&'static str, &'static [u8]),
);

const TOOLS: [ToolEntry; 9] = [
    (Tool::Select, "Select", "V", icons::T_SELECT),
    (Tool::Rect, "Rectangle", "R", icons::T_RECT),
    (Tool::Ellipse, "Ellipse", "O", icons::T_ELLIPSE),
    (Tool::Triangle, "Triangle", "Y", icons::T_TRIANGLE),
    (Tool::Arrow, "Arrow", "A", icons::T_ARROW),
    (Tool::Line, "Line", "L", icons::T_LINE),
    (Tool::Pen, "Pen", "P", icons::T_PEN),
    (Tool::Text, "Text", "T", icons::T_TEXT),
    (Tool::Frame, "Frame", "F", icons::T_FRAME),
];

#[derive(Clone, PartialEq)]
struct Menu {
    at: (f32, f32),
    world: KPoint,
    target: Option<Selection>,
}

impl Component for Board {
    fn render_key(&self) -> DiffKey {
        (&(&self.rel, self.rev)).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let rel = self.rel.clone();
        let path = app.root().join(&rel);
        let mut scene = use_state({
            let path = path.clone();
            move || Scene::load(path)
        });
        let mut tool = use_state(|| Tool::Select);
        let mut pressed = use_state(|| false);
        let mut last_click = use_state(|| {
            (
                Instant::now() - std::time::Duration::from_secs(5),
                KPoint::ZERO,
            )
        });
        let mut origin = use_state(|| (0f32, 0f32));
        let mut fitted = use_state(|| false);
        // the card a mention brought into view, kept centred while the pane
        // still grows (the split glides open) and until a hand moves the view
        let mut centred = use_state(|| None::<String>);
        let mut space = use_state(|| false);
        let mut menu = use_state(|| None::<Menu>);
        let edit_value = use_state(String::new);
        // where the press that opened the editor landed, board-local: the
        // caret starts under it; `None` (⏎, a new shape) starts at the end
        let caret_at = use_state(|| None::<KPoint>);
        // the pan's last pointer positions, and which glide may run: a new
        // press or a new glide bumps it, and the running one stops
        let mut pan_trail = use_state(Vec::<(Instant, KPoint)>::new);
        let mut glides = use_state(|| 0u64);
        // what is typed is the scene's as it is typed: a press anywhere else
        // on the board commits the edit (`Scene::press`), and that commit
        // reads the scene's text, so it must already hold every keystroke.
        // And text typed onto the board takes room as it goes: the label is
        // as big as its words (`Scene::fit_label`), so nothing runs out of it
        use_side_effect(move || {
            let typed = edit_value.read().clone();
            if scene.peek().editing.is_none() {
                return;
            }
            let label = {
                let s = scene.peek();
                s.editing
                    .as_ref()
                    .and_then(Selection::node)
                    .and_then(|id| s.canvas.node(id))
                    .filter(|n| n.shape() == Shape::Label)
                    .map(|n| {
                        let weight = if n.bold() { 700 } else { 400 };
                        (n.id.clone(), n.text_size(TEXT_SM), weight, n.mono())
                    })
            };
            let mut scene = scene;
            let mut s = scene.write();
            s.set_edit_text(&typed);
            if let Some((id, size, weight, mono)) = label
                && let Some((w, h)) = measure(mono, &typed, size, weight)
            {
                // a hair over the line, for the caret at its end
                s.fit_label(&id, kurbo::Size::new(f64::from(w) + 4., f64::from(h)));
            }
        });
        let saves = use_state(|| 0u64);
        let a11y = use_a11y();
        // undo: the board as JSON before each change (the app does the same)
        let mut last = use_state({
            let path = path.clone();
            move || {
                synapso_core::vault::Canvas::load(&path)
                    .map(|c| synapso_core::history::canvas_json(&c))
                    .unwrap_or_default()
            }
        });
        let undo = use_state(Vec::<String>::new);
        let redo = use_state(Vec::<String>::new);
        let mut switcher = use_state(|| false);
        // the chevron and the open switcher as laid out: a press on neither shuts it
        let mut chevron_area = use_state(Area::default);
        let mut switcher_area = use_state(Area::default);
        let tint = use_state(|| None::<Option<String>>);
        // the rel as a Copy handle, for the save closures
        let here = use_state(|| rel.clone());

        if rel.is_empty() {
            return empty_board(app).into_element();
        }
        FLUSH.with(|f| {
            *f.borrow_mut() = Some(Box::new(move || {
                let result = save_board(app, scene, &here.read());
                refused(&scene.read(), result)
            }))
        });
        use_drop(|| FLUSH.with(|f| *f.borrow_mut() = None));

        // a `[[canvas: board#…]]` mention asked for one of these cards: taken
        // once the pane has a size, which is what `reveal` centres in
        let asked = app.reveal.read().clone();
        if let Some((_, card)) = asked.filter(|(board, _)| *board == rel)
            && *fitted.read()
        {
            let mut r = app.reveal;
            r.set(None);
            let found = card_id(&scene.read().canvas, &card);
            match found {
                Some(id) => {
                    scene.write().reveal(&id);
                    centred.set(Some(id));
                }
                None if scene.read().broken.is_none() => app.toast(
                    "that card is gone",
                    format!("{card} is not on {} any more", crate::base_name(&rel)),
                    None,
                ),
                None => {}
            }
        }

        // a change happened: the state before it goes on the undo stack
        let checkpoint = move || {
            let mut last = last;
            let mut undo = undo;
            let mut redo = redo;
            let now = synapso_core::history::canvas_json(&scene.read().canvas);
            if now != *last.read() {
                let before = std::mem::replace(&mut *last.write(), now);
                undo.write().push(before);
                if undo.read().len() > 100 {
                    undo.write().remove(0);
                }
                redo.write().clear();
            }
        };
        // write after 1.5 s of quiet; a refused save is the core's to rescue
        // ⚠️ the board's, not the caller's: a caption's ⏎ calls this from the
        // inline field, which unmounts as the edit ends — a timer spawned
        // there died with it, and what was typed sat unsaved until the next
        // change or the window's close
        let board_scope = current_scope_id();
        let schedule_save = move || {
            checkpoint();
            // ⚠️ from the scene, not `true`: opening the editor and most of the
            // menu (fit, copy, open) come through here and change nothing
            let mut s = scene;
            let unsaved = s.write().dirty();
            mark_unsaved(app, &here.read(), unsaved);
            let mut saves = saves;
            *saves.write() += 1;
            let generation = *saves.read();
            spawn_in_scope(
                async move {
                    freya::prelude::timer(std::time::Duration::from_millis(1500)).await;
                    if *saves.read() == generation
                        && let Some(Err(e)) = save_board(app, scene, &here.read())
                    {
                        app.toast("canvas not saved", e.to_string(), None);
                    }
                },
                board_scope,
            );
        };

        let handle = move |ev: SEvent| {
            let mut scene = scene;
            let mut tool = tool;
            let mut menu = menu;
            let mut v = edit_value;
            let mut caret = caret_at;
            let pressed_at = match &ev {
                SEvent::Pressed(at, _) => Some(*at),
                _ => None,
            };
            let t = *tool.read();
            // a press anywhere else commits what was being typed, and that
            // press's own outcome is often nothing to save (an empty spot)
            let was_editing = scene.peek().editing.is_some();
            let outcome = scene.write().handle(ev, t);
            if was_editing && scene.peek().editing.is_none() {
                schedule_save();
            }
            match outcome {
                Outcome::Changed => schedule_save(),
                Outcome::Created { edit } => {
                    // back to select, or every click draws another one —
                    // except the pen, whose strokes come in runs: a word, a
                    // circle round something, an arrow drawn by hand
                    if !stays_armed(t) {
                        tool.set(Tool::Select);
                    }
                    schedule_save();
                    if edit {
                        let text = scene.read().edit_text.clone();
                        caret.set(None);
                        v.set(text);
                    }
                }
                Outcome::EditNode => {
                    let text = scene.read().edit_text.clone();
                    caret.set(pressed_at);
                    v.set(text);
                    schedule_save();
                }
                Outcome::OpenDoc(file, section) => {
                    let found = {
                        let index = app.index.read();
                        index
                            .doc(&file)
                            .or_else(|| {
                                index.by_stem(
                                    file.trim_end_matches(".md").trim_end_matches(".canvas"),
                                )
                            })
                            .map(|d| d.rel.clone())
                    };
                    // a section card opens its note on that section
                    if let Some(rel) = found {
                        app.open_at(&rel, section.as_deref());
                    } else {
                        app.toast(format!("nothing named {file}"), "", None);
                    }
                }
                Outcome::Menu { at, target } => {
                    let world = scene.read().view.to_world(at);
                    // the click itself: the menu flips once it knows its size
                    menu.set(Some(Menu {
                        at: (at.x as f32, at.y as f32),
                        world,
                        target,
                    }));
                }
                Outcome::None | Outcome::Quiet => {}
            }
        };

        // files dragged in from the OS: where they hover (board-local) and what
        let mut file_drag = use_state(|| None::<(f64, f64, Vec<std::path::PathBuf>)>);
        let local = move |gx: f64, gy: f64| -> KPoint {
            let (ox, oy) = *origin.read();
            KPoint::new(gx - ox as f64, gy - oy as f64)
        };

        let on_down = move |e: Event<PointerEventData>| {
            centred.set_if_modified(None);
            a11y.request_focus();
            menu.set(None);
            // a press catches a gliding board, as a hand would
            *glides.write() += 1;
            pan_trail.write().clear();
            let g = e.global_location();
            let at = local(g.x, g.y);
            let button = match &*e {
                PointerEventData::Mouse(m) => m.button,
                _ => Some(MouseButton::Left),
            };
            match button {
                Some(MouseButton::Right) => handle(SEvent::Menu(at)),
                Some(MouseButton::Middle) => {
                    pressed.set(true);
                    handle(SEvent::PanPressed(at));
                }
                _ => {
                    if *space.read() {
                        pressed.set(true);
                        handle(SEvent::PanPressed(at));
                        return;
                    }
                    let (t0, p0) = *last_click.read();
                    let double = t0.elapsed().as_millis() < 400 && (p0 - at).hypot() < 6.;
                    last_click.set((Instant::now(), at));
                    pressed.set(true);
                    handle(SEvent::Pressed(at, if double { 2 } else { 1 }));
                }
            }
        };

        let on_move = move |e: Event<PointerEventData>| {
            // ⚠️ a drop that worked sends no "hover cancelled", and the board
            // only hears the drops that land on it: the pointer moving again
            // is what says the OS drag is over (none move while it runs)
            if file_drag.peek().is_some() {
                file_drag.set(None);
            }
            let g = e.global_location();
            let at = local(g.x, g.y);
            let size = scene.read().pane_size();
            let inside = at.x >= 0. && at.y >= 0. && at.x <= size.width && at.y <= size.height;
            if *pressed.read() || inside {
                // Ctrl/⌘ held: a move snaps onto the nodes around it
                scene.write().align = app.ctrl_down();
                handle(SEvent::Dragged(at, app.shift_down()));
                // a pan's last moments, for the glide when it's let go
                if scene.peek().is_panning() {
                    let mut trail = pan_trail.write();
                    trail.push((Instant::now(), at));
                    let extra = trail.len().saturating_sub(8);
                    trail.drain(..extra);
                }
            }
            if app.drag_doc.read().is_some() {
                let mut hint = app.drop_at;
                hint.set(inside.then_some((at.x, at.y)));
            }
        };

        let on_up = move |e: Event<PointerEventData>| {
            // a note dragged in from the tree lands as a reference card
            let dragged = app.drag_doc.read().clone();
            if let Some(file) = dragged {
                let g = e.global_location();
                let at = local(g.x, g.y);
                let size = scene.read().pane_size();
                if at.x >= 0. && at.y >= 0. && at.x <= size.width && at.y <= size.height {
                    let world = scene.read().view.to_world(at);
                    scene.write().attach_ref(None, world, &file, None);
                    schedule_save();
                }
                return;
            }
            if *pressed.read() {
                pressed.set(false);
                let g = e.global_location();
                let at = local(g.x, g.y);
                // a pan let go while moving glides on (`geom::Glide`)
                let fling = scene.peek().is_panning().then(|| {
                    let mut trail = pan_trail.write();
                    // ⚠️ the release itself too: a hand that stopped and
                    // then let go has its last move long ago, and flung
                    trail.push((Instant::now(), at));
                    let t0 = trail[0].0;
                    let timed: Vec<(f64, KPoint)> = trail
                        .iter()
                        .map(|(t, p)| (t.duration_since(t0).as_secs_f64(), *p))
                        .collect();
                    trail.clear();
                    geom::release_velocity(&timed)
                });
                handle(SEvent::Released(at));
                if let Some(mut glide) = fling.and_then(geom::Glide::new) {
                    *glides.write() += 1;
                    let mine = *glides.peek();
                    // ⚠️ one step per frame the window draws, by the time
                    // that frame really took, as Freya's own animations go: a
                    // 16 ms timer stepped 60 times a second on a 160 Hz
                    // screen, so frames repeated a position and it juddered
                    let ticker = RenderingTicker::get();
                    let window = Platform::get().current_window();
                    spawn_in_scope(
                        async move {
                            window.send(UserEvent::RequestRedraw);
                            let mut last = Instant::now();
                            loop {
                                ticker.tick().await;
                                if *glides.peek() != mine {
                                    return;
                                }
                                let dt = last.elapsed().as_secs_f64();
                                last = Instant::now();
                                let Some(by) = glide.step(dt) else {
                                    return;
                                };
                                let mut scene = scene;
                                scene.write().view.offset += by;
                            }
                        },
                        board_scope,
                    );
                }
            }
        };

        let on_wheel = move |e: Event<WheelEventData>| {
            centred.set_if_modified(None);
            let at = local(e.global_location.x, e.global_location.y);
            // ⚠️ as winit gives it, the sign the Floem board passed on: +y is
            // the wheel turned away from you, which zooms in and moves the
            // board down, the way Freya's scroll views move their content.
            // Negated, the wheel zoomed backwards.
            let delta = kurbo::Vec2::new(e.delta_x, e.delta_y);
            handle(wheel(e.source, app.ctrl_down(), delta, at));
        };
        // the fork's pinch: `delta_y` is the magnification since the last one
        let on_pinch = move |e: Event<WheelEventData>| {
            centred.set_if_modified(None);
            let at = local(e.global_location.x, e.global_location.y);
            handle(SEvent::Pinched {
                delta: e.delta_y,
                at,
            });
        };

        // as this render saw it: the Esc that closes the tooltip reaches the
        // app's handler first, and must not also clear the selection here
        let popover = app.color_pick.read().is_some();
        let on_key = move |e: Event<KeyboardEventData>| {
            if matches!(&e.key, Key::Character(ch) if ch == " ") && a11y.is_focused() {
                space.set(true);
            }
            // ⚠️ the colour tooltip keeps no focus of its own: while it is open
            // its keys (⏎, digits) are its own, as the note's editor knows too
            if !a11y.is_focused()
                || scene.read().editing.is_some()
                || popover
                || app.color_pick.read().is_some()
            {
                return;
            }
            let ctrl = crate::ctrl(&e);
            let shift = e.modifiers.contains(Modifiers::SHIFT);
            match (&e.key, ctrl) {
                (Key::Named(NamedKey::Delete | NamedKey::Backspace), false) => {
                    handle(SEvent::DeleteSelected);
                    return;
                }
                (Key::Named(NamedKey::Enter), false) => {
                    handle(SEvent::EditSelected);
                    return;
                }
                (Key::Named(NamedKey::Escape), false) => {
                    tool.set(Tool::Select);
                    scene.write().selected.clear();
                    return;
                }
                _ => {}
            }
            if ctrl && (e.code == Code::KeyZ || e.code == Code::KeyY) {
                let back = e.code == Code::KeyZ && !shift;
                let (mut from, mut to) = if back { (undo, redo) } else { (redo, undo) };
                let popped = from.write().pop();
                if let Some(json) = popped
                    && let Some(canvas) = synapso_core::history::parse_canvas(&json)
                {
                    let now = synapso_core::history::canvas_json(&scene.read().canvas);
                    to.write().push(now);
                    {
                        let mut s = scene.write();
                        s.canvas = canvas;
                        s.selected.clear();
                    }
                    last.set(json);
                    // an undo can land right on what the file holds
                    let unsaved = scene.write().dirty();
                    mark_unsaved(app, &here.read(), unsaved);
                    let mut saves = saves;
                    *saves.write() += 1;
                    let generation = *saves.read();
                    spawn(async move {
                        freya::prelude::timer(std::time::Duration::from_millis(600)).await;
                        if *saves.read() == generation {
                            let _ = save_board(app, scene, &here.read());
                        }
                    });
                }
                return;
            }
            if ctrl && e.code == Code::KeyC {
                copy_selection(&scene.read());
                return;
            }
            if ctrl && e.code == Code::KeyV {
                let root = app.root();
                let size = scene.read().pane_size();
                let centre = scene
                    .read()
                    .view
                    .to_world(KPoint::new(size.width / 2., size.height / 2.));
                // a picture on the system clipboard wins; else the board's own copy
                match vault::paste_image(&root) {
                    Ok(file) => {
                        let dims = vault::image_size(&root, &file);
                        scene.write().add_image(centre, &file, dims);
                        app.reindex();
                        schedule_save();
                    }
                    Err(err) => {
                        if std::env::var_os("SYNAPSO_DEBUG").is_some() {
                            eprintln!("paste image: {err}");
                        }
                        let nodes = BOARD_CLIP.with(|c| c.borrow().clone());
                        if !nodes.is_empty() {
                            scene.write().paste_nodes(centre, &nodes);
                            schedule_save();
                        }
                    }
                }
                return;
            }
            if ctrl && e.code == Code::KeyD {
                if scene.write().duplicate_selection() {
                    schedule_save();
                }
                return;
            }
            if shift && e.code == Code::Digit1 {
                handle(SEvent::ZoomFit);
                return;
            }
            if shift && e.code == Code::Digit0 {
                handle(SEvent::SetZoom(1.));
                return;
            }
            if ctrl || shift {
                return;
            }
            let pick = match e.code {
                Code::KeyV => Some(Tool::Select),
                Code::KeyR => Some(Tool::Rect),
                Code::KeyO => Some(Tool::Ellipse),
                Code::KeyY => Some(Tool::Triangle),
                Code::KeyA => Some(Tool::Arrow),
                Code::KeyL => Some(Tool::Line),
                Code::KeyP => Some(Tool::Pen),
                Code::KeyT => Some(Tool::Text),
                Code::KeyF => Some(Tool::Frame),
                _ => None,
            };
            if let Some(t) = pick {
                tool.set(t);
            }
        };

        // what to paint, copied out once per render. ⚠️ a note's save is a
        // render too (`note_rev`): an edit in a section's middle moves nothing
        // in the index, and `section_card` only looks again when asked
        let _ = *app.note_rev.read();
        let mut frame = frame_of(&scene.read(), &app.index.read());
        if let Some(color) = &*tint.read() {
            tint_canvas(&mut frame.canvas, &frame.selected, color);
        }
        let drop_hint = app
            .drag_doc
            .read()
            .is_some()
            .then(|| *app.drop_at.read())
            .flatten();
        let view = scene.read().view;
        if (*app.canvas_zoom.peek() - view.zoom).abs() > 0.001 {
            let mut z = app.canvas_zoom;
            z.set(view.zoom);
        }
        let editing = inline_editor(&scene.read(), &edit_value.read());
        let broken = scene.read().broken.clone();
        let painter = RenderCallback::new(move |ctx| {
            if FONTS.with(|f| f.borrow().is_none()) {
                FONTS.with(|f| *f.borrow_mut() = Some(ctx.font_collection.clone()));
            }
            draw_frame(
                ctx.canvas,
                ctx.font_collection,
                &frame,
                ctx.size.width,
                ctx.size.height,
            );
            if let Some((x, y)) = drop_hint {
                let mut s = stroke(c().accent, 1.5);
                s.set_path_effect(PathEffect::dash(&[5., 4.], 0.));
                let w = 240. * view.zoom as f32;
                let h = 112. * view.zoom as f32;
                ctx.canvas.draw_rrect(
                    sk::SkRRect::new_rect_xy(
                        sk::SkRect::from_xywh(x as f32, y as f32, w, h),
                        6.,
                        6.,
                    ),
                    &s,
                );
            }
        });
        let name = crate::base_name(&rel).to_string();
        let (tone, handed_down) = {
            let index = app.index.read();
            (index.tone(&rel), index.handed_down(&rel))
        };
        let minimap_nodes = scene.read().canvas.nodes.clone();
        let current_tool = *tool.read();
        // a board at the vault's root says so, the way the note's crumbs do
        let in_folder = !crate::index::parent_folder(&rel).is_empty();
        let folder_label = match crate::index::parent_folder(&rel).trim_end_matches('/') {
            "" => "vault /".to_string(),
            folder => format!("{folder} /"),
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .a11y_id(a11y)
            .a11y_focusable(true)
            .background(painter)
            .cursor(if *space.read() {
                CursorIcon::Grab
            } else if current_tool == Tool::Select {
                CursorIcon::Default
            } else {
                CursorIcon::Crosshair
            })
            .on_sized(move |e: Event<SizedEventData>| {
                origin.set((e.area.min_x(), e.area.min_y()));
                let size = kurbo::Size::new(e.area.width() as f64, e.area.height() as f64);
                scene.read().size.set(size);
                if !*fitted.read() && size.width > 50. {
                    fitted.set(true);
                    scene.write().zoom_to_fit();
                }
                if let Some(id) = centred.peek().clone() {
                    scene.write().reveal(&id);
                }
            })
            .on_pointer_down(on_down)
            .on_global_pointer_move(on_move)
            .on_global_pointer_up(on_up)
            .on_wheel(on_wheel)
            .on_pinch(on_pinch)
            .on_global_file_hover(move |e: Event<FileEventData>| {
                let (x, y) = crate::file_cursor(&e);
                let p = local(x, y);
                let size = scene.peek().pane_size();
                if p.x < 0. || p.y < 0. || p.x > size.width || p.y > size.height {
                    file_drag.set(None);
                    return;
                }
                // one event per file: gather them, and follow the cursor
                let mut paths = file_drag
                    .peek()
                    .as_ref()
                    .map(|(.., v)| v.clone())
                    .unwrap_or_default();
                for f in &e.file_paths {
                    if !paths.contains(f) {
                        paths.push(f.clone());
                    }
                }
                file_drag.set(Some((p.x, p.y, paths)));
            })
            .on_global_file_hover_cancelled(move |_| file_drag.set(None))
            .on_file_drop(move |e: Event<FileEventData>| {
                e.stop_propagation();
                file_drag.set(None);
                let root = app.root();
                let (x, y) = crate::file_cursor(&e);
                let at = scene.read().view.to_world(local(x, y));
                let mut added = 0;
                for path in &e.file_paths {
                    if let Some(rel) = crate::absorb(app, path, &here.read()) {
                        // several land in a little cascade from the cursor
                        let step = 24. * added as f64;
                        let at = KPoint::new(at.x + step, at.y + step);
                        if vault::is_image(&rel) {
                            let dims = vault::image_size(&root, &rel);
                            scene.write().add_image(at, &rel, dims);
                        } else {
                            scene.write().attach_ref(None, at, &rel, None);
                        }
                        added += 1;
                    }
                }
                if added > 0 {
                    app.reindex();
                    schedule_save();
                }
            })
            .on_global_key_down(on_key)
            .on_global_key_up(move |e: Event<KeyboardEventData>| {
                if matches!(&e.key, Key::Character(ch) if ch == " ") {
                    space.set(false);
                }
            })
            .child(
                // the board chip
                rect()
                    .position(Position::new_absolute().left(14.).top(10.))
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(6.)
                    .padding((4., 8.))
                    .background(c().surface_panel)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(4.)
                    .shadow(Shadow::new().y(1.).blur(2.).color(c().shadow(0.9)))
                    .font_size(TEXT_SM)
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    // as the note's crumbs: the folder's line (the vault is no
                    // group, so none at the root), then the board's own only
                    // when its colour isn't the one the folder hands down,
                    // still a colour control (design 4)
                    .maybe_child(in_folder.then(|| {
                        rect()
                            .width(Size::px(2.))
                            .height(Size::px(10.))
                            .background(c().tone(handed_down))
                    }))
                    .child(label().text(folder_label).color(c().text_faint))
                    .maybe_child((tone != handed_down).then(|| crate::Spine {
                        key: rel.clone(),
                        tone,
                        height: Size::px(10.),
                        glow: false,
                    }))
                    .child(
                        label()
                            .text(name)
                            .color(c().text_primary)
                            .font_weight(FontWeight::MEDIUM),
                    )
                    .child(
                        rect()
                            .on_sized(move |e: Event<SizedEventData>| chevron_area.set(e.area))
                            .on_press(move |_| {
                                let open = *switcher.read();
                                switcher.set(!open);
                            })
                            .child(tag("▾")),
                    )
                    .maybe_child(broken.map(|why| {
                        label()
                            .text(format!("did not load — {why}"))
                            .font_size(TEXT_XS)
                            .color(c().danger)
                    })),
            )
            .child(ToolRail {
                tool,
                scene,
                changed: EventHandler::new(move |_: ()| schedule_save()),
            })
            .maybe_child(file_drag.read().clone().map(|(x, y, paths)| {
                DropOverlay {
                    x: x as f32,
                    y: y as f32,
                    paths,
                    folder: crate::index::parent_folder(&rel),
                }
                .into_element()
            }))
            .maybe_child({
                // a tool in hand and nothing selected: the bar is the tool's,
                // what the next stroke, shape, text or arrow is made with
                let idle = scene.read().selected.is_empty();
                let pen = current_tool == Tool::Pen && idle;
                let next =
                    (idle && scene.read().next(current_tool).is_some()).then_some(current_tool);
                let styleable = match next {
                    Some(t) => scene.read().next_styleable(t),
                    None => scene.read().styleable(),
                };
                ((!idle || pen || next.is_some()) && scene.read().editing.is_none()).then(|| {
                    StyleBar {
                        scene,
                        styleable,
                        pen,
                        next,
                        tint,
                        changed: EventHandler::new(move |_: ()| schedule_save()),
                    }
                    .into_element()
                })
            })
            .maybe_child(switcher.read().then(|| {
                // the open boards, as the rail holds the open notes. Like the
                // shapes flyout, a press anywhere else (the board, the other
                // pane, the tree) or any key shuts it, and still does its job
                let boards = app.canvases.read().clone();
                rect()
                    .position(Position::new_absolute().left(14.).top(40.))
                    .layer(Layer::Relative(10))
                    .min_width(Size::px(220.))
                    // ⚠️ fit + fill_minimum rows: a plain fill row is as wide
                    // as the pane, and so is the menu around it
                    .content(Content::fit())
                    .padding(4.)
                    .background(c().surface_overlay)
                    .border(Border::new().fill(c().border_strong).width(1.))
                    .corner_radius(4.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .on_sized(move |e: Event<SizedEventData>| switcher_area.set(e.area))
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    .on_global_pointer_down(move |e: Event<PointerEventData>| {
                        let p = e.global_location();
                        let keep = [*switcher_area.peek(), *chevron_area.peek()];
                        if shuts(p.x as f32, p.y as f32, &keep) {
                            switcher.set(false);
                        }
                    })
                    .on_global_key_down(move |_| switcher.set(false))
                    .children(
                        boards
                            .into_iter()
                            .map(|rel| BoardRow { rel, switcher }.into_element()),
                    )
                    .child(
                        rect()
                            .width(Size::fill_minimum())
                            .padding((5., 10.))
                            .on_press(move |_| {
                                switcher.set(false);
                                crate::new_canvas(app, None);
                            })
                            .child(
                                label()
                                    .text("+ new canvas")
                                    .font_size(TEXT_SM)
                                    .color(c().accent),
                            ),
                    )
            }))
            .child(Minimap {
                nodes: minimap_nodes,
                view,
                zoom: view.zoom,
            })
            .maybe_child(editing.map(|ed| {
                let v = edit_value;
                let r = ed.at;
                rect()
                    .position(Position::new_absolute().left(r.x0 as f32).top(r.y0 as f32))
                    // laid out at the text's own size, then zoomed as the
                    // board draws it (`Inline::zoom`)
                    .width(Size::px(r.width() as f32 / ed.zoom))
                    .scale(ed.zoom)
                    .transform_origin(TransformOrigin::top_left())
                    .maybe(!ed.bare, |b| {
                        b.padding((2., 6.))
                            .background(c().surface_overlay)
                            .border(Border::new().fill(c().accent).width(1.))
                            .corner_radius(3.)
                    })
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    .child(
                        Field::new(
                            v,
                            move |value: String| {
                                let mut scene = scene;
                                {
                                    let mut s = scene.write();
                                    s.set_edit_text(&value);
                                    s.commit_edit();
                                }
                                schedule_save();
                            },
                            move |_| {
                                let mut scene = scene;
                                scene.write().cancel_edit();
                                schedule_save();
                            },
                        )
                        .multiline()
                        // thin on screen at any zoom
                        .caret(1.5 / ed.zoom)
                        .place(match *caret_at.peek() {
                            // into the field's own layout pixels: the zoom
                            // taken off a bare one, the well's padding off
                            // a boxed one
                            Some(p) if ed.bare => Place::At(
                                ((p.x - r.x0) / f64::from(ed.zoom)) as f32,
                                ((p.y - r.y0) / f64::from(ed.zoom)) as f32,
                            ),
                            Some(p) => {
                                Place::At((p.x - r.x0 - 6.) as f32, (p.y - r.y0 - 2.) as f32)
                            }
                            None => Place::End,
                        })
                        .size(ed.size)
                        .weight(ed.weight)
                        .looks(ed.family, ed.color, ed.align),
                    )
            }))
            .maybe_child(menu.read().clone().map(|m| {
                ContextMenu {
                    menu: m,
                    scene,
                    close: menu,
                    rel: rel.clone(),
                    changed: EventHandler::new(move |_: ()| schedule_save()),
                }
                .into_element()
            }))
            .child(
                label()
                    .position(Position::new_absolute().left(60.).bottom(10.))
                    .text("drag from tree · reference   right-click · menu")
                    .font_size(TEXT_XS)
                    .color(c().text_faint),
            )
            .into_element()
    }
}

/// A row of the board switcher: an open board, opened by a press and closed
/// by the × its end slot shows on hover, as a rail tab is.
#[derive(PartialEq)]
struct BoardRow {
    rel: String,
    switcher: State<bool>,
}

impl Component for BoardRow {
    fn render_key(&self) -> DiffKey {
        (&self.rel).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let mut switcher = self.switcher;
        let rel = self.rel.clone();
        let hov = *hover.read();
        let dirty = app.dirty.read().contains(&rel);
        let color = c().tone(app.index.read().tone(&rel));
        let open = rel.clone();
        rect()
            .width(Size::fill_minimum())
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(6.)
            .padding((5., 6., 5., 10.))
            .corner_radius(3.)
            .background(if hov {
                c().surface_raised
            } else {
                Color::TRANSPARENT
            })
            // ⚠️ over/out, not enter/leave: those are exclusive, so reaching
            // the × would leave the row and take the × away under the cursor
            .on_pointer_over(move |_| hover.set(true))
            .on_pointer_out(move |_| hover.set(false))
            .on_press(move |_| {
                switcher.set(false);
                app.open(&open);
            })
            .child(label().text(format!("◇ {rel}")).font_size(TEXT_SM))
            // ⚠️ the slot's room in the flow, and the slot itself hung from
            // the row's end (an absolute child is placed inside the padding),
            // so every row's × sits in one column: a flex spacer, or
            // SpaceBetween, lays it out against the pane's width, not the menu's
            .child(rect().width(Size::px(16.)).height(Size::px(16.)))
            .child(
                rect()
                    .position(Position::new_absolute().right(0.).top(0.))
                    .child(crate::rail::end_slot(app, rel, hov, dirty, color)),
            )
    }
}

/// What a turn of the wheel does on the board (Weber, 2026-10-20): a mouse
/// wheel zooms about the cursor, ⌘ or not; a trackpad's two fingers pan, and
/// zoom under ⌘. A pinch zooms about the fingers, and is not a wheel
/// (`on_pinch`).
///
/// ⚠️ line or pixel deltas are the only tell between the two devices, and it
/// is the device's, not the user's: a Magic Mouse sends pixel deltas, so it
/// pans like a trackpad.
fn wheel(source: WheelSource, ctrl: bool, delta: kurbo::Vec2, at: KPoint) -> SEvent {
    SEvent::Scrolled {
        delta,
        zoom: ctrl || source == WheelSource::Line,
        at,
    }
}

/// Whether a press anywhere in the window shuts a popover: anywhere but on
/// the areas it keeps — itself, and the key that opened it, whose own press
/// toggles it already (shut here too, that press would open it straight back).
fn shuts(x: f32, y: f32, keep: &[Area]) -> bool {
    !keep
        .iter()
        .any(|a| x >= a.min_x() && x <= a.max_x() && y >= a.min_y() && y <= a.max_y())
}

/// Whether a tool is still in hand after it made something (the Floem
/// shell's rule): shapes and connectors hand the board back to select, the
/// pen keeps drawing until Esc, V or another tool.
fn stays_armed(tool: Tool) -> bool {
    tool == Tool::Pen
}

/// Write the board if it holds anything unsaved, and keep `app.dirty` (the
/// status bar's "saving…") true to it: the rel leaves once the file has what
/// is on screen and stays while a save fails. A board that did not load is
/// never dirty (the core's rule), so it never saves and never shows unsaved.
fn save_board(app: App, mut scene: State<Scene>, rel: &str) -> Option<std::io::Result<()>> {
    let (result, unsaved) = {
        let mut s = scene.write();
        let result = s.dirty().then(|| s.save());
        (result, s.dirty())
    };
    mark_unsaved(app, rel, unsaved);
    result
}

fn mark_unsaved(app: App, rel: &str, unsaved: bool) {
    let mut dirty = app.dirty;
    if dirty.read().contains(rel) == unsaved {
        return;
    }
    if unsaved {
        dirty.write().insert(rel.to_string());
    } else {
        dirty.write().remove(rel);
    }
}

/// ⌘C and the menu's copy: the nodes go to BOARD_CLIP for ⌘V on any board,
/// and the system's copy is a `[[canvas: …]]` reference, live when it lands
/// in a note.
fn copy_selection(scene: &Scene) {
    let nodes = scene.copied_nodes();
    if !nodes.is_empty() {
        BOARD_CLIP.with(|c| *c.borrow_mut() = nodes);
    }
    if let Some(text) = scene.copy_text() {
        let _ = freya::clipboard::Clipboard::set(text);
    }
}

/// The menu's "new note here": a fresh note in the board's folder, pinned
/// where the menu opened, its name ready to type in the breadcrumb.
fn new_note_card(app: App, mut scene: State<Scene>, world: KPoint, folder: &str) {
    let root = app.root();
    let dir = root.join(folder);
    let path = vault::unique_path(&dir, "untitled", "md");
    if vault::write(&path, "").is_err() {
        return;
    }
    let rel = crate::index::rel_of(&root, &path);
    scene.write().attach_ref(None, world, &rel, None);
    app.reindex();
    crate::note::flush_note();
    let mut n = app.note;
    n.set(rel.clone());
    let mut tabs = app.tabs;
    if !tabs.read().contains(&rel) {
        tabs.write().push(rel.clone());
    }
    let mut v = app.view;
    if *v.read() == crate::app::View::Canvas {
        v.set(crate::app::View::Split);
    }
    let mut naming = app.naming;
    naming.set(Some(rel));
}

fn empty_board(app: App) -> impl IntoElement {
    rect().expanded().center().background(c().canvas_bg).child(
        rect()
            .padding((24., 32.))
            .spacing(8.)
            .cross_align(Alignment::Center)
            .border(Border::new().fill(c().border_strong).width(1.))
            .corner_radius(6.)
            .child(
                label()
                    .text("the canvas is a hub")
                    .font_size(TEXT_SM)
                    .font_weight(FontWeight::MEDIUM),
            )
            .child(
                label()
                    .text("create a note, then drag it here from the tree")
                    .font_size(TEXT_XS)
                    .color(c().text_muted),
            )
            .child(
                rect()
                    .margin((8., 0., 0., 0.))
                    .background(c().accent)
                    .corner_radius(4.)
                    .padding((5., 14.))
                    .on_press(move |_| crate::new_canvas(app, None))
                    .child(
                        label()
                            .text("new canvas")
                            .font_size(TEXT_SM)
                            .color(c().on_accent),
                    ),
            ),
    )
}

/// The context menu's width: the design's canvas menu.
const MENU_W: f32 = 220.;

#[derive(Clone, Copy, PartialEq)]
enum Act {
    NewNote,
    NewFolder,
    Fit,
    Open,
    Front,
    Back,
    Rename,
    Copy,
    Duplicate,
    Delete,
    Curve,
    Elbow,
    Straight,
    Dash,
}

#[derive(PartialEq)]
struct ContextMenu {
    menu: Menu,
    scene: State<Scene>,
    close: State<Option<Menu>>,
    rel: String,
    changed: EventHandler<()>,
}

impl Component for ContextMenu {
    // a new click is a new menu, measured afresh: a card's has more rows
    fn render_key(&self) -> DiffKey {
        (&(self.menu.at.0.to_bits(), self.menu.at.1.to_bits())).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut size = use_state(|| None::<(f32, f32)>);
        let scene = self.scene;
        let m = self.menu.clone();
        let folder = crate::index::parent_folder(&self.rel);
        let folder_shown = match folder.trim_end_matches('/') {
            "" => "vault".to_string(),
            f => f.to_string(),
        };
        let acts: Vec<(Act, &'static str, &'static str)> = match &m.target {
            None => vec![
                (Act::NewNote, "new note here", ""),
                (Act::NewFolder, "", ""),
                (Act::Fit, "zoom to fit", "⇧1"),
            ],
            Some(Selection::Node(id)) => {
                let mut v = vec![];
                if scene
                    .read()
                    .canvas
                    .node(id)
                    .is_some_and(|n| n.file.is_some())
                {
                    v.push((Act::Open, "open", "dbl-click"));
                }
                v.extend([
                    (Act::Front, "bring to front", ""),
                    (Act::Back, "send to back", ""),
                    (Act::Rename, "rename", "⏎"),
                    (Act::Copy, "copy", "⌘C"),
                    (Act::Duplicate, "duplicate", "⌘D"),
                    (Act::Delete, "delete", "⌫"),
                ]);
                v
            }
            Some(Selection::Edge(_)) => vec![
                (Act::Curve, "curve", ""),
                (Act::Elbow, "elbow", ""),
                (Act::Straight, "straight", ""),
                (Act::Dash, "dashed", ""),
                (Act::Delete, "delete", "⌫"),
            ],
        };
        let target = m.target.clone();
        let world = m.world;
        let close = self.close;
        let changed = self.changed.clone();
        let run = move |act: Act| {
            let mut scene = scene;
            let mut close = close;
            close.set(None);
            let id = target.as_ref().and_then(|t| t.node().map(str::to_string));
            match act {
                Act::NewNote => new_note_card(app, scene, world, &folder),
                Act::NewFolder => {
                    crate::new_folder(app, &folder);
                    let mut t = app.tree;
                    t.set(true);
                }
                Act::Fit => {
                    let _ = scene.write().handle(SEvent::ZoomFit, Tool::Select);
                }
                Act::Open => {
                    let file = id.and_then(|id| {
                        let scene = scene.read();
                        let n = scene.canvas.node(&id)?;
                        Some((n.file.clone()?, n.section().map(str::to_string)))
                    });
                    if let Some((f, section)) = file {
                        let rel = app.index.read().doc(&f).map(|d| d.rel.clone());
                        if let Some(r) = rel {
                            app.open_at(&r, section.as_deref());
                        }
                    }
                }
                Act::Front | Act::Back => {
                    if let Some(id) = id {
                        scene.write().restack(&id, act == Act::Front);
                    }
                }
                Act::Rename => {
                    let _ = scene.write().handle(SEvent::EditSelected, Tool::Select);
                }
                Act::Copy => copy_selection(&scene.read()),
                Act::Duplicate => {
                    scene.write().duplicate_selection();
                }
                Act::Delete => {
                    let _ = scene.write().handle(SEvent::DeleteSelected, Tool::Select);
                }
                Act::Curve => scene.write().set_route(vault::Route::Curve),
                Act::Elbow => scene.write().set_route(vault::Route::Elbow),
                Act::Straight => scene.write().set_route(vault::Route::Straight),
                Act::Dash => scene.write().toggle_dash(),
            }
            changed.call(());
        };
        // ⚠️ the pane, not the window: the pane clips it. The old flip
        // guessed 200px for the height, so a 3-row menu opened ~110px above a
        // click near the bottom.
        let pane = scene.read().pane_size();
        let pane = Area::from_size(Size2D::new(pane.width as f32, pane.height as f32));
        let (x, y) = crate::overlays::menu_at(m.at, size.read().unwrap_or_default(), pane);
        rect()
            .position(Position::new_absolute().left(x).top(y))
            // hidden until measured: it can only flip knowing its size
            .opacity(if size.read().is_some() { 1. } else { 0. })
            .on_sized(move |e: Event<SizedEventData>| {
                size.set_if_modified(Some((e.area.width(), e.area.height())))
            })
            .layer(Layer::Relative(10))
            // the design's canvas menu is 220px; ⚠️ without a width the
            // fill rows make it as wide as the pane
            .width(Size::px(MENU_W))
            .padding(4.)
            .background(c().surface_overlay)
            .border(Border::new().fill(c().border_subtle).width(1.))
            .corner_radius(6.)
            .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .children(acts.into_iter().map(|(act, l, k)| {
                let run = run.clone();
                MenuRow {
                    // the design's "new folder in auth/", or "vault/" at the root
                    label: match act {
                        Act::NewFolder => format!("new folder in {}/", folder_shown),
                        _ => l.to_string(),
                    },
                    keys: k,
                    act,
                    run: EventHandler::new(move |a: Act| run(a)),
                }
                .into_element()
            }))
    }
}

#[derive(PartialEq)]
struct MenuRow {
    label: String,
    keys: &'static str,
    act: Act,
    run: EventHandler<Act>,
}

impl Component for MenuRow {
    fn render(&self) -> impl IntoElement {
        let mut hover = use_state(|| false);
        let run = self.run.clone();
        let act = self.act;
        let danger = act == Act::Delete;
        let bg = if *hover.read() {
            if danger {
                c().danger_dim
            } else {
                c().surface_raised
            }
        } else {
            Color::TRANSPARENT
        };
        rect()
            .width(Size::fill())
            .horizontal()
            .content(Content::Flex)
            .padding((6., 8.))
            .corner_radius(3.)
            .background(bg)
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .on_press(move |_| run.call(act))
            .font_size(TEXT_SM)
            .child(
                rect()
                    .width(Size::flex(1.))
                    .child(label().text(self.label.clone()).color(if danger {
                        c().danger
                    } else if act == Act::NewFolder {
                        c().text_muted
                    } else {
                        c().text_primary
                    })),
            )
            .child(
                label()
                    .text(crate::keys::show(self.keys))
                    .font_size(TEXT_XS)
                    .color(c().text_faint),
            )
    }
}

#[derive(PartialEq)]
struct ToolRail {
    tool: State<Tool>,
    scene: State<Scene>,
    changed: EventHandler<()>,
}

/// The design's rail: select, a shapes flyout (rect · ellipse · triangle),
/// arrow, line, pen, text, then image and frame.
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Tool(Tool),
    Shapes,
    Sep,
}

// no image tool: pictures come in by paste or by dropping files on the board
const RAIL: [Slot; 9] = [
    Slot::Tool(Tool::Select),
    Slot::Shapes,
    Slot::Sep,
    Slot::Tool(Tool::Arrow),
    Slot::Tool(Tool::Line),
    Slot::Tool(Tool::Pen),
    Slot::Tool(Tool::Text),
    Slot::Sep,
    Slot::Tool(Tool::Frame),
];

const SHAPES: [ToolEntry; 3] = [
    (Tool::Rect, "Rectangle", "R", icons::T_RECT),
    (Tool::Ellipse, "Ellipse", "O", icons::T_ELLIPSE),
    (Tool::Triangle, "Triangle", "Y", icons::T_TRIANGLE),
];

fn slot_meta(s: Slot, shape: Tool) -> (&'static str, &'static str, (&'static str, &'static [u8])) {
    match s {
        Slot::Tool(t) => TOOLS
            .iter()
            .find(|(tt, ..)| *tt == t)
            .map(|(_, n, k, i)| (*n, *k, *i))
            .unwrap_or(("", "", icons::T_SELECT)),
        Slot::Shapes => SHAPES
            .iter()
            .find(|(t, ..)| *t == shape)
            .map(|(_, n, k, i)| (*n, *k, *i))
            .unwrap_or(("Rectangle", "R", icons::T_RECT)),
        Slot::Sep => ("", "", icons::T_SELECT),
    }
}

impl Component for ToolRail {
    fn render(&self) -> impl IntoElement {
        let mut tool = self.tool;
        let scene = self.scene;
        let mut hover = use_state(|| None::<usize>);
        let mut flyout = use_state(|| false);
        let shape = use_state(|| Tool::Rect);
        // the shapes key, the rail and the flyout as laid out, to hang the
        // flyout off the key and to tell a press on it from one elsewhere
        let mut key_area = use_state(Area::default);
        let mut rail_area = use_state(Area::default);
        let mut fly_area = use_state(Area::default);
        let mut row_hover = use_state(|| None::<usize>);
        let shown = hover.read().unwrap_or(usize::MAX);
        let tip =
            use_animation_transition(crate::use_tracked(hover.read().is_some()), |from, to| {
                let v = |b: bool| if b { 1. } else { 0. };
                AnimNum::new(v(from), v(to)).time(120)
            });
        let t = tip.read().value();
        let mut last = use_state(|| 1usize);
        if shown != usize::MAX && *last.read() != shown {
            last.set(shown);
        }
        let tip_i = *last.read();
        let current = *tool.read();
        let cur_shape = *shape.read();
        // where each slot sits: 28px buttons, 2px gaps, 7px separators
        let top_of = |i: usize| {
            RAIL[..i]
                .iter()
                .map(|s| if *s == Slot::Sep { 7. } else { 30. })
                .sum::<f32>()
                + 4.
        };
        let rail_h = top_of(RAIL.len()) + 2. * 9.;
        // the rail is centred in the pane; popups hang beside their button
        let pane_h = scene.read().pane_size().height as f32;
        let rail_top = ((pane_h - rail_h) / 2.).max(0.);
        let beside = move |i: usize, own_h: f32| rail_top + top_of(i) + 14. - own_h / 2.;
        let (tip_name, tip_key, _) = slot_meta(RAIL[tip_i], cur_shape);

        let buttons = RAIL.iter().enumerate().map(|(i, s)| {
            let s = *s;
            if s == Slot::Sep {
                return rect()
                    .width(Size::px(24.))
                    .height(Size::px(1.))
                    .margin((3., 2.))
                    .background(c().border_subtle)
                    .into_element();
            }
            let (_, _, src) = slot_meta(s, cur_shape);
            let active = match s {
                Slot::Tool(tl) => current == tl,
                Slot::Shapes => matches!(current, Tool::Rect | Tool::Ellipse | Tool::Triangle),
                Slot::Sep => false,
            };
            let hov = shown == i;
            let color = if active {
                c().accent
            } else if hov {
                c().text_primary
            } else {
                c().text_muted
            };
            let bg = if active {
                c().accent_dim
            } else if hov {
                c().surface_overlay
            } else {
                Color::TRANSPARENT
            };
            rect()
                .width(Size::px(28.))
                .height(Size::px(28.))
                .center()
                .corner_radius(4.)
                .background(bg)
                .on_pointer_enter(move |_| hover.set(Some(i)))
                .on_pointer_leave(move |_| hover.set(None))
                .on_press(move |_| match s {
                    Slot::Tool(tl) => {
                        tool.set(tl);
                        flyout.set(false);
                    }
                    Slot::Shapes => {
                        let open = *flyout.read();
                        flyout.set(!open);
                        row_hover.set(None);
                        tool.set(*shape.read());
                    }
                    Slot::Sep => {}
                })
                .maybe(s == Slot::Shapes, |r| {
                    r.on_sized(move |e: Event<SizedEventData>| key_area.set(e.area))
                })
                .child(icons::icon(src, 16., color))
                .maybe_child((s == Slot::Shapes).then(|| {
                    // the flyout's corner mark
                    rect()
                        .position(Position::new_absolute().right(2.).bottom(2.))
                        .width(Size::px(4.))
                        .height(Size::px(4.))
                        .background(color)
                        .corner_radius(1.)
                }))
                .into_element()
        });

        // the design's flyout: a column off the shapes key, one row per shape
        // the scene draws. A pick, another tool, a press anywhere else or any
        // key shuts it, and the press or key still does what it was for.
        let flyout_el = flyout.read().then(|| {
            let (k, at) = (*key_area.read(), *rail_area.read());
            let hovered = *row_hover.read();
            rect()
                .position(
                    Position::new_absolute()
                        .left(k.min_x() - at.min_x() + 38.)
                        .top(k.min_y() - at.min_y() - 8.),
                )
                .width(Size::px(184.))
                .padding(4.)
                .spacing(1.)
                .background(c().surface_overlay)
                .border(Border::new().fill(c().border_subtle).width(1.))
                .corner_radius(6.)
                .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                .on_sized(move |e: Event<SizedEventData>| fly_area.set(e.area))
                .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                // ⚠️ not on the key: its own press toggles the flyout shut
                .on_global_pointer_down(move |e: Event<PointerEventData>| {
                    let p = e.global_location();
                    if shuts(
                        p.x as f32,
                        p.y as f32,
                        &[*fly_area.peek(), *key_area.peek()],
                    ) {
                        flyout.set(false);
                    }
                })
                .on_global_key_down(move |_| flyout.set(false))
                .child(
                    rect().padding((2., 6., 4., 6.)).child(
                        label()
                            .text("SHAPES")
                            .font_size(TEXT_XS)
                            .letter_spacing(0.4)
                            .color(c().text_faint),
                    ),
                )
                .children(SHAPES.iter().enumerate().map(|(i, (tl, name, key, src))| {
                    let tl = *tl;
                    let mut shape = shape;
                    let hov = hovered == Some(i);
                    let on = cur_shape == tl;
                    let fg = if hov {
                        c().text_primary
                    } else if on {
                        c().accent
                    } else {
                        c().text_muted
                    };
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .spacing(6.)
                        .padding((4., 6.))
                        .corner_radius(4.)
                        .background(if hov {
                            c().surface_raised
                        } else if on {
                            c().accent_dim
                        } else {
                            Color::TRANSPARENT
                        })
                        .on_pointer_enter(move |_| row_hover.set(Some(i)))
                        .on_pointer_leave(move |_| {
                            if *row_hover.peek() == Some(i) {
                                row_hover.set(None);
                            }
                        })
                        .on_press(move |_| {
                            shape.set(tl);
                            tool.set(tl);
                            flyout.set(false);
                        })
                        .child(icons::icon(*src, 14., fg))
                        .child(
                            rect()
                                .width(Size::flex(1.))
                                .child(label().text(*name).font_size(TEXT_SM).color(fg)),
                        )
                        .child(
                            rect()
                                .background(c().surface_inset)
                                .corner_radius(3.)
                                .child(tag(*key)),
                        )
                        .into_element()
                }))
                .into_element()
        });

        rect()
            .position(Position::new_absolute().left(14.).top(0.))
            .height(Size::fill())
            .main_align(Alignment::Center)
            .on_sized(move |e: Event<SizedEventData>| rail_area.set(e.area))
            .child(
                rect()
                    .background(c().surface_raised)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(6.)
                    .padding(4.)
                    .spacing(2.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    .children(buttons),
            )
            .maybe_child(flyout_el)
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(44. + 6. * (1. - t))
                            .top(beside(tip_i, 26.)),
                    )
                    .opacity(if *flyout.read() { 0. } else { t })
                    .child(
                        rect()
                            .horizontal()
                            .cross_align(Alignment::Center)
                            .spacing(6.)
                            .padding((4., 8.))
                            .background(c().surface_overlay)
                            .border(Border::new().fill(c().border_subtle).width(1.))
                            .corner_radius(4.)
                            .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                            .child(label().text(tip_name).font_size(TEXT_SM))
                            .child(tag(tip_key)),
                    ),
            )
    }
}

struct Minimap {
    nodes: Vec<vault::Node>,
    view: geom::Viewport,
    zoom: f64,
}

impl PartialEq for Minimap {
    fn eq(&self, _: &Self) -> bool {
        // nodes carry no equality; the map is cheap to redraw
        false
    }
}

/// The board's shapes fitted into `w`×`h`, as the minimap draws them — and
/// ⌘P's preview of a board. Answers where a board point lands on the map,
/// for the viewport drawn over it; `None` for an empty board.
fn draw_shapes(
    cv: &sk::Canvas,
    w: f32,
    h: f32,
    nodes: &[vault::Node],
) -> Option<impl Fn(KPoint) -> KPoint> {
    // bounds of everything, so the map always shows the whole board
    let mut b: Option<kurbo::Rect> = None;
    for n in nodes {
        let r = n.rect();
        b = Some(b.map_or(r, |b| b.union(r)));
    }
    let b = b?.inflate(40., 40.);
    let s = ((w - 8.) as f64 / b.width()).min((h - 8.) as f64 / b.height());
    let map = move |p: KPoint| KPoint::new(4. + (p.x - b.x0) * s, 4. + (p.y - b.y0) * s);
    for n in nodes {
        let r = n.rect();
        let (a, z) = (map(KPoint::new(r.x0, r.y0)), map(KPoint::new(r.x1, r.y1)));
        let rr = sk::SkRect::new(
            a.x as f32,
            a.y as f32,
            (z.x as f32).max(a.x as f32 + 2.),
            (z.y as f32).max(a.y as f32 + 2.),
        );
        match n.shape() {
            Shape::Label | Shape::Draw => {}
            Shape::Ellipse => {
                cv.draw_oval(rr, &paint(c().border_strong));
            }
            Shape::File => {
                cv.draw_rect(rr, &paint(c().doc_dim[0]));
                cv.draw_rect(
                    sk::SkRect::from_xywh(rr.left, rr.top, 2., rr.height()),
                    &paint(c().border_strong),
                );
            }
            Shape::Frame => {
                cv.draw_rect(rr, &stroke(c().border_strong, 1.));
            }
            _ => {
                cv.draw_rect(rr, &paint(c().border_strong));
            }
        }
    }
    Some(map)
}

/// A board's shapes filling the element, with no viewport over them: what
/// ⌘P previews a board as.
pub fn shapes(nodes: Vec<vault::Node>) -> impl IntoElement {
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(RenderCallback::new(move |ctx| {
            draw_shapes(ctx.canvas, ctx.size.width, ctx.size.height, &nodes);
        }))
}

impl Component for Minimap {
    fn render(&self) -> impl IntoElement {
        let nodes = self.nodes.clone();
        let view = self.view;
        let draw = RenderCallback::new(move |ctx| {
            let cv = ctx.canvas;
            let Some(map) = draw_shapes(cv, ctx.size.width, ctx.size.height, &nodes) else {
                return;
            };
            // the viewport
            let tl = view.to_world(KPoint::new(0., 0.));
            let br = view.to_world(KPoint::new(1000., 860.));
            let (a, z) = (map(tl), map(br));
            let vr = sk::SkRect::new(a.x as f32, a.y as f32, z.x as f32, z.y as f32);
            cv.draw_rect(vr, &paint(c().accent_dim));
            let mut p = stroke(c().accent, 1.);
            p.set_alpha_f(0.7);
            cv.draw_rect(vr, &p);
        });
        rect()
            .position(Position::new_absolute().right(16.).bottom(14.))
            .width(Size::px(150.))
            .height(Size::px(110.))
            .background(c().surface_panel)
            .border(Border::new().fill(c().border_subtle).width(1.))
            .corner_radius(4.)
            .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
            .overflow(Overflow::Clip)
            .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::fill())
                    .background(draw),
            )
            .child(
                label()
                    .position(Position::new_absolute().right(4.).bottom(2.))
                    .text(format!("{:.0}%", self.zoom * 100.))
                    .font_size(TEXT_XS)
                    .color(c().text_faint),
            )
    }
}

/// What the style bar's keys read their "on" from: the first node and the
/// first edge selected, the same rule the core's verbs apply a group toggle by.
#[derive(Debug, Default, PartialEq)]
struct Lit {
    shape: Option<Shape>,
    align: Option<vault::Align>,
    bold: bool,
    route: Option<vault::Route>,
    dashed: bool,
    arrow: bool,
    back: bool,
    stroke: Option<f32>,
    /// As the file spells it: a preset `"1"`–`"6"` or hex.
    color: Option<String>,
    /// Written in the app's face rather than the hand.
    mono: bool,
}

fn lit(scene: &Scene) -> Lit {
    let node = scene
        .selected
        .iter()
        .find_map(Selection::node)
        .and_then(|id| scene.canvas.node(id));
    let edge = scene.selected.iter().find_map(|what| match what {
        Selection::Edge(id) => scene.canvas.edge(id),
        Selection::Node(_) => None,
    });
    lit_of(node, edge)
}

/// What the bar lights for this node or edge: the first of the selection's,
/// or a tool's template.
fn lit_of(node: Option<&vault::Node>, edge: Option<&vault::Edge>) -> Lit {
    Lit {
        shape: node.map(vault::Node::shape),
        // ⚠️ absent is the shape's default, never "nothing lit" (ADR-004 §D2)
        align: node.map(|n| n.align().unwrap_or(n.shape().default_align())),
        bold: node.is_some_and(vault::Node::bold),
        route: edge.map(vault::Edge::route),
        dashed: edge.is_some_and(vault::Edge::dashed),
        arrow: edge.is_some_and(vault::Edge::has_arrow),
        back: edge.is_some_and(vault::Edge::back_arrow),
        stroke: node
            .map(vault::Node::stroke)
            .or_else(|| edge.map(vault::Edge::width)),
        color: node
            .and_then(|n| n.color.clone())
            .or_else(|| edge.and_then(|e| e.color.clone())),
        mono: node.map_or_else(|| edge.is_some_and(vault::Edge::mono), vault::Node::mono),
    }
}

/// The colour the style bar is previewing, worn by the selection in the
/// painted copy only: the scene and the file stay as they were.
fn tint_canvas(canvas: &mut vault::Canvas, selected: &[Selection], color: &Option<String>) {
    for what in selected {
        match what {
            Selection::Node(id) => {
                if let Some(node) = canvas.node_mut(id) {
                    node.color = color.clone();
                }
            }
            Selection::Edge(id) => {
                if let Some(edge) = canvas.edge_mut(id) {
                    edge.color = color.clone();
                }
            }
        }
    }
}

type Icon = (&'static str, &'static [u8]);
/// A key that sets one value: the value (to light it), its icon, the verb.
type Choice<T> = (T, Icon, fn(&mut Scene));

/// The thickness keys, `vault::STROKES` in order.
const WEIGHTS: [Icon; 3] = [
    icons::S_WEIGHT_THIN,
    icons::S_WEIGHT_MEDIUM,
    icons::S_WEIGHT_THICK,
];

/// One key of the style bar or the pen's: 28px, lit in the accent.
fn style_key(face: Element, on: bool, press: impl Fn() + 'static) -> Element {
    rect()
        .width(Size::px(28.))
        .height(Size::px(28.))
        .center()
        .corner_radius(4.)
        .background(if on {
            c().accent_dim
        } else {
            Color::TRANSPARENT
        })
        .cursor(CursorIcon::Pointer)
        .on_press(move |_| press())
        .child(face)
        .into_element()
}

fn style_icon(src: Icon, on: bool) -> Element {
    icons::icon(src, 16., if on { c().accent } else { c().text_muted }).into_element()
}

fn style_sep() -> Element {
    rect()
        .width(Size::px(1.))
        .height(Size::px(20.))
        .margin((0., 2.))
        .background(c().border_subtle)
        .into_element()
}

/// The style bar's colours, one click each (#183): the six JSON Canvas
/// presets, none, then the rainbow, which swaps them for the strip of any
/// colour (design 4, `colorpop`). Hovering previews on the selection.
#[derive(PartialEq)]
struct ColorRow {
    tint: State<Option<Option<String>>>,
    /// What the selection wears, as the file spells it.
    wearing: Option<String>,
    scene: State<Scene>,
    /// The pen's row: a pick is the next stroke's colour, not an edit.
    pen: bool,
    /// A tool's row: a pick is its next node's or edge's colour.
    next: Option<Tool>,
    changed: EventHandler<()>,
}

impl Component for ColorRow {
    fn render(&self) -> impl IntoElement {
        let (tint, scene, changed) = (self.tint, self.scene, self.changed.clone());
        let (wearing, pen, next) = (self.wearing.clone(), self.pen, self.next);
        let strip = use_state(|| false);
        let pad = use_state(|| None::<(f32, f32)>);
        use_drop(move || {
            let mut tint = tint;
            tint.set(None);
        });
        let preview = move |color: Option<Option<String>>| {
            let mut tint = tint;
            tint.set(color);
        };
        let apply = move |color: Option<String>| {
            let (mut scene, mut strip) = (scene, strip);
            if pen {
                scene.write().pen_color = color;
            } else if let Some(t) = next {
                scene.write().with_next(t, |s| s.set_color(color));
            } else {
                scene.write().set_color(color);
                changed.call(());
            }
            preview(None);
            strip.set(false);
        };
        if *strip.read() {
            let hex = |(h, l): (f32, f32)| {
                let c = crate::colorpop::strip_color(h, l);
                format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
            };
            return rect()
                .margin((0., 4.))
                .child(crate::colorpop::hue_strip(
                    *pad.read(),
                    move |at| {
                        let mut pad = pad;
                        pad.set(at);
                        preview(at.map(|at| Some(hex(at))));
                    },
                    move || {
                        if let Some(at) = *pad.read() {
                            apply(Some(hex(at)));
                        }
                    },
                    move || {
                        let mut strip = strip;
                        strip.set(false);
                        preview(None);
                    },
                ))
                .into_element();
        }
        let ring = |r: Rect, on: bool| {
            r.maybe(on, |r| {
                r.border(
                    Border::new()
                        .fill(c().text_primary)
                        .width(1.5)
                        .alignment(BorderAlignment::Outer),
                )
            })
        };
        let current = wearing.as_deref().and_then(synapso_core::color::canvas);
        let mut row: Vec<Element> = vec![];
        // the presets, then none
        for value in ["1", "2", "3", "4", "5", "6"]
            .map(Some)
            .into_iter()
            .chain([None])
        {
            let fill = value
                .and_then(synapso_core::color::canvas)
                .map(kcolor)
                .unwrap_or(Color::TRANSPARENT);
            let on = match value {
                Some(v) => current.is_some() && current == synapso_core::color::canvas(v),
                None => wearing.is_none(),
            };
            let value = value.map(str::to_string);
            let hovered = value.clone();
            let apply = apply.clone();
            row.push(
                ring(
                    rect()
                        .width(Size::px(14.))
                        .height(Size::px(14.))
                        .corner_radius(3.)
                        .background(fill)
                        .border(
                            Border::new()
                                .fill(if value.is_none() {
                                    c().border_strong
                                } else {
                                    Color::TRANSPARENT
                                })
                                .width(1.),
                        )
                        .cursor(CursorIcon::Pointer),
                    on,
                )
                .on_pointer_enter(move |_| preview(Some(hovered.clone())))
                .on_press(move |_| apply(value.clone()))
                .into_element(),
            );
        }
        // a colour none of the squares names lights the rainbow
        let custom = wearing.is_some()
            && !(1..=6).any(|i| synapso_core::color::canvas(&i.to_string()) == current);
        row.push(
            ring(
                rect()
                    .width(Size::px(14.))
                    .height(Size::px(14.))
                    .corner_radius(3.)
                    .background(
                        LinearGradient::new()
                            .angle(-90.)
                            .stops(crate::colorpop::hue_stops()),
                    )
                    .cursor(CursorIcon::Pointer),
                custom,
            )
            .on_pointer_enter(move |_| preview(None))
            .on_press(move |_| {
                // the strip opens on the colour the selection wears, if custom
                let start = current
                    .filter(|_| custom)
                    .map(|c| crate::colorpop::oklch_of(kcolor(c)));
                let (mut pad, mut strip) = (pad, strip);
                pad.set(start);
                strip.set(true);
            })
            .into_element(),
        );
        rect()
            .horizontal()
            .cross_align(Alignment::Center)
            .spacing(4.)
            .margin((0., 4.))
            .on_pointer_leave(move |_| preview(None))
            .children(row)
            .into_element()
    }
}

/// The style bar (ADR-004): what the selection understands, bottom centre.
/// With the pen in hand and nothing selected it is the pen's, as in the
/// Floem shell: what the next stroke is drawn with, in the same place. Any
/// other drawing tool's likewise (Weber, 2026-10-28): picked before drawing,
/// what the next shape, text or arrow is born with (`Scene::with_next`).
#[derive(PartialEq)]
struct StyleBar {
    scene: State<Scene>,
    styleable: synapso_core::board::Styleable,
    pen: bool,
    /// The tool whose next node or edge the bar dresses, when nothing is
    /// selected: no file changes, so nothing is saved.
    next: Option<Tool>,
    /// What the pointer is previewing: `Some(colour)`, `Some(None)` for none.
    tint: State<Option<Option<String>>>,
    changed: EventHandler<()>,
}

impl Component for StyleBar {
    fn render(&self) -> impl IntoElement {
        let scene = self.scene;
        let changed = self.changed.clone();
        let st = self.styleable;
        let next = self.next;
        let on = match next.and_then(|t| scene.read().next(t)) {
            Some(Template::Node(n)) => lit_of(Some(&n), None),
            Some(Template::Edge(e)) => lit_of(None, Some(&e)),
            None => lit(&scene.read()),
        };
        let act = move |f: fn(&mut Scene)| {
            let mut scene = scene;
            match next {
                Some(t) => {
                    scene.write().with_next(t, f);
                }
                None => {
                    f(&mut scene.write());
                    changed.call(());
                }
            }
        };
        let key = |src: Icon, lit: bool, f: fn(&mut Scene)| {
            let act = act.clone();
            style_key(style_icon(src, lit), lit, move || act(f))
        };
        let mut items: Vec<Element> = vec![];
        if st.any {
            items.push(
                ColorRow {
                    tint: self.tint,
                    wearing: on.color.clone(),
                    scene,
                    pen: false,
                    next,
                    changed: self.changed.clone(),
                }
                .into_element(),
            );
        }
        // ⚠️ bare text has no box, so the group about the box stays out of
        // its way — and it holds outlines only: a shape is never flattened
        // into text (`Scene::set_shape`)
        if st.text && !st.label && next.is_none() {
            items.push(style_sep());
            const SHAPES: [Choice<Shape>; 3] = [
                (Shape::Rect, icons::T_RECT, |s| s.set_shape(Shape::Rect)),
                (Shape::Ellipse, icons::T_ELLIPSE, |s| {
                    s.set_shape(Shape::Ellipse)
                }),
                (Shape::Triangle, icons::T_TRIANGLE, |s| {
                    s.set_shape(Shape::Triangle)
                }),
            ];
            for (shape, src, f) in SHAPES {
                items.push(key(src, on.shape == Some(shape), f));
            }
        }
        if st.text {
            items.push(style_sep());
            const ALIGNS: [Choice<vault::Align>; 3] = [
                (vault::Align::Left, icons::S_ALIGN_LEFT, |s| {
                    s.set_align(vault::Align::Left)
                }),
                (vault::Align::Center, icons::S_ALIGN_CENTER, |s| {
                    s.set_align(vault::Align::Center)
                }),
                (vault::Align::Right, icons::S_ALIGN_RIGHT, |s| {
                    s.set_align(vault::Align::Right)
                }),
            ];
            for (align, src, f) in ALIGNS {
                items.push(key(src, on.align == Some(align), f));
            }
            let act = act.clone();
            items.push(style_key(
                label()
                    .text("B")
                    .font_size(TEXT_SM)
                    .font_weight(FontWeight::BOLD)
                    .color(if on.bold { c().accent } else { c().text_muted })
                    .into_element(),
                on.bold,
                move || act(|s| s.toggle_bold()),
            ));
        }
        // the face of what is written: the hand or the app's, each key in its
        // own face so the choice shows what it gives
        if st.text || st.edges {
            items.push(style_sep());
            for (mono, family) in [(false, HAND), (true, FONT)] {
                let act = act.clone();
                let lit = on.mono == mono;
                items.push(style_key(
                    label()
                        .text("Aa")
                        .font_size(TEXT_SM)
                        .font_family(family)
                        .color(if lit { c().accent } else { c().text_muted })
                        .into_element(),
                    lit,
                    move || {
                        if mono {
                            act(|s| s.set_mono(true))
                        } else {
                            act(|s| s.set_mono(false))
                        }
                    },
                ));
            }
        }
        if st.edges {
            items.push(style_sep());
            const ROUTES: [Choice<vault::Route>; 3] = [
                (vault::Route::Curve, icons::S_CURVE, |s| {
                    s.set_route(vault::Route::Curve)
                }),
                (vault::Route::Elbow, icons::S_ELBOW, |s| {
                    s.set_route(vault::Route::Elbow)
                }),
                (vault::Route::Straight, icons::T_LINE, |s| {
                    s.set_route(vault::Route::Straight)
                }),
            ];
            for (route, src, f) in ROUTES {
                items.push(key(src, on.route == Some(route), f));
            }
            items.push(key(icons::S_DASH, on.dashed, |s| s.toggle_dash()));
            items.push(style_sep());
            items.push(key(icons::S_HEAD_END, on.arrow, |s| {
                s.toggle_arrow(synapso_core::board::End::To)
            }));
            items.push(key(icons::S_HEAD_START, on.back, |s| {
                s.toggle_arrow(synapso_core::board::End::From)
            }));
        }
        // a border, a pen stroke or a line's width — bare text has none
        if st.any && !st.label {
            items.push(style_sep());
            const STROKE: [fn(&mut Scene); 3] = [
                |s| s.set_stroke(0),
                |s| s.set_stroke(1),
                |s| s.set_stroke(2),
            ];
            for (i, f) in STROKE.into_iter().enumerate() {
                items.push(key(WEIGHTS[i], on.stroke == Some(vault::STROKES[i]), f));
            }
        }
        if self.pen {
            // the hand's settings, not the canvas's: nothing to save
            items.push(
                ColorRow {
                    tint: self.tint,
                    wearing: scene.read().pen_color.clone(),
                    scene,
                    pen: true,
                    next: None,
                    changed: self.changed.clone(),
                }
                .into_element(),
            );
            items.push(style_sep());
            let pen = scene.read().pen;
            for (i, w) in vault::STROKES.iter().enumerate() {
                let on = (pen - *w).abs() < 0.01;
                items.push(style_key(style_icon(WEIGHTS[i], on), on, move || {
                    let mut scene = scene;
                    scene.write().set_pen(i);
                }));
            }
        }
        rect()
            .position(Position::new_absolute().left(0.).bottom(40.))
            .width(Size::fill())
            .center()
            .child(
                rect()
                    .horizontal()
                    .cross_align(Alignment::Center)
                    .spacing(2.)
                    .padding(4.)
                    .background(c().surface_raised)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(6.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .on_pointer_down(|e: Event<PointerEventData>| e.stop_propagation())
                    .children(items),
            )
    }
}

/// A picture on the board, decoded once per file.
fn picture(path: &std::path::Path) -> Option<sk::SkImage> {
    thread_local! {
        static PICTURES: std::cell::RefCell<HashMap<std::path::PathBuf, Option<sk::SkImage>>> = std::cell::RefCell::new(HashMap::new());
    }
    PICTURES.with(|p| {
        p.borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| {
                let bytes = std::fs::read(path).ok()?;
                sk::SkImage::from_encoded(sk::SkData::new_copy(&bytes))
            })
            .clone()
    })
}

/// What a section card quotes (`index::section_excerpt`), read again only
/// when the note changed on disk: the frame is rebuilt on every render, a
/// pan is a render per step, and that must cost a stat per card, not a parse.
fn section_card(path: &std::path::Path, section: &str) -> Option<String> {
    type Read = HashMap<(std::path::PathBuf, String), (vault::Stamp, Option<String>)>;
    thread_local! {
        static READ: std::cell::RefCell<Read> = std::cell::RefCell::new(HashMap::new());
    }
    let stamp = vault::stamp(path);
    READ.with(|r| {
        let mut r = r.borrow_mut();
        let key = (path.to_path_buf(), section.to_string());
        if let Some((at, text)) = r.get(&key)
            && *at == stamp
        {
            return text.clone();
        }
        let text = vault::read(path)
            .ok()
            .and_then(|t| crate::index::section_excerpt(&t, section));
        r.insert(key, (stamp, text.clone()));
        text
    })
}

/// Files dragged over the board from the OS: the pane tints and outlines, a
/// pill says what will happen, and a ghost card sits where they will land.
#[derive(PartialEq)]
struct DropOverlay {
    x: f32,
    y: f32,
    paths: Vec<std::path::PathBuf>,
    folder: String,
}

impl Component for DropOverlay {
    fn render(&self) -> impl IntoElement {
        let images = self
            .paths
            .iter()
            .filter(|p| vault::is_image(&p.to_string_lossy()))
            .count();
        let n = self.paths.len().max(1);
        let what = if images == n {
            if n == 1 {
                "1 image".to_string()
            } else {
                format!("{n} images")
            }
        } else if n == 1 {
            "1 file".to_string()
        } else {
            format!("{n} files")
        };
        let folder = self.folder.trim_end_matches('/');
        let dest = if folder.is_empty() {
            "assets".to_string()
        } else {
            format!("{folder}/assets")
        };
        let name = match self.paths.as_slice() {
            [one] => vault::display_name(one),
            _ => what.clone(),
        };
        let (gx, gy) = (self.x, self.y);
        let paint_it = RenderCallback::new(move |ctx| {
            let (w, h) = (ctx.size.width, ctx.size.height);
            let cv = ctx.canvas;
            cv.draw_rect(sk::SkRect::from_wh(w, h), &paint(c().accent.with_a(13)));
            let mut dash = stroke(c().accent, 1.5);
            dash.set_path_effect(PathEffect::dash(&[6., 4.], 0.));
            cv.draw_rect(sk::SkRect::from_xywh(10., 10., w - 20., h - 20.), &dash);
            // the ghost: hatched accent, centred on the cursor
            let r = sk::SkRect::from_xywh(gx - 100., gy - 66., 200., 132.);
            let rr = sk::SkRRect::new_rect_xy(r, 6., 6.);
            cv.save();
            cv.clip_rrect(rr, None, true);
            let hatch = stroke(c().accent_dim, 8.);
            let mut k = -132.;
            while k < 200. {
                cv.draw_line((r.left + k, r.bottom), (r.left + k + 132., r.top), &hatch);
                k += 16.;
            }
            cv.restore();
            cv.draw_rrect(rr, &stroke(c().accent, 1.));
        });
        rect()
            .position(Position::new_absolute().left(0.).top(0.))
            .expanded()
            .layer(Layer::Relative(30))
            .background(paint_it)
            .child(
                rect()
                    .position(Position::new_absolute().left(0.).top(52.))
                    .width(Size::fill())
                    .main_align(Alignment::Center)
                    .horizontal()
                    .child(
                        rect()
                            .horizontal()
                            .cross_align(Alignment::Center)
                            .spacing(8.)
                            .padding((6., 10.))
                            .background(c().surface_overlay)
                            .border(Border::new().fill(c().accent).width(1.))
                            .corner_radius(4.)
                            .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                            .child(icons::icon(icons::DROP, 14., c().accent))
                            .child(
                                label()
                                    .text(format!("drop to place {what}"))
                                    .font_size(TEXT_SM),
                            )
                            .child(
                                label()
                                    .text(format!("at cursor · saved to {dest}"))
                                    .font_size(TEXT_XS)
                                    .color(c().text_faint),
                            ),
                    ),
            )
            .child(
                rect()
                    .position(
                        Position::new_absolute()
                            .left(gx - 100. + 6.)
                            .top(gy + 66. - 6. - 18.),
                    )
                    .padding((1., 6.))
                    .background(c().surface_overlay)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(3.)
                    .child(label().text(name).font_size(TEXT_XS)),
            )
    }
}

/// The card a mention's `#…` names on this board: `id:3` by its id, anything
/// else by its caption folded to one line, in any case.
///
/// ⚠️ An older caption may itself start with `id:`: the id form is only taken
/// when that id is on the board, or the caption lookup it meant still runs.
pub fn card_id(canvas: &vault::Canvas, card: &str) -> Option<String> {
    let by_id = card.strip_prefix("id:").and_then(|id| canvas.node(id));
    let caption = vault::one_line(card).to_lowercase();
    by_id
        .or_else(|| {
            canvas
                .nodes
                .iter()
                .find(|n| vault::one_line(n.caption()).to_lowercase() == caption)
        })
        .map(|n| n.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `canvas` painted at 100%, no pan, nothing selected.
    fn painted(canvas: vault::Canvas, w: i32, h: i32) -> sk::SkSurface {
        let frame = Frame {
            canvas,
            view: geom::Viewport::default(),
            selected: vec![],
            editing: None,
            under: Under::Nothing,
            preview: None,
            marquee: None,
            connecting: None,
            drawing: None,
            pen: (vault::STROKES[0], None),
            edge_handles: None,
            bend: None,
            guides: Vec::new(),
            rotating: false,
            cards: HashMap::new(),
            root: std::path::PathBuf::new(),
        };
        let mut surface = sk::raster_n32_premul((w, h)).unwrap();
        let mut fc = sk::FontCollection::new();
        fc.set_default_font_manager(sk::FontMgr::default(), None);
        draw_frame(surface.canvas(), &fc, &frame, w as f32, h as f32);
        surface
    }

    fn pixel(surface: &mut sk::SkSurface, at: KPoint) -> Color {
        let info = sk::ImageInfo::new(
            (1, 1),
            sk::ColorType::BGRA8888,
            sk::AlphaType::Unpremul,
            None,
        );
        let mut px = [0u8; 4];
        assert!(surface.read_pixels(&info, &mut px, 4, (at.x as i32, at.y as i32)));
        Color::from_argb(px[3], px[2], px[1], px[0])
    }

    fn rect_node(id: &str, x: f64, y: f64, w: f64, h: f64) -> vault::Node {
        vault::Node::new(
            id.into(),
            Shape::Rect,
            kurbo::Rect::new(x, y, x + w, y + h),
            "",
        )
    }

    /// The style bar lights what the selection wears: the first node's shape,
    /// its alignment (the shape's default when it has none) and stroke, and
    /// the first edge's route, dash and heads.
    #[test]
    fn the_style_bar_lights_what_the_selection_wears() {
        let mut scene = Scene::load(std::path::PathBuf::from(
            "/tmp/synapso-board-test-not-written.canvas",
        ));
        let mut oval = vault::Node::new(
            "o".into(),
            Shape::Ellipse,
            kurbo::Rect::new(0., 0., 80., 40.),
            "",
        );
        oval.set_stroke(vault::STROKES[2]);
        let edge = vault::Edge {
            id: "e".into(),
            from_node: "o".into(),
            to_node: "o".into(),
            route: Some("elbow".into()),
            dash: Some(true),
            ..vault::Edge::default()
        };
        scene.canvas = vault::Canvas {
            nodes: vec![oval],
            edges: vec![edge],
            ..vault::Canvas::default()
        };
        scene.selected = vec![Selection::Edge("e".into()), Selection::Node("o".into())];
        let on = lit(&scene);
        assert_eq!(on.shape, Some(Shape::Ellipse));
        assert_eq!(on.align, Some(Shape::Ellipse.default_align()));
        assert_eq!(on.stroke, Some(vault::STROKES[2]));
        assert_eq!(on.route, Some(vault::Route::Elbow));
        assert!(on.dashed && on.arrow && !on.back);
    }

    /// A colour previewed in the style bar is worn by what is selected, and
    /// only there: the rest of the board keeps its own.
    #[test]
    fn a_previewed_colour_tints_only_the_selection() {
        let mut canvas = vault::Canvas {
            nodes: vec![
                rect_node("a", 0., 0., 10., 10.),
                rect_node("b", 20., 0., 10., 10.),
            ],
            edges: vec![vault::Edge {
                id: "e".into(),
                from_node: "a".into(),
                to_node: "b".into(),
                ..vault::Edge::default()
            }],
            ..vault::Canvas::default()
        };
        canvas.nodes[1].color = Some("2".into());
        let picked = Some("#8BC9FA".to_string());
        tint_canvas(
            &mut canvas,
            &[Selection::Node("a".into()), Selection::Edge("e".into())],
            &picked,
        );
        assert_eq!(canvas.nodes[0].color, picked);
        assert_eq!(canvas.edges[0].color, picked);
        assert_eq!(canvas.nodes[1].color.as_deref(), Some("2"));
    }

    /// The pen stays in hand between strokes; every other tool that draws
    /// goes back to select once it has.
    #[test]
    fn only_the_pen_stays_armed() {
        for (tool, ..) in TOOLS {
            assert_eq!(stays_armed(tool), tool == Tool::Pen, "{tool:?}");
        }
    }

    /// A link's name sits on the link's layer: a box over the line covers the
    /// word too, as it covers the line.
    #[test]
    fn a_node_over_an_edge_covers_its_label() {
        let (a, b) = (
            rect_node("a", 20., 20., 80., 40.),
            rect_node("b", 300., 20., 80., 40.),
        );
        let edge = vault::Edge {
            id: "e".into(),
            from_node: "a".into(),
            to_node: "b".into(),
            label: Some("on 401".into()),
            ..vault::Edge::default()
        };
        let line = geom::route_edge(&edge, &a, &b);
        let at = geom::label_anchor(&line.points, edge.route());
        let over = rect_node("over", at.x - 40., at.y - 20., 80., 40.);
        let mut surface = painted(
            vault::Canvas {
                nodes: vec![a, b, over],
                edges: vec![edge],
                ..vault::Canvas::default()
            },
            400,
            100,
        );
        assert_eq!(pixel(&mut surface, at), c().surface_raised);
    }

    /// A close stops when the board's write is refused and no rescue copy
    /// holds what is on screen (ADR-001); a rescue, or a write, lets it go.
    #[test]
    fn a_refused_write_is_kept_only_by_a_rescue_that_holds_the_board() {
        let dir = std::env::temp_dir().join(format!("synapso-refused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("b.canvas");
        std::fs::write(&path, "{}").unwrap();
        let attempt = |scene: &mut Scene| {
            let result = scene.dirty().then(|| scene.save());
            refused(scene, result)
        };
        let mut scene = Scene::load(path.clone());
        assert_eq!(attempt(&mut scene), None, "nothing to write");
        scene.canvas.nodes.push(rect_node("a", 0., 0., 10., 10.));
        // another writer moved the file, and the rescue's folder is gone
        std::fs::write(&path, r#"{"nodes":[],"edges":[]}"#).unwrap();
        scene.rescued = Some(dir.join("gone/b-unsaved.canvas"));
        assert!(attempt(&mut scene).is_some(), "the edit is on screen only");
        // the same refusal with a rescue that lands
        scene.rescued = None;
        assert_eq!(attempt(&mut scene), None, "the rescue holds the board");
        assert!(dir.join("b-unsaved.canvas").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The caption's field lands on the shape it edits, wherever the board is
    /// looking and wherever its pane sits in the window. The press comes in
    /// board-local (global minus the pane's corner) and the field goes out
    /// board-local too (`Position::new_absolute` is relative to the board).
    #[test]
    fn the_caption_field_sits_on_the_shape_it_edits() {
        let mut scene = Scene::load(std::path::PathBuf::from(
            "/tmp/synapso-board-test-not-written.canvas",
        ));
        let mut triangle = rect_node("t", 400., 50., 90., 70.);
        triangle.set_shape(Shape::Triangle);
        scene.canvas = vault::Canvas {
            nodes: vec![rect_node("r", 100., 50., 150., 56.), triangle],
            ..vault::Canvas::default()
        };
        scene.view = geom::Viewport {
            offset: kurbo::Vec2::new(-40., 25.),
            zoom: 1.5,
        };
        // split mode: the notes pane takes the window's left side
        let pane = kurbo::Vec2::new(420., 36.);

        let shape = scene.view.screen_rect(scene.canvas.nodes[0].rect()) + pane;
        let click = shape.center();
        scene.handle(SEvent::Pressed(click - pane, 2), Tool::Select);
        let field = edit_field(&scene).expect("a double-click edits the caption") + pane;
        assert_eq!(field.x0, shape.x0, "{field:?} is not on {shape:?}");
        assert_eq!(field.width(), shape.width());
        assert!((field.center().y - click.y).abs() < 1e-9, "{field:?}");

        // a triangle's caption sits low, where the shape is wide: so does its field
        scene.cancel_edit();
        scene.selected = vec![Selection::Node("t".into())];
        scene.handle(SEvent::EditSelected, Tool::Select);
        let shape = scene.view.screen_rect(scene.canvas.nodes[1].rect()) + pane;
        let field = edit_field(&scene).unwrap() + pane;
        assert_eq!(field.x0, shape.x0);
        let line = shape.y0 + shape.height() * 0.66;
        assert!((field.center().y - line).abs() < 1e-9, "{field:?}");
    }

    /// A popover (the board switcher, the shapes flyout) shuts on a press
    /// anywhere but on itself and on the key that opened it.
    #[test]
    fn a_press_outside_a_popover_and_its_key_shuts_it() {
        // the chip's chevron, and the switcher hung under it — in split mode,
        // so the board and both of them sit right of the notes pane
        let chevron = Area::new((560., 46.).into(), (12., 14.).into());
        let menu = Area::new((434., 76.).into(), (220., 90.).into());
        let keep = [menu, chevron];
        assert!(!shuts(500., 120., &keep), "a row of the switcher");
        assert!(!shuts(565., 52., &keep), "the chevron toggles it itself");
        assert!(shuts(800., 400., &keep), "the board");
        assert!(shuts(200., 300., &keep), "the notes pane");
        assert!(shuts(30., 10., &keep), "the tree / the title bar");
    }

    /// Weber's call (2026-10-20): a mouse wheel zooms about the cursor, with
    /// or without ⌘; a trackpad's two fingers pan, and zoom under ⌘.
    #[test]
    fn a_mouse_wheel_zooms_and_a_trackpad_pans() {
        let (delta, at) = (kurbo::Vec2::new(0., 53.), KPoint::new(300., 200.));
        let zooms = |source, ctrl| match wheel(source, ctrl, delta, at) {
            SEvent::Scrolled {
                zoom,
                delta: d,
                at: p,
            } => {
                assert_eq!((d, p), (delta, at), "the turn and the cursor go through");
                zoom
            }
            _ => panic!("a wheel turn is a scroll"),
        };
        assert!(zooms(WheelSource::Line, false), "the mouse wheel zooms");
        assert!(zooms(WheelSource::Line, true), "and ⌘ changes nothing");
        assert!(!zooms(WheelSource::Pixel, false), "two fingers pan");
        assert!(zooms(WheelSource::Pixel, true), "⌘ + two fingers zoom");
        // a scroll made by code (accessibility) moves the board like fingers
        assert!(!zooms(WheelSource::Custom, false));
    }

    /// What `[[canvas: board#…]]` points at, as the Floem shell read it.
    #[test]
    fn a_mention_finds_its_card_by_id_and_else_by_caption() {
        let picture = vault::Node::new(
            "1".into(),
            Shape::File,
            kurbo::Rect::new(0., 0., 100., 80.),
            "assets/pasted-10.png",
        );
        let mut twin_a = rect_node("2", 200., 0., 100., 80.);
        twin_a.text = "same caption".into();
        let mut twin_b = rect_node("3", 400., 0., 100., 80.);
        twin_b.text = "same caption".into();
        let mut sprint = rect_node("4", 600., 0., 100., 80.);
        sprint.text = "Sprint  #47\nready".into();
        let mut legacy = rect_node("5", 800., 0., 100., 80.);
        legacy.text = "id:old".into();
        let mut accent = rect_node("6", 1000., 0., 100., 80.);
        accent.text = "Árvore".into();
        let canvas = vault::Canvas {
            nodes: vec![picture, twin_a, twin_b, sprint, legacy, accent],
            ..Default::default()
        };

        assert_eq!(
            card_id(&canvas, "id:1").as_deref(),
            Some("1"),
            "a picture by its id"
        );
        assert_eq!(
            card_id(&canvas, "id:3").as_deref(),
            Some("3"),
            "the id beats a twin caption"
        );
        assert_eq!(card_id(&canvas, "same caption").as_deref(), Some("2"));
        assert_eq!(
            card_id(&canvas, "sprint #47 ready").as_deref(),
            Some("4"),
            "folded like copy"
        );
        assert_eq!(card_id(&canvas, "árvore").as_deref(), Some("6"));
        assert_eq!(
            card_id(&canvas, "id:old").as_deref(),
            Some("5"),
            "an old caption that looks like an id"
        );
        assert_eq!(card_id(&canvas, "id:99"), None, "a card that is gone");
        assert_eq!(card_id(&canvas, "nothing like it"), None);
    }
}
