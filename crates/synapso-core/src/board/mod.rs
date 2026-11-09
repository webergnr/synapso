//! Board geometry — the hit-testing and resize arithmetic, in kurbo.
//!
//! Phase 2, unit 2 of `docs/floem-migration-trd.md`. None of this needs a
//! renderer: a handle is where it is, a resize is a rectangle, a click either
//! lands on a path or it does not. This is the answer the shell draws and the
//! answer the file keeps, and they have to be the same one.
//!
//! Everything here is `f64` because kurbo is. ⚠️ this used to add "the shell
//! converts at its own edge (`src/conv.rs`), which is scaffolding that dies at
//! cutover" — the cutover landed and `conv.rs` died with it; Floem is kurbo
//! all the way down and nothing converts any more.

use kurbo::{Point, Rect, Vec2};

mod scene;
pub use scene::{End, Event, Outcome, Scene, Selection, Styleable, Template, Tool, Under};

use crate::vault;

/// The smallest a node may be resized to.
pub const MIN_SIZE: f64 = 24.0;
/// How far above a node's top edge the rotate grip sits, in screen pixels:
/// the grip is drawn screen-sized, like the handles.
pub const GRIP_GAP: f64 = 26.0;

/// How far `p` is from a polyline — the shortest distance to any of its
/// segments. An edge is clickable by its ink, not by its bounding box.
///
/// The edge arrives sampled into points rather than as a curve — a habit from
/// the iced shell, whose canvas paths could not be asked anything about
/// themselves. Kurbo's can, so this could take the `BezPath` directly; the
/// sampling stays because the board already feeds it the same points it draws,
/// and a hit test that disagrees with the ink is worse than one a little coarse.
pub fn distance_to_path(line: &[Point], p: Point) -> f64 {
    line.windows(2)
        .map(|seg| distance_to_segment(seg[0], seg[1], p))
        .fold(f64::INFINITY, f64::min)
}

pub fn distance_to_segment(a: Point, b: Point, p: Point) -> f64 {
    let d = b - a;
    let len2 = d.hypot2();
    // a zero-length segment is a point, and the projection below would be 0/0
    let t = if len2 <= f64::EPSILON {
        0.0
    } else {
        ((p - a).dot(d) / len2).clamp(0.0, 1.0)
    };
    (a + d * t - p).hypot()
}

/// The top-left of a group of nodes — what a paste lands by, so the group
/// keeps its own shape wherever it goes.
pub fn corner(nodes: &[vault::Node]) -> Option<Point> {
    nodes.iter().fold(None, |corner: Option<Point>, n| {
        let (x, y) = (f64::from(n.x), f64::from(n.y));
        Some(match corner {
            Some(c) => Point::new(c.x.min(x), c.y.min(y)),
            None => Point::new(x, y),
        })
    })
}

/// The box between two points, exactly. Unlike [`normalized`] it has no floor:
/// a click that never moved must select nothing, not a 24px square of board.
pub fn spanning(a: Point, b: Point) -> Rect {
    Rect::from_points(a, b)
}

/// The box between two points, never smaller than [`MIN_SIZE`] on either axis
/// — what a drawn node comes out as.
pub fn normalized(a: Point, b: Point) -> Rect {
    Rect::from_origin_size(
        Point::new(a.x.min(b.x), a.y.min(b.y)),
        (
            (b.x - a.x).abs().max(MIN_SIZE),
            (b.y - a.y).abs().max(MIN_SIZE),
        ),
    )
}

/// The eight handles as (x, y) in {-1, 0, 1}, centre excluded.
pub fn handle_points(r: Rect) -> impl Iterator<Item = ((i8, i8), Point)> {
    [-1i8, 0, 1]
        .into_iter()
        .flat_map(move |hx| [-1i8, 0, 1].into_iter().map(move |hy| (hx, hy)))
        .filter(|&(hx, hy)| !(hx == 0 && hy == 0))
        .map(move |(hx, hy)| {
            let x = r.x0 + r.width() * (f64::from(hx) + 1.0) / 2.0;
            let y = r.y0 + r.height() * (f64::from(hy) + 1.0) / 2.0;
            ((hx, hy), Point::new(x, y))
        })
}

pub fn handle_at(r: Rect, p: Point, tolerance: f64) -> Option<(i8, i8)> {
    handle_points(r)
        .find(|(_, h)| (h.x - p.x).abs() <= tolerance && (h.y - p.y).abs() <= tolerance)
        .map(|(handle, _)| handle)
}

/// The point a resize holds still: the corner or edge opposite the handle.
/// [`resize`] never moves it, so it reads the same before and after.
pub fn anchor_point(r: Rect, (hx, hy): (i8, i8)) -> Point {
    let x = match hx {
        -1 => r.x1,
        1 => r.x0,
        _ => r.center().x,
    };
    let y = match hy {
        -1 => r.y1,
        1 => r.y0,
        _ => r.center().y,
    };
    Point::new(x, y)
}

/// Where the rotate grip sits, in world coordinates: over the top edge, turned
/// with the node so it always leads the way it is facing. Its gap is screen
/// pixels, so the further in the board is zoomed the less world it spans.
pub fn grip_point(node: &vault::Node, zoom: f64) -> Point {
    let r = node.rect();
    vault::turn(
        Point::new(r.center().x, r.y0 - GRIP_GAP / zoom),
        r.center(),
        node.radians(),
    )
}

pub fn resize(orig: Rect, (hx, hy): (i8, i8), p: Point) -> Rect {
    let (mut x, mut w) = (orig.x0, orig.width());
    let (mut y, mut h) = (orig.y0, orig.height());
    match hx {
        -1 => {
            x = p.x.min(orig.x1 - MIN_SIZE);
            w = orig.x1 - x;
        }
        1 => w = (p.x - orig.x0).max(MIN_SIZE),
        _ => {}
    }
    match hy {
        -1 => {
            y = p.y.min(orig.y1 - MIN_SIZE);
            h = orig.y1 - y;
        }
        1 => h = (p.y - orig.y0).max(MIN_SIZE),
        _ => {}
    }
    Rect::from_origin_size(Point::new(x, y), (w, h))
}

/// ⇧-resize: keeps `orig`'s aspect ratio, so a node scales instead of
/// stretching. The dominant axis drives — the dragged corner follows the
/// pointer's diagonal, a side drag anchors its edge and centres the other
/// axis. Every delta is measured from [`anchor_point`] — the edge that stays
/// put, as in [`resize`]. Measuring from `orig`'s top-left instead is right
/// only for the right/bottom handles, and off by a whole side for the others.
pub fn resize_locked(orig: Rect, (hx, hy): (i8, i8), p: Point) -> Rect {
    if orig.height() <= 1.0 {
        return resize(orig, (hx, hy), p);
    }
    let ratio = orig.width() / orig.height();
    let anchor = anchor_point(orig, (hx, hy));
    let (w, h) = if hx != 0 && hy != 0 {
        let dw = (p.x - anchor.x) * f64::from(hx);
        let dh = (p.y - anchor.y) * f64::from(hy);
        let w = if dw.abs() >= dh.abs() {
            dw.max(MIN_SIZE)
        } else {
            (dh * ratio).max(MIN_SIZE)
        };
        (w, w / ratio)
    } else if hx != 0 {
        let w = ((p.x - anchor.x) * f64::from(hx)).max(MIN_SIZE);
        (w, w / ratio)
    } else {
        let h = ((p.y - anchor.y) * f64::from(hy)).max(MIN_SIZE);
        (h * ratio, h)
    };
    // ratio lock and the size floor fight on extreme drags — scale the pair
    // up together so neither axis collapses below MIN, ratio intact
    let need = (MIN_SIZE / w).max(MIN_SIZE / h).max(1.0);
    let (w, h) = (w * need, h * need);
    let x = match hx {
        1 => anchor.x,
        -1 => anchor.x - w,
        _ => anchor.x - w / 2.0,
    };
    let y = match hy {
        1 => anchor.y,
        -1 => anchor.y - h,
        _ => anchor.y - h / 2.0,
    };
    Rect::from_origin_size(Point::new(x, y), (w, h))
}

/// How far from a shape a click still counts as on it.
pub const CLICK_SLOP: f64 = 6.0;

/// The size a caption starts at, by the shape wearing it.
pub fn caption_size(shape: crate::vault::Shape) -> f32 {
    match shape {
        // a label is text and nothing else, so it starts at reading size
        crate::vault::Shape::Label => crate::theme::TEXT_BASE,
        _ => crate::theme::TEXT_SM,
    }
}

/// How far a connector leaves its face before turning — nothing on a straight
/// line, a wide arc on a curve, a short stub on an elbow.
pub fn reach(distance: f64, route: crate::vault::Route) -> f64 {
    match route {
        crate::vault::Route::Straight => 0.0,
        crate::vault::Route::Curve => (distance * 0.4).clamp(24.0, 160.0),
        crate::vault::Route::Elbow => (distance * 0.25).clamp(12.0, 40.0),
    }
}

/// A connector, routed: the line to draw, and — when the route is a curve —
/// the two control points, so a shell can stroke a real curve instead of the
/// sample. The sample is what the hit test measures, where the label sits, and
/// what the culling box has to cover.
///
/// The control points leave along the face's own normal, which is what makes a
/// curve come out of a node square to the side it left.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeLine {
    pub points: Vec<Point>,
    pub curve: Option<(Point, Point)>,
}

/// An edge, from the file to the line to draw: its ends (`vault::edge_ends`),
/// its route, its reach and the bend it was dragged into. The one place a
/// connector's geometry is put together — the scene's hit test and handles,
/// the painter and the label all ask here, so what is grabbed is what is
/// drawn.
///
/// ⚠️ The reach is in world units: a curve keeps its shape as the board
/// zooms, as it is drawn. The scene used to work it out in screen units, so
/// away from 100% the line it hit-tested was not the line on screen.
pub fn route_edge(
    edge: &crate::vault::Edge,
    from: &crate::vault::Node,
    to: &crate::vault::Node,
) -> EdgeLine {
    let (a, sa, b, sb) = crate::vault::edge_ends(edge, from, to);
    let route = edge.route();
    let out = reach((b - a).hypot(), route);
    edge_line(a, sa, b, sb, route, out, bend_point(edge, a, b))
}

/// The point a bent edge passes through: its ends' middle, moved by the
/// edge's `bend`. `None` for a line never bent.
pub fn bend_point(edge: &crate::vault::Edge, a: Point, b: Point) -> Option<Point> {
    let [dx, dy] = edge.bend?;
    Some(a.midpoint(b) + Vec2::new(f64::from(dx), f64::from(dy)))
}

/// Where the middle of an edge's line is: the bend when it has one, halfway
/// along it otherwise. The handle that bends it sits here.
pub fn edge_middle(line: &EdgeLine, edge: &crate::vault::Edge) -> Option<Point> {
    let (a, b) = (*line.points.first()?, *line.points.last()?);
    if let Some(p) = bend_point(edge, a, b) {
        return Some(p);
    }
    Some(match line.curve {
        Some((c1, c2)) => cubic(a, c1, c2, b, 0.5),
        None => label_anchor(&line.points, crate::vault::Route::Straight),
    })
}

/// `out` is [`reach`], already in the caller's units. `bend`, when the line
/// was dragged by its middle, is a point it passes through there.
pub fn edge_line(
    a: Point,
    from: crate::vault::Side,
    b: Point,
    to: crate::vault::Side,
    route: crate::vault::Route,
    out: f64,
    bend: Option<Point>,
) -> EdgeLine {
    let step = |p: Point, side: crate::vault::Side, by: f64| {
        let n = side.normal();
        Point::new(p.x + n.x * by, p.y + n.y * by)
    };

    if route == crate::vault::Route::Curve {
        let (mut c1, mut c2) = (step(a, from, out), step(b, to, out));
        // a cubic's middle is (a + 3·c1 + 3·c2 + b) / 8: moving both control
        // points by the same amount puts it on the bend, and keeps the way
        // the curve leaves each face
        if let Some(m) = bend {
            let now = cubic(a, c1, c2, b, 0.5);
            let shift = (m - now) * (8.0 / 6.0);
            c1 += shift;
            c2 += shift;
        }
        // enough samples that a hit-test 6px wide never falls through a bend
        const STEPS: usize = 12;
        return EdgeLine {
            points: (0..=STEPS)
                .map(|i| cubic(a, c1, c2, b, i as f64 / STEPS as f64))
                .collect(),
            curve: Some((c1, c2)),
        };
    }

    let mut points = vec![a];
    // a straight line bent by its middle is two straight legs through it
    if let (crate::vault::Route::Straight, Some(m)) = (route, bend) {
        points.push(m);
    }
    if route == crate::vault::Route::Elbow {
        let (p1, p2) = (step(a, from, out), step(b, to, out));
        points.push(p1);
        // an elbow's bend moves the leg it turns on, where it has one
        match (from.horizontal(), to.horizontal()) {
            // out sideways, back sideways: turn on a shared vertical
            (true, true) => {
                let mid = bend.map_or((p1.x + p2.x) / 2.0, |m| m.x);
                points.push(Point::new(mid, p1.y));
                points.push(Point::new(mid, p2.y));
            }
            (false, false) => {
                let mid = bend.map_or((p1.y + p2.y) / 2.0, |m| m.y);
                points.push(Point::new(p1.x, mid));
                points.push(Point::new(p2.x, mid));
            }
            // one of each: a single corner does it
            (true, false) => points.push(Point::new(p2.x, p1.y)),
            (false, true) => points.push(Point::new(p1.x, p2.y)),
        }
        points.push(p2);
    }
    points.push(b);
    // a stub of zero length leaves duplicate points, and a duplicate point is
    // a zero-length segment the arrowhead would take its direction from
    points.dedup_by(|a, b| (*a - *b).hypot() < 0.5);
    EdgeLine {
        points,
        curve: None,
    }
}

/// Where a label sits on its edge. Not the middle of the chord: once the route
/// bends, that point is not on the line at all. Halfway along the line it is,
/// except on an elbow, where the middle of the longest leg is the only place
/// with room for a word.
pub fn label_anchor(line: &[Point], route: crate::vault::Route) -> Point {
    let middle = |seg: &[Point]| seg[0].midpoint(seg[1]);
    if line.len() < 2 {
        return line.first().copied().unwrap_or(Point::ZERO);
    }
    if route == crate::vault::Route::Elbow {
        return line
            .windows(2)
            .max_by(|a, b| (a[0] - a[1]).hypot().total_cmp(&(b[0] - b[1]).hypot()))
            .map_or(line[0], middle);
    }
    let total: f64 = line.windows(2).map(|s| (s[0] - s[1]).hypot()).sum();
    let mut walked = 0.0;
    for seg in line.windows(2) {
        let step = (seg[0] - seg[1]).hypot();
        if walked + step >= total / 2.0 {
            let t = if step <= f64::EPSILON {
                0.0
            } else {
                (total / 2.0 - walked) / step
            };
            return seg[0].lerp(seg[1], t);
        }
        walked += step;
    }
    middle(&line[line.len() - 2..])
}

/// A pan let go while moving keeps going and eases to a stop (Weber,
/// 2026-10-28): the board slides on, as a phone's list does, instead of
/// stopping dead under the hand. The speed falls off exponentially, so it
/// slides about `velocity × TAU` px in all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    /// Screen px per second.
    pub velocity: Vec2,
}

impl Glide {
    /// How fast the slide dies away, in seconds: a short, light coast.
    pub const TAU: f64 = 0.18;
    /// Below this, in px/s, the slide is over: nobody sees it any more.
    pub const STOP: f64 = 30.0;
    /// The fastest a fling is taken to be, px/s: a jerk of the hand on
    /// release must not throw the board off into nowhere.
    pub const MAX: f64 = 4000.0;

    /// A slide from how the pointer moved last: `None` when it was too slow
    /// to coast.
    pub fn new(velocity: Vec2) -> Option<Self> {
        let speed = velocity.hypot();
        let velocity = if speed > Self::MAX {
            velocity * (Self::MAX / speed)
        } else {
            velocity
        };
        (velocity.hypot() >= Self::STOP * 3.0).then_some(Self { velocity })
    }

    /// The next `dt` seconds of it: how far to move, or `None` when it has
    /// stopped.
    pub fn step(&mut self, dt: f64) -> Option<Vec2> {
        if self.velocity.hypot() < Self::STOP {
            return None;
        }
        // exact over the step, so a slow frame moves no further than two
        // quick ones: ∫ v·e^(−t/τ) dt from 0 to dt
        let decay = (-dt / Self::TAU).exp();
        let by = self.velocity * (Self::TAU * (1.0 - decay));
        self.velocity *= decay;
        Some(by)
    }
}

/// How fast the pointer was going as it let go: over the last of `trail`
/// (seconds, screen point), oldest first. Only the last 80 ms count — a
/// hand that stopped and then let go is not flinging.
pub fn release_velocity(trail: &[(f64, Point)]) -> Vec2 {
    let Some(&(t1, p1)) = trail.last() else {
        return Vec2::ZERO;
    };
    let Some(&(t0, p0)) = trail.iter().find(|(t, _)| t1 - t <= 0.08) else {
        return Vec2::ZERO;
    };
    let dt = t1 - t0;
    if dt < 0.008 {
        return Vec2::ZERO;
    }
    (p1 - p0) / dt
}

pub fn cubic(a: Point, c1: Point, c2: Point, b: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(
        a.x * w0 + c1.x * w1 + c2.x * w2 + b.x * w3,
        a.y * w0 + c1.y * w1 + c2.y * w2 + b.y * w3,
    )
}

/// Where the board is looking: an offset in screen pixels and a zoom. The
/// conversions are kept as arithmetic rather than an `Affine` because the
/// scene stores them as two numbers and both shells read them back — but
/// [`Viewport::affine`] hands the matrix to a renderer that would rather have
/// it than multiply every point itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub offset: Vec2,
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    pub fn to_world(self, p: Point) -> Point {
        Point::new(
            (p.x - self.offset.x) / self.zoom,
            (p.y - self.offset.y) / self.zoom,
        )
    }

    pub fn to_screen(self, p: Point) -> Point {
        Point::new(
            p.x * self.zoom + self.offset.x,
            p.y * self.zoom + self.offset.y,
        )
    }

    pub fn screen_rect(self, r: Rect) -> Rect {
        Rect::from_origin_size(
            self.to_screen(Point::new(r.x0, r.y0)),
            (r.width() * self.zoom, r.height() * self.zoom),
        )
    }

    /// The same transform as a matrix: what a renderer takes once instead of
    /// being handed every point already multiplied.
    pub fn affine(self) -> kurbo::Affine {
        kurbo::Affine::translate(self.offset) * kurbo::Affine::scale(self.zoom)
    }

    /// Zoom about a screen point, so the content under the cursor stays put.
    pub fn zoom_at(&mut self, anchor: Point, factor: f64) {
        let before = self.to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(0.25, 4.0);
        let after = self.to_world(anchor);
        self.offset += (after - before) * self.zoom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pan_let_go_moving_glides_on_and_stops() {
        // 1000 px/s across: it coasts about v·τ, and stops on its own
        let mut glide = Glide::new(Vec2::new(1000.0, 0.0)).unwrap();
        let mut slid = Vec2::ZERO;
        let mut frames = 0;
        while let Some(by) = glide.step(1.0 / 60.0) {
            slid += by;
            frames += 1;
        }
        assert!((slid.x - 1000.0 * Glide::TAU).abs() < 10.0, "{slid:?}");
        assert!(frames < 60, "over in under a second ({frames} frames)");
        // the same slide in big frames goes no further
        let mut coarse = Glide::new(Vec2::new(1000.0, 0.0)).unwrap();
        let mut far = 0.0;
        while let Some(by) = coarse.step(0.1) {
            far += by.x;
        }
        assert!(far <= 1000.0 * Glide::TAU + 0.01);
        // a slow let go doesn't coast, a wild one is capped
        assert_eq!(Glide::new(Vec2::new(40.0, 0.0)), None);
        assert_eq!(
            Glide::new(Vec2::new(0.0, 99_999.0)).unwrap().velocity.y,
            Glide::MAX
        );
    }

    #[test]
    fn only_the_last_moments_before_letting_go_count() {
        let p = |x: f64| Point::new(x, 0.0);
        // moving at 500 px/s right up to the release
        let flung = [(0.0, p(0.0)), (0.05, p(25.0)), (0.1, p(50.0))];
        assert!((release_velocity(&flung).x - 500.0).abs() < 1e-9);
        // moved, then held still for 200 ms before letting go: no fling
        let held = [(0.0, p(0.0)), (0.05, p(25.0)), (0.25, p(25.0))];
        assert_eq!(release_velocity(&held), Vec2::ZERO);
        assert_eq!(release_velocity(&[]), Vec2::ZERO);
    }
    use kurbo::Size;

    const BOX: Rect = Rect::new(10.0, 20.0, 110.0, 80.0);

    #[test]
    fn a_point_off_the_end_of_a_segment_measures_to_the_end() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        assert_eq!(distance_to_segment(a, b, Point::new(5.0, 3.0)), 3.0);
        assert_eq!(distance_to_segment(a, b, Point::new(14.0, 0.0)), 4.0);
        // a segment of no length is a point, not a division by zero
        assert_eq!(distance_to_segment(a, a, Point::new(3.0, 4.0)), 5.0);
    }

    #[test]
    fn a_path_measures_to_its_nearest_segment() {
        let line = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ];
        assert_eq!(distance_to_path(&line, Point::new(12.0, 5.0)), 2.0);
        // nothing to be near: a path of one point has no segments
        assert_eq!(distance_to_path(&line[..1], Point::ZERO), f64::INFINITY);
    }

    #[test]
    fn a_click_that_never_moved_spans_nothing_but_draws_a_node() {
        let p = Point::new(40.0, 40.0);
        assert_eq!(spanning(p, p).area(), 0.0);
        // drawing has a floor, so a node is never born unclickable
        let drawn = normalized(p, p);
        assert_eq!((drawn.width(), drawn.height()), (MIN_SIZE, MIN_SIZE));
        // and either way the box is the same wherever the drag started
        assert_eq!(
            spanning(Point::new(9.0, 9.0), p),
            spanning(p, Point::new(9.0, 9.0))
        );
    }

    #[test]
    fn the_handles_are_the_eight_around_the_box() {
        let handles: Vec<_> = handle_points(BOX).collect();
        assert_eq!(handles.len(), 8);
        assert!(handles.iter().all(|&((hx, hy), _)| (hx, hy) != (0, 0)));
        // the corners sit exactly on the box's own corners
        let corner = handles
            .iter()
            .find(|&&(h, _)| h == (-1, -1))
            .map(|&(_, p)| p);
        assert_eq!(corner, Some(Point::new(BOX.x0, BOX.y0)));
        // and each one answers to a click on it, within tolerance
        for ((hx, hy), p) in handles {
            assert_eq!(handle_at(BOX, p, 4.0), Some((hx, hy)));
        }
        assert_eq!(handle_at(BOX, BOX.center(), 4.0), None);
    }

    #[test]
    fn a_resize_holds_the_opposite_edge_still() {
        for handle in [(-1i8, -1i8), (1, -1), (-1, 1), (1, 1), (0, 1), (-1, 0)] {
            let anchor = anchor_point(BOX, handle);
            let resized = resize(BOX, handle, Point::new(200.0, 200.0));
            assert_eq!(
                anchor_point(resized, handle),
                anchor,
                "handle {handle:?} moved the edge it should have held"
            );
        }
    }

    #[test]
    fn a_resize_never_goes_under_the_floor() {
        // dragging the right handle far past the left edge
        let squashed = resize(BOX, (1, 1), Point::new(-500.0, -500.0));
        assert!(squashed.width() >= MIN_SIZE && squashed.height() >= MIN_SIZE);
        let locked = resize_locked(BOX, (1, 1), Point::new(-500.0, -500.0));
        assert!(locked.width() >= MIN_SIZE && locked.height() >= MIN_SIZE);
    }

    #[test]
    fn a_locked_resize_keeps_the_shape_it_started_with() {
        let ratio = BOX.width() / BOX.height();
        for handle in [(1i8, 1i8), (-1, -1), (1, 0), (0, -1)] {
            let out = resize_locked(BOX, handle, Point::new(300.0, 260.0));
            assert!(
                (out.width() / out.height() - ratio).abs() < 1e-9,
                "handle {handle:?} stretched the node: {out:?}"
            );
            assert_eq!(
                anchor_point(out, handle),
                anchor_point(BOX, handle),
                "handle {handle:?} moved its anchor"
            );
        }
    }

    #[test]
    fn resizing_from_the_top_left_keeps_the_far_corner_pinned() {
        let r = Rect::from_origin_size(Point::new(10.0, 10.0), Size::new(100.0, 100.0));
        let out = resize(r, (-1, -1), Point::new(40.0, 30.0));
        assert_eq!(
            out,
            Rect::from_origin_size(Point::new(40.0, 30.0), Size::new(70.0, 80.0))
        );

        // and cannot invert past the minimum
        let squashed = resize(r, (-1, -1), Point::new(999.0, 999.0));
        assert_eq!(squashed.width(), MIN_SIZE);
        assert_eq!(squashed.x0, 110.0 - MIN_SIZE);
    }

    #[test]
    fn shift_resize_keeps_the_aspect_ratio_and_the_anchor() {
        let r = Rect::from_origin_size(Point::new(10.0, 10.0), Size::new(100.0, 50.0));
        let out = resize_locked(r, (1, 1), Point::new(210.0, 160.0));
        assert_eq!(out.width() / out.height(), 2.0);
        assert_eq!(out.origin(), Point::new(10.0, 10.0));
        assert_eq!(out.size(), Size::new(200.0, 100.0));

        // vertically-driven corner keeps the same diagonal, not just the width
        let tall = resize_locked(r, (1, 1), Point::new(60.0, 110.0));
        assert_eq!(tall.width() / tall.height(), 2.0);
        assert_eq!(tall.height(), 100.0);
        assert_eq!(tall.width(), 200.0);

        // a side grab scales the box, centred on the free axis
        let side = resize_locked(r, (1, 0), Point::new(160.0, 60.0));
        assert_eq!(side.width() / side.height(), 2.0);
        assert_eq!(side.x0, 10.0);
        assert_eq!(side.center().y, r.center().y);

        // and it still cannot collapse below the minimum
        let squashed = resize_locked(r, (-1, -1), Point::new(5.0, 5.0));
        assert!(squashed.width() >= MIN_SIZE);
        assert!(squashed.height() >= MIN_SIZE);
    }

    #[test]
    fn shift_resize_from_the_left_and_top_measures_from_the_far_edge() {
        let r = Rect::from_origin_size(Point::new(10.0, 10.0), Size::new(100.0, 50.0));
        // left-middle handle dragged to the middle: the right edge is pinned at
        // 110, so the box is half as wide and the ratio pulls the height down
        let left = resize_locked(r, (-1, 0), Point::new(60.0, 60.0));
        assert_eq!(left.width(), 50.0);
        assert_eq!(left.height(), 25.0);
        assert_eq!(left.x0 + left.width(), 110.0);

        // top-left corner dragged away from the box grows it, anchored bottom-right
        let corner = resize_locked(r, (-1, -1), Point::new(-90.0, -40.0));
        assert_eq!(corner.width(), 200.0);
        assert_eq!(corner.height(), 100.0);
        assert_eq!(corner.x0 + corner.width(), 110.0);
        assert_eq!(corner.y0 + corner.height(), 60.0);
    }

    #[test]
    fn the_viewport_reads_back_what_it_wrote() {
        let view = Viewport {
            offset: Vec2::new(-40.0, 12.0),
            zoom: 1.75,
        };
        let world = Point::new(120.0, -30.0);
        let round = view.to_world(view.to_screen(world));
        assert!((round - world).hypot() < 1e-9);
        // the matrix says the same thing as the arithmetic
        let by_matrix = view.affine() * world;
        assert!((by_matrix - view.to_screen(world)).hypot() < 1e-9);
    }

    #[test]
    fn zooming_holds_the_point_under_the_cursor() {
        let mut view = Viewport::default();
        let cursor = Point::new(320.0, 180.0);
        let under = view.to_world(cursor);
        view.zoom_at(cursor, 1.5);
        assert!((view.to_world(cursor) - under).hypot() < 1e-9);
        // and it stops where the board says it stops
        for _ in 0..20 {
            view.zoom_at(cursor, 2.0);
        }
        assert_eq!(view.zoom, 4.0);
        for _ in 0..40 {
            view.zoom_at(cursor, 0.5);
        }
        assert_eq!(view.zoom, 0.25);
    }
}
