//! The graph view (design `Graph.dc.html`, `GRAPH_VIEW_SPEC.md`): one focused
//! note in the middle, the rest on rings by hop distance, beyond-depth notes
//! as dots on an outer ring. Deterministic layout (no physics), so a change
//! of focus, depth or layout animates from where things were to where they go.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    f32::consts::PI,
    time::{Duration, SystemTime},
};

use freya::animation::*;
use freya::prelude::*;
use freya_engine::prelude::{self as sk, Paint, PaintStyle, PathBuilder, PathEffect};

use crate::{
    app::{App, GraphState, View},
    base_name,
    field::Field,
    index::{DocKind, Index, Tone},
    theme::*,
};

const STAGE_W: f32 = 1040.;
const STAGE_H: f32 = 760.;
const CX: f32 = 520.;
const CY: f32 = 380.;
const WEEKS: usize = 26;
/// The beyond column's distance from the centre in branch (spec §4.5).
const BEYOND_COL: f32 = 492.;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Look {
    Focus,
    Pill,
    Dot,
}

#[derive(Clone, PartialEq, Debug)]
struct GNode {
    rel: String,
    group: String,
    tone: Tone,
    look: Look,
    dist: usize,
    degree: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    canvas: bool,
    /// A beyond dot's name shows beside it (spec §4.7); else on hover only.
    named: bool,
}

#[derive(Clone, PartialEq, Debug, Default)]
struct Layout {
    nodes: Vec<GNode>,
    /// (from, to) as indices into `nodes`, undirected for drawing.
    edges: Vec<(usize, usize)>,
    focus: String,
    depth: usize,
    branch: bool,
}

fn label_of(rel: &str) -> String {
    let base = base_name(rel);
    if rel.ends_with(".canvas") {
        format!("◇ {base}")
    } else {
        base.to_string()
    }
}

/// The vault's path as the design writes it: `~/work/sled`.
fn tilde(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    // strip_prefix is by component, so /Users/meg is not under /Users/me
    match home.and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => std::path::Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

/// `1 hop`, `2 hops`.
fn hops(k: usize) -> String {
    format!("{k} hop{}", if k == 1 { "" } else { "s" })
}

/// The guides' labels (spec §5), in stage coordinates: the centre of each
/// one and the bottom it sits on. Orbit: 8 above each ring's top, and the
/// beyond ring's when it has dots. Branch: over each column, the beyond
/// ones too when they have dots.
fn guide_labels(rings: usize, branch: bool, beyond: usize) -> Vec<(f32, f32, String)> {
    let mut out = vec![];
    if branch {
        for k in 1..=rings {
            for side in [-1., 1.] {
                out.push((CX + side * column_x(k, rings), 28., hops(k)));
            }
        }
        if beyond > 0 {
            for side in [-1., 1.] {
                out.push((CX + side * BEYOND_COL, 28., "beyond".into()));
            }
        }
        return out;
    }
    for k in 1..=rings {
        out.push((CX, CY - ring_rx(k, rings) * 0.7 - 8., hops(k)));
    }
    if beyond > 0 {
        out.push((CX, CY - 330. - 8., format!("beyond · {beyond}")));
    }
    out
}

/// Ring k's horizontal radius in orbit (spec §4.4); ry is 0.7 of it.
fn ring_rx(k: usize, rings: usize) -> f32 {
    380. * (k as f32 / rings as f32).powf(0.7)
}

/// Column k's distance from the centre in branch (spec §4.5).
fn column_x(k: usize, rings: usize) -> f32 {
    if rings == 1 {
        300.
    } else {
        190. + (k as f32 - 1.) * 240. / (rings as f32 - 1.)
    }
}

fn pill_w(rel: &str) -> f32 {
    let chars = base_name(rel).chars().count() as f32;
    chars * 6.6 + 44. + if rel.ends_with(".canvas") { 13. } else { 0. }
}

/// Off until the vault-history feature is redone; the bar and its wiring stay for then.
const HISTORY_BAR: bool = false;

fn weeks_ago(t: Option<SystemTime>) -> usize {
    t.and_then(|t| t.elapsed().ok())
        .map(|d| (d.as_secs() / (7 * 86400)) as usize)
        .unwrap_or(0)
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Days since 1970-01-01 → (month 1–12, day), proleptic Gregorian (H. Hinnant's
/// `civil_from_days`).
fn month_day(days: i64) -> (usize, u32) {
    let z = days + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as usize;
    (m, d)
}

/// The history bar's words (spec §7), `today` being `days` since the epoch:
/// the date for the week `back` weeks ago, and under each of the `n` bars
/// (oldest first) its month's name where one starts, and on the first bar
/// when there's room.
fn history_words(days: i64, back: usize, n: usize) -> (String, Vec<Option<&'static str>>) {
    let at = |back: usize| month_day(days - 7 * back as i64);
    let (m, d) = at(back);
    let now = if back == 0 {
        format!("today · {} {d}", MONTHS[m - 1])
    } else {
        format!("week of {} {d:02}", MONTHS[m - 1])
    };
    let mut last = 0;
    let mut months: Vec<Option<&'static str>> = (0..n)
        .map(|i| {
            let m = at(n - 1 - i).0;
            let new = m != last;
            last = m;
            new.then_some(MONTHS[m - 1])
        })
        .collect();
    // the first bar's month only when the next name is 3+ bars on
    if months.iter().skip(1).take(2).any(Option::is_some) {
        months[0] = None;
    }
    (now, months)
}

/// Spread `ids` around a ring near the angles they want, at least `gap`
/// apart; a fixed blocker at the top keeps the ring's label clear.
fn relax(want: &mut [(String, f32)], cap: f32) {
    let n = want.len();
    if n == 0 {
        return;
    }
    let gap = (2. * PI / n as f32 * 0.92).min(cap);
    let norm = |a: f32| {
        let mut a = a;
        while a < -PI / 2. {
            a += 2. * PI;
        }
        while a >= 3. * PI / 2. {
            a -= 2. * PI;
        }
        a
    };
    for w in want.iter_mut() {
        w.1 = norm(w.1);
    }
    want.sort_by(|a, b| a.1.total_cmp(&b.1));
    let top = -PI / 2.;
    for _ in 0..60 {
        let mut moved = false;
        // keep clear of the label slot at the top
        if let Some(first) = want.first_mut()
            && first.1 - top < 0.34
        {
            first.1 = top + 0.34;
            moved = true;
        }
        if let Some(last) = want.last_mut()
            && top + 2. * PI - last.1 < 0.34
        {
            last.1 = top + 2. * PI - 0.34;
            moved = true;
        }
        for i in 1..n {
            let d = want[i].1 - want[i - 1].1;
            if d < gap {
                let push = (gap - d) / 2.;
                want[i - 1].1 -= push;
                want[i].1 += push;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

fn circular_mean(angles: &[f32]) -> Option<f32> {
    if angles.is_empty() {
        return None;
    }
    let (s, c) = angles
        .iter()
        .fold((0., 0.), |(s, c), a| (s + a.sin(), c + a.cos()));
    Some(s.atan2(c))
}

fn compute(index: &Index, st: &GraphState, filter: &str, scale: f32) -> Layout {
    let cutoff = st.week;
    // visible: linked documents of shown groups that existed at the scrubber's week
    let visible: Vec<&crate::index::DocEntry> = index
        .docs
        .iter()
        .filter(|d| index.degree(&d.rel) > 0)
        .filter(|d| !st.hidden.contains(&index.group(&d.rel)))
        .filter(|d| weeks_ago(d.created.or(d.modified)) >= cutoff)
        .filter(|d| filter.is_empty() || d.rel.to_lowercase().contains(&filter.to_lowercase()))
        .collect();
    let set: HashSet<&str> = visible.iter().map(|d| d.rel.as_str()).collect();
    if set.is_empty() {
        return Layout::default();
    }
    let focus = if set.contains(st.focus.as_str()) {
        st.focus.clone()
    } else {
        visible
            .iter()
            .max_by_key(|d| index.degree(&d.rel))
            .map(|d| d.rel.clone())
            .unwrap_or_default()
    };
    let adj = index.neighbours();
    let mut dist: HashMap<&str, usize> = HashMap::new();
    let mut queue = VecDeque::new();
    dist.insert(focus.as_str(), 0);
    queue.push_back(focus.as_str());
    while let Some(at) = queue.pop_front() {
        let d = dist[at];
        for n in adj.get(at).into_iter().flatten() {
            if set.contains(n) && !dist.contains_key(n) {
                dist.insert(n, d + 1);
                queue.push_back(n);
            }
        }
    }
    let depth = st.depth.clamp(1, 3);
    let dist_of = |rel: &str| *dist.get(rel).unwrap_or(&99);
    let groups: Vec<String> = index.groups().into_iter().map(|(g, _)| g).collect();
    let group_rank = |rel: &str| {
        groups
            .iter()
            .position(|g| *g == index.group(rel))
            .unwrap_or(99)
    };

    let mut angle: HashMap<String, f32> = HashMap::new();
    // ring 1: by group, then degree
    let mut ring1: Vec<&str> = set.iter().copied().filter(|r| dist_of(r) == 1).collect();
    ring1.sort_by(|a, b| {
        group_rank(a)
            .cmp(&group_rank(b))
            .then(index.degree(b).cmp(&index.degree(a)))
            .then(a.cmp(b))
    });
    let n1 = ring1.len().max(1) as f32;
    for (i, r) in ring1.iter().enumerate() {
        angle.insert(r.to_string(), -PI / 2. + (i as f32 + 0.5) * 2. * PI / n1);
    }
    // deeper rings: toward their parents
    for k in 2..=depth {
        let mut want: Vec<(String, f32)> = set
            .iter()
            .copied()
            .filter(|r| dist_of(r) == k)
            .map(|r| {
                let parents: Vec<f32> = adj
                    .get(r)
                    .into_iter()
                    .flatten()
                    .filter(|n| dist_of(n) == k - 1)
                    .filter_map(|n| angle.get(*n).copied())
                    .collect();
                (r.to_string(), circular_mean(&parents).unwrap_or(0.))
            })
            .collect();
        relax(&mut want, 0.5);
        for (r, a) in want {
            angle.insert(r, a);
        }
    }
    // beyond the depth: dots on the outer ring
    let mut beyond: Vec<(String, f32)> = set
        .iter()
        .copied()
        .filter(|r| dist_of(r) > depth && *r != focus)
        .enumerate()
        .map(|(i, r)| {
            let near: Vec<f32> = adj
                .get(r)
                .into_iter()
                .flatten()
                .filter_map(|n| angle.get(*n).copied())
                .collect();
            (
                r.to_string(),
                circular_mean(&near).unwrap_or(i as f32 * 1.7),
            )
        })
        .collect();
    relax(&mut beyond, 0.3);
    for (r, a) in beyond {
        angle.insert(r, a);
    }

    let mut nodes: Vec<GNode> = vec![];
    let mut push = |rel: &str, look: Look, x: f32, y: f32, d: usize| {
        let (w, h) = match look {
            Look::Focus => (208., 58.),
            Look::Pill => (pill_w(rel), 24.),
            Look::Dot => (12., 12.),
        };
        nodes.push(GNode {
            rel: rel.to_string(),
            group: index.group(rel),
            tone: index.tone(rel),
            look,
            dist: d,
            degree: index.degree(rel),
            x,
            y,
            w,
            h,
            canvas: rel.ends_with(".canvas"),
            named: false,
        });
    };
    push(&focus, Look::Focus, CX, CY, 0);

    let mut sorted: Vec<&str> = set.iter().copied().filter(|r| *r != focus).collect();
    sorted.sort();
    if !st.branch {
        for r in &sorted {
            let d = dist_of(r);
            let a = angle.get(*r).copied().unwrap_or(0.);
            let (rx, ry, look) = if d <= depth {
                let rx = ring_rx(d, depth);
                (rx, rx * 0.7, Look::Pill)
            } else {
                (460., 330., Look::Dot)
            };
            push(r, look, CX + rx * a.cos(), CY + ry * a.sin(), d);
        }
    } else {
        // mind map: columns by level, left or right by the shared angle. A
        // column sits clear of the one inside it at the size nodes are drawn.
        let mut buckets: HashMap<(bool, usize), Vec<(&str, f32)>> = HashMap::new();
        for r in &sorted {
            let d = dist_of(r).min(depth + 1);
            let a = angle.get(*r).copied().unwrap_or(0.);
            buckets.entry((a.cos() < 0., d)).or_default().push((r, a));
        }
        let widest = |k: usize| {
            buckets
                .iter()
                .filter(|((_, kk), _)| *kk == k)
                .flat_map(|(_, v)| {
                    v.iter()
                        .map(|(r, _)| if k > depth { 12. } else { pill_w(r) })
                })
                .fold(0f32, f32::max)
        };
        let mut offs = vec![0f32; depth + 2];
        let mut inner = 104.;
        for k in 1..=depth + 1 {
            let spec = if k > depth {
                BEYOND_COL
            } else {
                column_x(k, depth)
            };
            let w = widest(k);
            let need = (inner + w / 2. + 28.) / scale;
            offs[k] = spec.max(need).max(if k > 1 {
                offs[k - 1] + (w / 2. + 40.) / scale
            } else {
                0.
            });
            inner = offs[k] * scale + w / 2.;
        }
        for ((left, k), mut items) in buckets {
            items.sort_by(|a, b| a.1.sin().total_cmp(&b.1.sin()));
            let m = items.len() as f32;
            let off = offs[k];
            let x = if left { CX - off } else { CX + off };
            let spacing = if k > depth {
                (640. / m).min(44.)
            } else {
                (660. / m).min(64.)
            };
            for (i, (r, _)) in items.iter().enumerate() {
                let y = CY + (i as f32 - (m - 1.) / 2.) * spacing.max(30. / scale);
                push(
                    r,
                    if k > depth { Look::Dot } else { Look::Pill },
                    x,
                    y,
                    dist_of(r),
                );
            }
        }
    }

    // collision pass: push overlapping boxes apart; branch moves vertically only
    for _ in 0..80 {
        let mut moved = false;
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let (a, b) = (&nodes[i], &nodes[j]);
                let ox = ((a.w + b.w) / 2. + 10.) / scale - (a.x - b.x).abs();
                let oy = ((a.h + b.h) / 2. + 6.) / scale - (a.y - b.y).abs();
                if ox <= 0. || oy <= 0. {
                    continue;
                }
                moved = true;
                let vertical = st.branch || oy < ox;
                let (fi, fj) = (nodes[i].look == Look::Focus, nodes[j].look == Look::Focus);
                let share = |me: bool, other: bool| {
                    if me {
                        0.
                    } else if other {
                        1.
                    } else {
                        0.5
                    }
                };
                if vertical {
                    let s = if nodes[i].y <= nodes[j].y { -1. } else { 1. };
                    nodes[i].y += s * oy * share(fi, fj);
                    nodes[j].y -= s * oy * share(fj, fi);
                } else {
                    let s = if nodes[i].x <= nodes[j].x { -1. } else { 1. };
                    nodes[i].x += s * ox * share(fi, fj);
                    nodes[j].x -= s * ox * share(fj, fi);
                }
            }
        }
        for n in nodes.iter_mut() {
            let (hw, hh) = (n.w / 2. / scale + 4., n.h / 2. / scale + 4.);
            n.x =
                n.x.clamp(hw.min(STAGE_W / 2.), (STAGE_W - hw).max(STAGE_W / 2.));
            n.y =
                n.y.clamp(hh.min(STAGE_H / 2.), (STAGE_H - hh).max(STAGE_H / 2.));
        }
        if !moved {
            break;
        }
    }

    if !st.branch {
        let named: Vec<bool> = nodes
            .iter()
            .map(|n| n.look == Look::Dot && dot_label_fits(n, &nodes, scale))
            .collect();
        for (n, ok) in nodes.iter_mut().zip(named) {
            n.named = ok;
        }
    }

    let pos: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.rel.as_str(), i))
        .collect();
    let mut edges = vec![];
    let mut seen = HashSet::new();
    for l in &index.links {
        if let (Some(&a), Some(&b)) = (pos.get(l.from.as_str()), pos.get(l.to.as_str())) {
            let key = (a.min(b), a.max(b));
            if seen.insert(key) {
                edges.push(key);
            }
        }
    }
    Layout {
        nodes,
        edges,
        focus,
        depth,
        branch: st.branch,
    }
}

/// Where a beyond dot's name goes (spec §4.7): above the dot in the top
/// half, below it otherwise; stage coordinates, the box's centre.
fn dot_label_at(dot: &GNode, scale: f32) -> (f32, f32) {
    let off = (dot.h / 2. + 7.) / scale;
    (dot.x, if dot.y < CY { dot.y - off } else { dot.y + off })
}

/// Whether that name (≈ chars·6 × 14) clears every pill and the focus card.
fn dot_label_fits(dot: &GNode, nodes: &[GNode], scale: f32) -> bool {
    let (x, y) = dot_label_at(dot, scale);
    let (w, h) = (label_of(&dot.rel).chars().count() as f32 * 6., 14.);
    nodes.iter().filter(|n| n.look != Look::Dot).all(|n| {
        (n.x - x).abs() * scale >= (n.w + w) / 2. || (n.y - y).abs() * scale >= (n.h + h) / 2.
    })
}

fn ease(t: f32) -> f32 {
    // close to cubic-bezier(.2,.8,.2,1)
    1. - (1. - t).powi(3)
}

#[derive(PartialEq)]
pub struct GraphView {}

impl Component for GraphView {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let gs = app.graph;
        let filter = use_state(String::new);
        let hover = use_state(|| None::<String>);
        let mut area = use_state(|| (0f32, 0f32, 800f32, 700f32));
        let playing = use_state(|| false);

        let st = gs.read().clone();
        let (_, _, aw0, ah0) = *area.read();
        let scale0 = ((aw0 - 40.) / STAGE_W)
            .min((ah0 - 120.) / STAGE_H)
            .clamp(0.4, 1.);
        let layout = {
            let index = app.index.read();
            compute(&index, &st, &filter.read(), scale0)
        };

        // animate from the last layout to this one
        let key = format!(
            "{}|{}|{}|{:?}|{}|{}|{}",
            layout.focus,
            layout.depth,
            layout.branch,
            {
                let mut h: Vec<_> = st.hidden.iter().cloned().collect();
                h.sort();
                h
            },
            st.week,
            filter.read(),
            layout.nodes.len() * 1000 + layout.edges.len()
        );
        let mut last_key = use_state(String::new);
        let mut from = use_state(HashMap::<String, (f32, f32, f32)>::new);
        let mut to = use_state(HashMap::<String, (f32, f32, f32)>::new);
        let anim =
            use_animation_transition(crate::use_tracked(key.clone()), |_: String, _: String| {
                AnimNum::new(0., 1.).time(600).function(Function::Linear)
            });
        let t = ease(anim.read().value());
        if *last_key.read() != key {
            // freeze where everything is now as the start of the next move
            let now: HashMap<String, (f32, f32, f32)> = {
                let f = from.read();
                to.read()
                    .iter()
                    .map(|(k, v)| {
                        let s = f.get(k).copied().unwrap_or(*v);
                        (
                            k.clone(),
                            (
                                s.0 + (v.0 - s.0) * t,
                                s.1 + (v.1 - s.1) * t,
                                s.2 + (v.2 - s.2) * t,
                            ),
                        )
                    })
                    .collect()
            };
            from.set(now);
            to.set(
                layout
                    .nodes
                    .iter()
                    .map(|n| (n.rel.clone(), (n.x, n.y, 1.)))
                    .collect(),
            );
            last_key.set(key.clone());
        }
        let focus_xy = (CX, CY);
        let place = |rel: &str| -> (f32, f32, f32) {
            let target = to
                .read()
                .get(rel)
                .copied()
                .unwrap_or((focus_xy.0, focus_xy.1, 1.));
            // newcomers fly out from the focus
            let start = from
                .read()
                .get(rel)
                .copied()
                .unwrap_or((focus_xy.0, focus_xy.1, 0.));
            (
                start.0 + (target.0 - start.0) * t,
                start.1 + (target.1 - start.1) * t,
                start.2 + (target.2 - start.2) * t,
            )
        };

        // the stage scales to the area; nodes keep their size
        let (ax, ay, aw, ah) = *area.read();
        let _ = (ax, ay);
        let scale = ((aw - 40.) / STAGE_W)
            .min((ah - 120.) / STAGE_H)
            .clamp(0.4, 1.);
        let sx = (aw - STAGE_W * scale) / 2.;
        let sy = 52.;
        let to_screen = move |x: f32, y: f32| (sx + x * scale, sy + y * scale);

        let hovered = hover.read().clone();
        let hot_center = hovered.clone().unwrap_or(layout.focus.clone());
        let neighbours: HashSet<String> = {
            let index = app.index.read();
            index
                .links
                .iter()
                .filter_map(|l| {
                    if l.from == hot_center {
                        Some(l.to.clone())
                    } else if l.to == hot_center {
                        Some(l.from.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        // edges and guides under the nodes, from the animated positions
        let placed: Vec<(f32, f32, f32)> = layout.nodes.iter().map(|n| place(&n.rel)).collect();
        let edge_paint: Vec<(usize, usize, bool, Color, bool)> = layout
            .edges
            .iter()
            .map(|&(a, b)| {
                let (na, nb) = (&layout.nodes[a], &layout.nodes[b]);
                let hot = na.rel == hot_center || nb.rel == hot_center;
                let other = if na.rel == hot_center { nb } else { na };
                let beyond = na.look == Look::Dot || nb.look == Look::Dot;
                (a, b, hot, c().tone(other.tone), beyond)
            })
            .collect();
        let is_focus: Vec<bool> = layout.nodes.iter().map(|n| n.look == Look::Focus).collect();
        let rings = layout.depth;
        let branch = layout.branch;
        let any_hover = hovered.is_some();
        let n_beyond = layout.nodes.iter().filter(|n| n.look == Look::Dot).count();
        let under = RenderCallback::new(move |ctx| {
            let cv = ctx.canvas;
            let p = c();
            let (w, h) = (ctx.size.width, ctx.size.height);
            cv.draw_rect(sk::SkRect::from_wh(w, h), &fill(p.canvas_bg));
            let dot = fill(p.canvas_dot);
            let mut y = 0.;
            while y < h {
                let mut x = 0.;
                while x < w {
                    cv.draw_circle((x, y), 1., &dot);
                    x += 22.;
                }
                y += 22.;
            }
            // guides
            let mut guide = line(p.border_strong, 1.);
            guide.set_alpha_f(0.7);
            guide.set_path_effect(PathEffect::dash(&[2., 5.], 0.));
            let (cx, cy) = to_screen(CX, CY);
            if !branch {
                for k in 1..=rings {
                    let rx = ring_rx(k, rings) * scale;
                    let ry = rx * 0.7;
                    cv.draw_oval(sk::SkRect::new(cx - rx, cy - ry, cx + rx, cy + ry), &guide);
                }
                let mut outer = line(p.border_strong, 1.);
                outer.set_alpha_f(0.6);
                outer.set_path_effect(PathEffect::dash(&[1., 7.], 0.));
                let (rx, ry) = (460. * scale, 330. * scale);
                if n_beyond > 0 {
                    cv.draw_oval(sk::SkRect::new(cx - rx, cy - ry, cx + rx, cy + ry), &outer);
                }
            } else {
                for k in 1..=rings {
                    let off = column_x(k, rings) * scale;
                    for s in [-1., 1.] {
                        cv.draw_line((cx + s * off, 40.), (cx + s * off, h - 90.), &guide);
                    }
                }
            }
            // edges
            for &(a, b, hot, color, beyond) in &edge_paint {
                let (pa, pb) = (placed[a], placed[b]);
                let alpha = pa.2.min(pb.2);
                let (x1, y1) = to_screen(pa.0, pa.1);
                let (x2, y2) = to_screen(pb.0, pb.1);
                let mut s = line(
                    if hot { color } else { p.connector },
                    if hot { 1.5 } else { 1. },
                );
                let base = if hot {
                    0.9
                } else if beyond {
                    0.35
                } else if any_hover {
                    0.12
                } else {
                    0.55
                };
                s.set_alpha_f(base * alpha);
                if beyond && !hot {
                    s.set_path_effect(PathEffect::dash(&[2., 4.], 0.));
                }
                let mut path = PathBuilder::new();
                path.move_to((x1, y1));
                if branch {
                    let mx = (x1 + x2) / 2.;
                    path.cubic_to((mx, y1), (mx, y2), (x2, y2));
                } else if is_focus[a] || is_focus[b] {
                    path.line_to((x2, y2));
                } else {
                    // bent 30% toward the centre
                    let (mx, my) = ((x1 + x2) / 2., (y1 + y2) / 2.);
                    let (qx, qy) = (mx + (cx - mx) * 0.3, my + (cy - my) * 0.3);
                    path.quad_to((qx, qy), (x2, y2));
                }
                cv.draw_path(&path.detach(), &s);
            }
        });

        let index = app.index.read();
        let groups = index.groups();
        let group_tones: Vec<Tone> = groups
            .iter()
            .map(|(g, _)| {
                if g.is_empty() {
                    Tone::Neutral
                } else {
                    index.tone(g)
                }
            })
            .collect();
        let unlinked: Vec<(String, Option<SystemTime>)> = index
            .docs
            .iter()
            .filter(|d| d.kind == DocKind::Note && index.degree(&d.rel) == 0)
            .map(|d| (d.rel.clone(), d.modified))
            .collect();
        let focus_doc = index.doc(&layout.focus).cloned();
        let backlinks: Vec<_> = index
            .backlinks(&layout.focus)
            .into_iter()
            .cloned()
            .collect();
        let outgoing: Vec<_> = index.outgoing(&layout.focus).into_iter().cloned().collect();
        let mentions = index.mentions(&layout.focus);
        let within = layout.nodes.iter().filter(|n| n.look == Look::Pill).count();
        let tone_of = |rel: &str| index.tone(rel);
        let back_tones: Vec<Tone> = backlinks.iter().map(|l| tone_of(&l.from)).collect();
        let out_tones: Vec<Tone> = outgoing.iter().map(|l| tone_of(&l.to)).collect();
        let focus_tone = tone_of(&layout.focus);
        let weeks: Vec<usize> = {
            let mut w = vec![0usize; WEEKS];
            for d in &index.docs {
                let k = weeks_ago(d.modified);
                if k < WEEKS {
                    w[WEEKS - 1 - k] += 1;
                }
            }
            w
        };
        let notes_now = layout.nodes.len();
        let links_now = layout.edges.len();
        drop(index);

        // 10px faint, on a canvas-coloured patch so lines under it break
        let guide_els: Vec<Element> = guide_labels(rings, branch, n_beyond)
            .into_iter()
            .map(|(x, y, text)| {
                let (x, y) = to_screen(x, y);
                // mono 10px is 6px a cell, plus the .04em tracking
                let w = text.chars().count() as f32 * 6.4 + 8.;
                rect()
                    .position(Position::new_absolute().left(x - w / 2.).top(y - 12.))
                    .width(Size::px(w))
                    .height(Size::px(14.))
                    .center()
                    .background(c().canvas_bg)
                    .child(
                        label()
                            .text(text)
                            .font_size(TEXT_XS)
                            .letter_spacing(0.4)
                            .color(c().text_faint),
                    )
                    .into_element()
            })
            .collect();
        let node_els: Vec<Element> = layout
            .nodes
            .iter()
            .map(|n| {
                let (x, y, o) = place(&n.rel);
                let (px, py) = to_screen(x, y);
                let faded = any_hover
                    && hovered.as_deref() != Some(&n.rel)
                    && !neighbours.contains(&n.rel)
                    && n.rel != layout.focus;
                GraphNode {
                    node: n.clone(),
                    x: px,
                    y: py,
                    opacity: o * if faded { 0.28 } else { 1. },
                    hover,
                    focus_name: layout.focus.clone(),
                }
                .into_element()
            })
            .collect();

        let back = move || {
            let mut history = gs;
            let mut g = history.write();
            if let Some(prev) = g.history.pop() {
                g.focus = prev;
            }
        };

        let hover_card = hovered
            .as_ref()
            .and_then(|rel| layout.nodes.iter().find(|n| &n.rel == rel))
            .map(|n| {
                let (x, y, _) = place(&n.rel);
                let (px, py) = to_screen(x, y);
                let index = app.index.read();
                let excerpt = index
                    .doc(&n.rel)
                    .map(|d| d.excerpt.clone())
                    .unwrap_or_default();
                let edited = index
                    .doc(&n.rel)
                    .map(|d| ago(d.modified))
                    .unwrap_or_default();
                drop(index);
                let left = if px + 280. > aw {
                    px - 280.
                } else {
                    px + n.w / 2. + 10.
                };
                let top = if py + 140. > ah { py - 140. } else { py - 10. };
                rect()
                    .position(Position::new_absolute().left(left).top(top))
                    .width(Size::px(260.))
                    .layer(Layer::Relative(5))
                    .padding((10., 12.))
                    .spacing(6.)
                    .background(c().surface_overlay)
                    .border(Border::new().fill(c().border_subtle).width(1.))
                    .corner_radius(6.)
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .child(
                        rect()
                            .horizontal()
                            .spacing(8.)
                            .cross_align(Alignment::Center)
                            .child(
                                rect()
                                    .width(Size::px(2.))
                                    .height(Size::px(12.))
                                    .background(c().tone(n.tone)),
                            )
                            .child(
                                label()
                                    .text(base_name(&n.rel).to_string())
                                    .font_size(TEXT_SM)
                                    .font_weight(FontWeight::MEDIUM),
                            )
                            .child(crate::tag(if n.dist == 0 {
                                "focus".to_string()
                            } else if n.dist >= 99 {
                                "unreached".to_string()
                            } else {
                                hops(n.dist)
                            })),
                    )
                    .child(
                        label()
                            .width(Size::fill())
                            .text(excerpt)
                            .font_size(TEXT_XS)
                            .line_height(1.5)
                            .color(c().text_muted)
                            .max_lines(4),
                    )
                    .child(
                        rect()
                            .horizontal()
                            .content(Content::Flex)
                            .width(Size::fill())
                            .child(
                                rect().width(Size::flex(1.)).child(
                                    label()
                                        .text(crate::keys::show("click · focus   ⌘click · open"))
                                        .font_size(TEXT_XS)
                                        .color(c().text_faint),
                                ),
                            )
                            .child(
                                label()
                                    .text(edited)
                                    .font_size(TEXT_XS)
                                    .color(c().text_faint),
                            ),
                    )
                    .into_element()
            });

        // keys (spec §9): ←→↑↓ walk the hover between neighbours, ⏎ focuses,
        // ⌘⏎ opens, ⌘[ goes back, 1/2/3 depth, O/B layout
        let order: Vec<(String, f32, f32)> = layout
            .nodes
            .iter()
            .map(|n| (n.rel.clone(), n.x, n.y))
            .collect();
        let focus_now = layout.focus.clone();
        let on_key = move |e: Event<KeyboardEventData>| {
            if *app.view.read() != View::Graph || *app.overlay.read() != crate::app::Overlay::None {
                return;
            }
            let ctrl = crate::ctrl(&e);
            let mut gs = gs;
            let mut hover = hover;
            match (&e.key, ctrl) {
                (Key::Named(NamedKey::Enter), false) => {
                    let h = hover.read().clone();
                    if let Some(rel) = h {
                        let mut g = gs.write();
                        if g.focus != rel {
                            let prev = std::mem::replace(&mut g.focus, rel);
                            g.history.push(prev);
                        }
                    }
                }
                (Key::Named(NamedKey::Enter), true) => {
                    let h = hover.read().clone().unwrap_or(focus_now.clone());
                    app.open(&h);
                }
                (
                    Key::Named(
                        NamedKey::ArrowLeft
                        | NamedKey::ArrowRight
                        | NamedKey::ArrowUp
                        | NamedKey::ArrowDown,
                    ),
                    false,
                ) => {
                    let from = hover.read().clone().unwrap_or(focus_now.clone());
                    let Some(&(_, fx, fy)) = order.iter().find(|(r, _, _)| *r == from) else {
                        return;
                    };
                    let (dx, dy) = match e.key {
                        Key::Named(NamedKey::ArrowLeft) => (-1., 0.),
                        Key::Named(NamedKey::ArrowRight) => (1., 0.),
                        Key::Named(NamedKey::ArrowUp) => (0., -1.),
                        _ => (0., 1.),
                    };
                    // the nearest node roughly in that direction
                    let best = order
                        .iter()
                        .filter(|(r, _, _)| *r != from)
                        .filter_map(|(r, x, y)| {
                            let (vx, vy) = (x - fx, y - fy);
                            let along = vx * dx + vy * dy;
                            (along > 4.).then(|| {
                                (
                                    r.clone(),
                                    (vx * vx + vy * vy).sqrt() + (vx * dy - vy * dx).abs() * 1.5,
                                )
                            })
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((r, _)) = best {
                        hover.set(Some(r));
                    }
                }
                (Key::Character(_), true) if e.code == Code::BracketLeft => {
                    let mut g = gs.write();
                    if let Some(prev) = g.history.pop() {
                        g.focus = prev;
                    }
                }
                (Key::Character(_), false) => match e.code {
                    Code::Digit1 => gs.write().depth = 1,
                    Code::Digit2 => gs.write().depth = 2,
                    Code::Digit3 => gs.write().depth = 3,
                    Code::KeyO => gs.write().branch = false,
                    Code::KeyB => gs.write().branch = true,
                    _ => {}
                },
                _ => {}
            }
        };

        let play = move || {
            let mut playing = playing;
            let now = *playing.read();
            playing.set(!now);
            if now {
                return;
            }
            spawn(async move {
                let mut gs = gs;
                for w in (0..WEEKS).rev() {
                    if !*playing.read() {
                        return;
                    }
                    gs.write().week = w;
                    freya::prelude::timer(Duration::from_millis(170)).await;
                }
                playing.set(false);
            });
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .horizontal()
            .content(Content::Flex)
            .on_global_key_down(on_key)
            .child(left_panel(app, groups, group_tones, unlinked, filter))
            .child(
                rect()
                    .width(Size::flex(1.))
                    .height(Size::fill())
                    .background(under)
                    .overflow(Overflow::Clip)
                    .on_sized(move |e: Event<SizedEventData>| {
                        let a = e.area;
                        let next = (a.min_x(), a.min_y(), a.width(), a.height());
                        if *area.read() != next {
                            area.set(next);
                        }
                    })
                    .children(guide_els)
                    .children(node_els)
                    .maybe_child(hover_card)
                    .child(top_left_pill(layout.focus.clone(), focus_tone, back))
                    .child(top_right_pill(gs))
                    .maybe_child(HISTORY_BAR.then(|| {
                        history_bar(gs, weeks, notes_now, links_now, *playing.read(), play)
                    })),
            )
            .child(inspector(
                app,
                focus_doc,
                layout.focus.clone(),
                focus_tone,
                backlinks,
                back_tones,
                outgoing,
                out_tones,
                mentions,
                (within, layout.depth),
            ))
    }
}

fn fill(color: Color) -> Paint {
    let mut p = Paint::default();
    p.set_anti_alias(true);
    p.set_color(sk::SkColor::from(color));
    p
}

fn line(color: Color, w: f32) -> Paint {
    let mut p = fill(color);
    p.set_style(PaintStyle::Stroke).set_stroke_width(w);
    p
}

fn ago(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return String::new() };
    let m = t.elapsed().unwrap_or_default().as_secs() / 60;
    match m {
        0 => "just now".into(),
        1..=59 => format!("{m}m ago"),
        60..=1439 => format!("{}h ago", m / 60),
        1440..=43199 => format!("{}d ago", m / 1440),
        _ => format!("{}mo ago", m / 43200),
    }
}

#[derive(PartialEq)]
struct GraphNode {
    node: GNode,
    x: f32,
    y: f32,
    opacity: f32,
    hover: State<Option<String>>,
    focus_name: String,
}

impl Component for GraphNode {
    fn render_key(&self) -> DiffKey {
        (&self.node.rel).into()
    }

    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = self.hover;
        let n = &self.node;
        let color = c().tone(n.tone);
        let rel = n.rel.clone();
        let rel_hover = rel.clone();
        let mut gs = app.graph;
        let click = move |_| {
            if app.ctrl_down() {
                app.open(&rel);
                return;
            }
            let mut g = gs.write();
            if g.focus != rel {
                let prev = std::mem::replace(&mut g.focus, rel.clone());
                g.history.push(prev);
                if g.history.len() > 20 {
                    g.history.remove(0);
                }
            }
        };
        let base = rect()
            .position(
                Position::new_absolute()
                    .left(self.x - n.w / 2.)
                    .top(self.y - n.h / 2.),
            )
            .opacity(self.opacity)
            .on_pointer_enter(move |_| hover.set(Some(rel_hover.clone())))
            .on_pointer_leave(move |_| hover.set(None))
            .on_press(click)
            .cursor(CursorIcon::Pointer);
        match n.look {
            Look::Focus => {
                let index = app.index.read();
                let (i, o) = (index.backlinks(&n.rel).len(), index.outgoing(&n.rel).len());
                let edited = index
                    .doc(&n.rel)
                    .map(|d| ago(d.modified))
                    .unwrap_or_default();
                drop(index);
                base.width(Size::px(n.w))
                    .height(Size::px(n.h))
                    .horizontal()
                    .background(c().surface_raised)
                    .border(Border::new().fill(c().border_strong).width(1.))
                    .border(
                        Border::new()
                            .fill(color)
                            .width(1.)
                            .alignment(BorderAlignment::Outer),
                    )
                    .corner_radius(6.)
                    // spec §5: --shadow-2, then a 32px halo in the dim tone.
                    // ⚠️ not dim.with_a(90): with_a scales, that left 4%
                    .shadow(Shadow::new().y(4.).blur(12.).color(c().shadow(1.)))
                    .shadow(Shadow::new().blur(32.).color(c().tone_dim(n.tone)))
                    .overflow(Overflow::Clip)
                    .child(
                        rect()
                            .width(Size::px(2.))
                            .height(Size::fill())
                            .background(color)
                            .shadow(Shadow::new().blur(8.).color(color)),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .padding((8., 10., 8., 12.))
                            .spacing(4.)
                            .child(
                                rect()
                                    .horizontal()
                                    .content(Content::Flex)
                                    .width(Size::fill())
                                    .cross_align(Alignment::Center)
                                    .child(
                                        rect().width(Size::flex(1.)).child(
                                            label()
                                                .text(label_of(&n.rel))
                                                .font_size(TEXT_SM)
                                                .font_weight(FontWeight::MEDIUM)
                                                .max_lines(1),
                                        ),
                                    )
                                    .child(
                                        rect()
                                            .border(Border::new().fill(c().accent).width(1.))
                                            .corner_radius(3.)
                                            .padding((0., 4.))
                                            .child(
                                                label()
                                                    .text("focus")
                                                    .font_size(TEXT_XS)
                                                    .color(c().accent),
                                            ),
                                    ),
                            )
                            .child(
                                label()
                                    .text(format!("{i} in · {o} out · edited {edited}"))
                                    .font_size(TEXT_XS)
                                    .color(c().text_faint),
                            ),
                    )
                    .into_element()
            }
            Look::Pill => base
                .width(Size::px(n.w))
                .height(Size::px(n.h))
                .horizontal()
                .cross_align(Alignment::Center)
                .background(c().surface_raised)
                .border(Border::new().fill(c().border_subtle).width(1.))
                .corner_radius(4.)
                .shadow(Shadow::new().y(1.).blur(2.).color(c().shadow(0.9)))
                .overflow(Overflow::Clip)
                .child(
                    rect()
                        .width(Size::px(2.))
                        .height(Size::fill())
                        .background(color),
                )
                .child(
                    rect()
                        .horizontal()
                        .padding((0., 8.))
                        .spacing(6.)
                        .child(
                            label()
                                .text(label_of(&n.rel))
                                .font_size(TEXT_SM)
                                .color(if n.dist <= 1 {
                                    c().text_primary
                                } else {
                                    c().text_muted
                                })
                                .max_lines(1),
                        )
                        .child(
                            label()
                                .text(n.degree.to_string())
                                .font_size(TEXT_XS)
                                .color(c().text_faint),
                        ),
                )
                .into_element(),
            Look::Dot => {
                // its name above it in the top half, below in the bottom
                // (spec §4.7); hidden where it would cover a pill, until hovered
                let named = n.named || hover.read().as_deref() == Some(n.rel.as_str());
                let name = label_of(&n.rel);
                let lw = name.chars().count() as f32 * 6.;
                let top = if n.y < CY { -14. } else { n.h };
                base.width(Size::px(n.w))
                    .height(Size::px(n.h))
                    .center()
                    .child(
                        rect()
                            .width(Size::px(7.))
                            .height(Size::px(7.))
                            .corner_radius(999.)
                            .background(c().canvas_bg)
                            .border(Border::new().fill(color).width(1.5)),
                    )
                    .maybe_child(named.then(|| {
                        rect()
                            .position(Position::new_absolute().left((n.w - lw) / 2.).top(top))
                            .width(Size::px(lw))
                            .height(Size::px(14.))
                            .center()
                            .child(
                                label()
                                    .text(name)
                                    .font_size(TEXT_XS)
                                    .color(c().text_faint)
                                    .max_lines(1),
                            )
                    }))
                    .into_element()
            }
        }
    }
}

fn left_panel(
    app: App,
    groups: Vec<(String, usize)>,
    tones: Vec<Tone>,
    unlinked: Vec<(String, Option<SystemTime>)>,
    filter: State<String>,
) -> impl IntoElement {
    let gs = app.graph;
    let hidden = gs.read().hidden.clone();
    let root = tilde(&app.root(), std::env::home_dir().as_deref());
    let n_unlinked = unlinked.len();
    rect()
        .width(Size::px(232.))
        .height(Size::fill())
        .background(c().surface_panel)
        .border(Border::new().fill(c().border_subtle).width(BorderWidth {
            right: 1.,
            ..Default::default()
        }))
        .padding((14., 12.))
        .spacing(10.)
        .content(Content::Flex)
        .child(
            rect()
                .spacing(2.)
                .child(
                    label()
                        .text("graph")
                        .font_size(TEXT_SM)
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(
                    label()
                        .text(format!("vault · {root}"))
                        .font_size(TEXT_XS)
                        .color(c().text_faint)
                        .max_lines(1),
                ),
        )
        .child(
            rect()
                .width(Size::fill())
                .horizontal()
                .cross_align(Alignment::Center)
                .content(Content::Flex)
                .spacing(6.)
                .padding((6., 8.))
                .background(c().surface_inset)
                .border(Border::new().fill(c().border_subtle).width(1.))
                .corner_radius(4.)
                .child(label().text("⌕").color(c().text_faint))
                .child(
                    rect().width(Size::flex(1.)).child(
                        Field::new(filter, |_| {}, |_| {})
                            .placeholder("filter notes…")
                            .quiet(),
                    ),
                )
                .child(crate::tag("/")),
        )
        .child(
            label()
                .margin((6., 0., 0., 0.))
                .text("GROUPS")
                .font_size(TEXT_XS)
                .letter_spacing(1.)
                .color(c().text_faint),
        )
        .child(
            rect()
                .width(Size::fill())
                .spacing(2.)
                .children(groups.into_iter().zip(tones).map(|((g, n), tone)| {
                    let shown = !hidden.contains(&g);
                    let name = if g.is_empty() {
                        "vault".to_string()
                    } else {
                        g.trim_end_matches('/').to_string()
                    };
                    let g2 = g.clone();
                    let mut gs = gs;
                    rect()
                        .width(Size::fill())
                        .height(Size::px(28.))
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .spacing(8.)
                        .on_press(move |_| {
                            let mut st = gs.write();
                            if !st.hidden.remove(&g2) {
                                st.hidden.insert(g2.clone());
                            }
                        })
                        .child(
                            rect()
                                .width(Size::px(2.))
                                .height(Size::px(14.))
                                .background(c().tone(tone)),
                        )
                        .child(rect().width(Size::flex(1.)).child(
                            label().text(name).font_size(TEXT_SM).color(if shown {
                                c().text_primary
                            } else {
                                c().text_faint
                            }),
                        ))
                        .child(
                            label()
                                .text(n.to_string())
                                .font_size(TEXT_XS)
                                .color(c().text_faint),
                        )
                        .child(toggle(shown))
                        .into_element()
                })),
        )
        .child(
            label()
                .margin((10., 0., 0., 0.))
                .text(format!("UNLINKED · {n_unlinked}"))
                .font_size(TEXT_XS)
                .letter_spacing(1.)
                .color(c().text_faint),
        )
        .child(
            rect()
                .width(Size::fill())
                .spacing(2.)
                .children(unlinked.into_iter().map(|(rel, t)| {
                    let r = rel.clone();
                    rect()
                        .width(Size::fill())
                        .height(Size::px(26.))
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .spacing(8.)
                        .on_press(move |_| app.open(&r))
                        .child(label().text("○").font_size(TEXT_XS).color(c().text_link))
                        .child(
                            rect().width(Size::flex(1.)).child(
                                label()
                                    .text(base_name(&rel).to_string())
                                    .font_size(TEXT_SM)
                                    .color(c().text_muted)
                                    .max_lines(1),
                            ),
                        )
                        .child(
                            label()
                                .text(ago(t))
                                .font_size(TEXT_XS)
                                .color(c().text_faint),
                        )
                        .into_element()
                })),
        )
        .child(
            label()
                .width(Size::fill())
                .text("notes with no links in or out. they stay off the graph until linked.")
                .font_size(TEXT_XS)
                .line_height(1.5)
                .color(c().text_faint),
        )
        .child(rect().width(Size::fill()).height(Size::flex(1.)))
        .child(
            rect()
                .width(Size::fill())
                .padding((12., 0., 0., 0.))
                .spacing(8.)
                .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                    top: 1.,
                    ..Default::default()
                }))
                .font_size(TEXT_XS)
                .color(c().text_muted)
                // the spec's legend draws each mark; the ring is solid here,
                // where the guides are dashed
                .child(legend_row(
                    rect()
                        .width(Size::px(18.))
                        .height(Size::px(10.))
                        .corner_radius(999.)
                        .border(Border::new().fill(c().border_strong).width(1.)),
                    "ring = hops from focus",
                ))
                .child(legend_row(
                    rect()
                        .width(Size::px(5.))
                        .height(Size::px(5.))
                        .corner_radius(999.)
                        .background(c().text_faint),
                    "dot = beyond depth",
                ))
                .child(legend_row(
                    rect()
                        .width(Size::px(2.))
                        .height(Size::px(12.))
                        .background(c().doc[0]),
                    "spine = group color",
                )),
        )
}

/// A legend line: the mark, centred in an 18px column, then what it means.
fn legend_row(mark: impl IntoElement, text: &'static str) -> impl IntoElement {
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(8.)
        .child(rect().width(Size::px(18.)).center().child(mark))
        .child(label().text(text))
}

fn toggle(on: bool) -> impl IntoElement {
    rect()
        .width(Size::px(24.))
        .height(Size::px(13.))
        .corner_radius(999.)
        .background(if on { c().accent } else { c().surface_inset })
        .border(
            Border::new()
                .fill(if on { c().accent } else { c().border_strong })
                .width(1.),
        )
        .padding(2.)
        .main_align(if on { Alignment::End } else { Alignment::Start })
        .horizontal()
        .child(
            rect()
                .width(Size::px(9.))
                .height(Size::px(9.))
                .corner_radius(999.)
                .background(if on { c().on_accent } else { c().text_faint }),
        )
}

fn top_left_pill(focus: String, tone: Tone, back: impl Fn() + 'static) -> impl IntoElement {
    rect()
        .position(Position::new_absolute().left(14.).top(10.))
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(8.)
        .padding((5., 10.))
        .background(c().surface_panel)
        .border(Border::new().fill(c().border_subtle).width(1.))
        .corner_radius(4.)
        .font_size(TEXT_SM)
        .child(
            label()
                .text("‹")
                .color(c().text_muted)
                .on_press(move |_| back()),
        )
        .child(label().text("graph /").color(c().text_faint))
        .child(
            rect()
                .width(Size::px(2.))
                .height(Size::px(12.))
                .background(c().tone(tone)),
        )
        .child(
            label()
                .text(base_name(&focus).to_string())
                .font_weight(FontWeight::MEDIUM),
        )
}

fn seg(text: &str, on: bool, f: impl FnMut(Event<PressEventData>) + 'static) -> Element {
    rect()
        .padding((2., 8.))
        .corner_radius(3.)
        .background(if on {
            c().surface_raised
        } else {
            Color::TRANSPARENT
        })
        .on_press(f)
        .child(
            label()
                .text(text.to_string())
                .font_size(TEXT_SM)
                .color(if on { c().text_primary } else { c().text_muted }),
        )
        .into_element()
}

fn top_right_pill(gs: State<GraphState>) -> impl IntoElement {
    let st = gs.read().clone();
    let mut g1 = gs;
    let mut g2 = gs;
    rect()
        .position(Position::new_absolute().right(14.).top(10.))
        .horizontal()
        .cross_align(Alignment::Center)
        .spacing(10.)
        .padding((4., 6.))
        .background(c().surface_panel)
        .border(Border::new().fill(c().border_subtle).width(1.))
        .corner_radius(4.)
        .child(
            rect()
                .horizontal()
                .padding(1.)
                .background(c().surface_inset)
                .corner_radius(3.)
                .child(seg("◎ orbit", !st.branch, move |_| {
                    g1.write().branch = false
                }))
                .child(seg("⊢ branch", st.branch, move |_| {
                    g2.write().branch = true
                })),
        )
        .child(
            label()
                .text("depth")
                .font_size(TEXT_XS)
                .color(c().text_faint),
        )
        .child(
            rect()
                .horizontal()
                .padding(1.)
                .background(c().surface_inset)
                .corner_radius(3.)
                .children((1..=3).map(|d| {
                    let mut g = gs;
                    seg(&d.to_string(), st.depth == d, move |_| g.write().depth = d)
                })),
        )
        .child(
            rect()
                .horizontal()
                .spacing(6.)
                .cross_align(Alignment::Center)
                .child(
                    label()
                        .text("⌕ find node")
                        .font_size(TEXT_SM)
                        .color(c().text_muted),
                )
                .child(crate::tag("⌘K")),
        )
}

fn history_bar(
    gs: State<GraphState>,
    weeks: Vec<usize>,
    notes: usize,
    links: usize,
    playing: bool,
    play: impl Fn() + 'static,
) -> impl IntoElement {
    let current = gs.read().week;
    let max = weeks.iter().copied().max().unwrap_or(1).max(1);
    // ⚠️ UTC days: near midnight the date can be a day off the local one
    let today = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as i64)
        .unwrap_or(0);
    let (label_text, months) = history_words(today, current, weeks.len());
    rect()
        .position(Position::new_absolute().left(14.).right(14.).bottom(12.))
        .horizontal()
        .cross_align(Alignment::Center)
        .content(Content::Flex)
        .spacing(14.)
        .padding((10., 14.))
        .background(c().surface_panel)
        .border(Border::new().fill(c().border_subtle).width(1.))
        .corner_radius(6.)
        .shadow(Shadow::new().y(1.).blur(2.).color(c().shadow(0.9)))
        .child(
            rect()
                .width(Size::px(26.))
                .height(Size::px(26.))
                .center()
                .corner_radius(4.)
                .border(Border::new().fill(c().border_strong).width(1.))
                .on_press(move |_| play())
                .child(
                    label()
                        .text(if playing { "❚❚" } else { "▶" })
                        .font_size(TEXT_XS),
                ),
        )
        .child(
            rect()
                .spacing(2.)
                .child(
                    label()
                        .text("VAULT HISTORY")
                        .font_size(TEXT_XS)
                        .letter_spacing(1.)
                        .color(c().text_faint),
                )
                .child(label().text(label_text).font_size(TEXT_SM)),
        )
        .child(
            rect()
                .width(Size::flex(1.))
                .spacing(3.)
                .child(bars_row(gs, &weeks, current, max))
                // the month under the bar where it starts, as the bars share space
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(12.))
                        .horizontal()
                        .content(Content::Flex)
                        .spacing(2.)
                        .children(months.into_iter().map(|m| {
                            rect()
                                .width(Size::flex(1.))
                                .maybe_child(m.map(|m| {
                                    label().text(m).font_size(TEXT_XS).color(c().text_faint)
                                }))
                                .into_element()
                        })),
                ),
        )
        .child(
            rect()
                .spacing(2.)
                .cross_align(Alignment::End)
                .child(
                    label()
                        .text(format!("{notes} notes · {links} links"))
                        .font_size(TEXT_SM),
                )
                .child(
                    label()
                        .text(if weeks.last().copied().unwrap_or(0) > 0 {
                            format!("+{} files this week", weeks[WEEKS - 1])
                        } else {
                            "no new notes this week".into()
                        })
                        .font_size(TEXT_XS)
                        .color(c().text_faint),
                ),
        )
}

/// The 26 weekly bars (spec §7): past weeks strong, the selected one accent
/// with a glow, later ones raised; a click picks that week.
fn bars_row(gs: State<GraphState>, weeks: &[usize], current: usize, max: usize) -> Element {
    rect()
        .width(Size::fill())
        .height(Size::px(34.))
        .horizontal()
        // ⚠️ without Flex content, flex(1) bars don't share the row
        .content(Content::Flex)
        .cross_align(Alignment::End)
        .spacing(2.)
        .children(weeks.iter().enumerate().map(|(i, &n)| {
            let back = WEEKS - 1 - i;
            let selected = back == current;
            let future = back < current;
            let mut g = gs;
            rect()
                .width(Size::flex(1.))
                .height(Size::px((4 + n * 26 / max) as f32 + 2.))
                .corner_radius(1.)
                .background(if selected {
                    c().accent
                } else if future {
                    c().surface_raised
                } else {
                    c().border_strong
                })
                .shadow(if selected {
                    Shadow::new().blur(8.).color(c().accent)
                } else {
                    Shadow::new()
                })
                .on_press(move |_| g.write().week = back)
                .into_element()
        }))
        .into_element()
}

#[allow(clippy::too_many_arguments)]
fn inspector(
    app: App,
    doc: Option<crate::index::DocEntry>,
    focus: String,
    tone: Tone,
    backlinks: Vec<crate::index::Link>,
    back_tones: Vec<Tone>,
    outgoing: Vec<crate::index::Link>,
    out_tones: Vec<Tone>,
    mentions: Vec<crate::index::Mention>,
    // notes on the rings, and how many rings
    (within, depth): (usize, usize),
) -> impl IntoElement {
    let color = c().tone(tone);
    let excerpt = doc.as_ref().map(|d| d.excerpt.clone()).unwrap_or_default();
    let group = focus
        .split_once('/')
        .map(|(g, _)| g.to_string())
        .unwrap_or_else(|| "vault".into());
    let gs = app.graph;
    let refocus = move |rel: String| {
        let mut gs = gs;
        let mut g = gs.write();
        if g.focus != rel {
            let prev = std::mem::replace(&mut g.focus, rel);
            g.history.push(prev);
        }
    };
    let (n_in, n_out) = (backlinks.len(), outgoing.len());
    let focus_open = focus.clone();
    let focus_pin = focus.clone();
    rect()
        .width(Size::px(320.))
        .height(Size::fill())
        .horizontal()
        .background(c().surface_panel)
        .child(
            rect()
                .width(Size::px(2.))
                .height(Size::fill())
                .background(color)
                .shadow(Shadow::new().blur(10.).color(color)),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .content(Content::Flex)
                .child(
                    rect()
                        .width(Size::fill())
                        .height(Size::px(34.))
                        .horizontal()
                        .content(Content::Flex)
                        .cross_align(Alignment::Center)
                        .padding((0., 14.))
                        .spacing(6.)
                        .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                            bottom: 1.,
                            ..Default::default()
                        }))
                        .font_size(TEXT_SM)
                        .child(label().text("inspector /").color(c().text_faint))
                        .child(
                            rect().width(Size::flex(1.)).child(
                                label()
                                    .text(base_name(&focus).to_string())
                                    .font_weight(FontWeight::MEDIUM)
                                    .max_lines(1),
                            ),
                        )
                        .child(crate::tag(group)),
                )
                .child(
                    ScrollView::new()
                        .width(Size::fill())
                        .height(Size::flex(1.))
                        .child(
                            rect()
                                .width(Size::fill())
                                .padding((12., 14.))
                                .spacing(10.)
                                .child(
                                    rect()
                                        .width(Size::fill())
                                        .padding((10., 12.))
                                        .background(c().surface_inset)
                                        .border(Border::new().fill(c().border_subtle).width(1.))
                                        .corner_radius(4.)
                                        .child(
                                            label()
                                                .width(Size::fill())
                                                .text(excerpt)
                                                .font_size(TEXT_XS)
                                                .line_height(1.6)
                                                .color(c().text_muted),
                                        ),
                                )
                                .child(
                                    rect()
                                        .width(Size::fill())
                                        .horizontal()
                                        .content(Content::Flex)
                                        .child(
                                            rect().width(Size::flex(1.)).child(
                                                label()
                                                    .text(format!("{n_in} in   {n_out} out"))
                                                    .font_size(TEXT_XS),
                                            ),
                                        )
                                        .child(
                                            label()
                                                .text(format!("{within} within {}", hops(depth)))
                                                .font_size(TEXT_XS)
                                                .color(c().text_faint),
                                        ),
                                )
                                .child(section(&format!("BACKLINKS · {n_in}")))
                                .children(backlinks.into_iter().zip(back_tones).map(|(l, t)| {
                                    let from = l.from.clone();
                                    rect()
                                        .width(Size::fill())
                                        .horizontal()
                                        .spacing(8.)
                                        .padding((4., 0.))
                                        .on_press(move |_| refocus(from.clone()))
                                        .child(
                                            rect()
                                                .width(Size::px(2.))
                                                .height(Size::px(30.))
                                                .background(c().tone(t)),
                                        )
                                        .child(
                                            rect()
                                                .width(Size::fill())
                                                .spacing(3.)
                                                .child(
                                                    label()
                                                        .text(base_name(&l.from).to_string())
                                                        .font_size(TEXT_SM),
                                                )
                                                .child(
                                                    paragraph()
                                                        .max_lines(2)
                                                        .font_size(TEXT_XS)
                                                        .span(
                                                            Span::new(format!(
                                                                "…{}",
                                                                l.before.trim_start()
                                                            ))
                                                            .color(c().text_muted),
                                                        )
                                                        .span(
                                                            Span::new(format!(
                                                                "[[{}]]",
                                                                base_name(&l.to)
                                                                    .trim_end_matches(".md")
                                                            ))
                                                            .color(c().text_link),
                                                        )
                                                        .span(
                                                            Span::new(l.after.clone())
                                                                .color(c().text_muted),
                                                        ),
                                                ),
                                        )
                                        .into_element()
                                }))
                                .child(section(&format!("OUTGOING · {n_out}")))
                                .child(
                                    rect()
                                        .width(Size::fill())
                                        .horizontal()
                                        .content(Content::Wrap {
                                            wrap_spacing: Some(6.),
                                        })
                                        .spacing(6.)
                                        .children(outgoing.into_iter().zip(out_tones).map(
                                            |(l, t)| {
                                                let to = l.to.clone();
                                                rect()
                                                    .margin((0., 0., 6., 0.))
                                                    .horizontal()
                                                    .border(
                                                        Border::new()
                                                            .fill(c().border_subtle)
                                                            .width(1.),
                                                    )
                                                    .corner_radius(3.)
                                                    .overflow(Overflow::Clip)
                                                    .on_press(move |_| refocus(to.clone()))
                                                    .child(
                                                        rect()
                                                            .width(Size::px(2.))
                                                            .height(Size::px(20.))
                                                            .background(c().tone(t)),
                                                    )
                                                    .child(
                                                        rect().padding((3., 8.)).child(
                                                            label()
                                                                .text(base_name(&l.to).to_string())
                                                                .font_size(TEXT_XS),
                                                        ),
                                                    )
                                                    .into_element()
                                            },
                                        )),
                                )
                                .child(section(&format!(
                                    "MENTIONED, NOT LINKED · {}",
                                    mentions.len()
                                )))
                                .children(mentions.into_iter().map(|m| {
                                    let target = m.target.clone();
                                    rect()
                                        .width(Size::fill())
                                        .horizontal()
                                        .content(Content::Flex)
                                        .cross_align(Alignment::Center)
                                        .padding((8., 10.))
                                        .border(Border::new().fill(c().border_strong).width(1.))
                                        .corner_radius(4.)
                                        .child(
                                            rect()
                                                .width(Size::flex(1.))
                                                .spacing(3.)
                                                .child(
                                                    paragraph()
                                                        .max_lines(2)
                                                        .font_size(TEXT_XS)
                                                        .span(
                                                            Span::new(m.before.clone())
                                                                .color(c().text_muted),
                                                        )
                                                        .span(
                                                            Span::new(m.matched.clone())
                                                                .color(c().text_primary)
                                                                .font_weight(FontWeight::BOLD),
                                                        )
                                                        .span(
                                                            Span::new(m.after.clone())
                                                                .color(c().text_muted),
                                                        ),
                                                )
                                                .child(
                                                    label()
                                                        .text(format!("→ {}", base_name(&m.target)))
                                                        .font_size(TEXT_XS)
                                                        .color(c().text_link),
                                                ),
                                        )
                                        .child(
                                            rect()
                                                .padding((2., 8.))
                                                .border(
                                                    Border::new().fill(c().border_strong).width(1.),
                                                )
                                                .corner_radius(3.)
                                                .on_press(move |_| {
                                                    link_mention(
                                                        app, &m.source, &m.matched, &target,
                                                    )
                                                })
                                                .child(label().text("link").font_size(TEXT_XS)),
                                        )
                                        .into_element()
                                })),
                        ),
                )
                .child(
                    rect()
                        .width(Size::fill())
                        .horizontal()
                        .main_align(Alignment::End)
                        .spacing(8.)
                        .padding((12., 14.))
                        .border(Border::new().fill(c().border_subtle).width(BorderWidth {
                            top: 1.,
                            ..Default::default()
                        }))
                        .child(
                            rect()
                                .padding((6., 12.))
                                .border(Border::new().fill(c().border_strong).width(1.))
                                .corner_radius(4.)
                                .on_press(move |_| pin_to_canvas(app, &focus_pin))
                                .child(label().text("pin to canvas").font_size(TEXT_SM)),
                        )
                        .child(
                            rect()
                                .padding((6., 12.))
                                .background(c().accent)
                                .corner_radius(4.)
                                .on_press(move |_| {
                                    app.open(&focus_open);
                                    let mut v = app.view;
                                    v.set(View::Split);
                                })
                                .child(
                                    label()
                                        .text("open note ↵")
                                        .font_size(TEXT_SM)
                                        .font_weight(FontWeight::MEDIUM)
                                        .color(c().on_accent),
                                ),
                        ),
                ),
        )
}

fn section(t: &str) -> impl IntoElement {
    label()
        .margin((6., 0., 0., 0.))
        .text(t.to_string())
        .font_size(TEXT_XS)
        .letter_spacing(1.)
        .color(c().text_faint)
}

/// "link" on a mention: the matched words become `[[target|words]]` in the
/// note, written through the stamp check like any other edit.
fn link_mention(app: App, source: &str, matched: &str, target: &str) {
    let root = app.root();
    let path = root.join(source);
    let stamp = synapso_core::vault::stamp(&path);
    let Ok(text) = synapso_core::vault::read(&path) else {
        return;
    };
    let stem = crate::index::doc_key(base_name(target));
    let Some(at) = text.find(matched) else { return };
    let new = format!(
        "{}[[{}|{}]]{}",
        &text[..at],
        stem,
        matched,
        &text[at + matched.len()..]
    );
    match synapso_core::vault::write_checked(&path, &new, stamp) {
        Ok(_) => {
            app.reindex();
            app.toast(format!("linked → {}", base_name(target)), "", None);
        }
        Err(e) => app.toast("not linked", e.to_string(), None),
    }
}

/// "pin to canvas": the focus lands as a card on the open board.
fn pin_to_canvas(app: App, rel: &str) {
    let board = app.canvas.read().clone();
    if board.is_empty() {
        return;
    }
    crate::board::flush_board();
    let path = app.root().join(&board);
    let mut scene = synapso_core::board::Scene::load(path);
    if scene.broken.is_some() {
        return;
    }
    let at = synapso_core::board::corner(&scene.canvas.nodes).unwrap_or_default();
    scene.attach_ref(None, kurbo::Point::new(at.x, at.y + 40.), rel, None);
    if scene.save().is_ok() {
        let mut rev = app.board_rev;
        *rev.write() += 1;
        app.toast(format!("pinned to {}", base_name(&board)), "", None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vault_path_starts_at_home() {
        use std::path::Path;
        let home = Some(Path::new("/Users/me"));
        // joined, so the separators are the platform's (CI runs Windows)
        let sled = Path::new("/Users/me").join("work").join("sled");
        let want = Path::new("~").join("work").join("sled");
        assert_eq!(tilde(&sled, home), want.display().to_string());
        assert_eq!(tilde(Path::new("/Users/me"), home), "~");
        assert_eq!(tilde(Path::new("/Users/meg/x"), home), "/Users/meg/x");
        assert_eq!(tilde(Path::new("/srv/vault"), None), "/srv/vault");
    }

    fn node(look: Look, x: f32, y: f32, w: f32, h: f32) -> GNode {
        GNode {
            rel: "a/sync-2026-09-12.md".into(),
            group: "a/".into(),
            tone: Tone::Neutral,
            look,
            dist: 3,
            degree: 1,
            x,
            y,
            w,
            h,
            canvas: false,
            named: false,
        }
    }

    #[test]
    fn a_beyond_dot_is_named_only_where_its_name_covers_nothing() {
        // top half: the name goes above the dot, bottom half below
        let up = node(Look::Dot, 300., 100., 12., 12.);
        assert_eq!(dot_label_at(&up, 1.), (300., 87.));
        let down = node(Look::Dot, 300., 600., 12., 12.);
        assert_eq!(dot_label_at(&down, 1.), (300., 613.));
        // alone it fits; a pill sitting where the name would go hides it
        assert!(dot_label_fits(&up, std::slice::from_ref(&up), 1.));
        let pill = node(Look::Pill, 320., 84., 120., 24.);
        assert!(!dot_label_fits(&up, &[up.clone(), pill], 1.));
        // another dot there doesn't count, and a pill clear of it neither
        let dot = node(Look::Dot, 320., 84., 12., 12.);
        let far = node(Look::Pill, 600., 84., 120., 24.);
        assert!(dot_label_fits(&up, &[up.clone(), dot, far], 1.));
    }

    #[test]
    fn the_history_bar_reads_dates_and_months() {
        // 2026-09-30 is day 20_726
        assert_eq!(month_day(20_726), (9, 30));
        assert_eq!(month_day(0), (1, 1));
        let (now, months) = history_words(20_726, 0, 26);
        assert_eq!(now, "today · sep 30");
        // 2026-06-03 is 17 weeks before 2026-09-30
        assert_eq!(history_words(20_726, 17, 26).0, "week of jun 03");
        // the last bar is this week; the first bar and each new month say
        // which month they're in (the oldest bar is 2026-04-08)
        let named: Vec<&str> = months.iter().flatten().copied().collect();
        assert_eq!(named, ["apr", "may", "jun", "jul", "aug", "sep"]);
        assert_eq!(months[0], Some("apr"));
        assert_eq!(months[1], None);
        // 2026-10-16: the first bar is apr 24 and may starts on the next one,
        // so apr would sit on may's name and is left off
        let (_, months) = history_words(20_742, 0, 26);
        assert_eq!((months[0], months[1]), (None, Some("may")));
    }

    #[test]
    fn every_ring_says_how_many_hops_out_it_is() {
        let orbit = guide_labels(2, false, 4);
        let names: Vec<&str> = orbit.iter().map(|l| l.2.as_str()).collect();
        assert_eq!(names, ["1 hop", "2 hops", "beyond · 4"]);
        // over the top of each ring: ry = 0.7·rx, rx = 380·(k/L)^0.7
        let ry1 = 380. * 0.5f32.powf(0.7) * 0.7;
        assert!((orbit[0].1 - (CY - ry1 - 8.)).abs() < 0.01);
        assert_eq!((orbit[1].0, orbit[1].1), (CX, CY - 380. * 0.7 - 8.));
        assert_eq!(orbit[2].1, CY - 330. - 8.);
        // no dots, no beyond label
        assert_eq!(guide_labels(1, false, 0).len(), 1);
        // branch: one per column, both sides
        let branch = guide_labels(2, true, 0);
        assert_eq!(branch.len(), 4);
        assert!(branch.iter().all(|l| l.1 == 28.));
        // and over both beyond columns when there are dots out there
        let branch = guide_labels(2, true, 3);
        let beyond: Vec<_> = branch.iter().filter(|l| l.2 == "beyond").collect();
        assert_eq!(beyond.len(), 2);
        assert_eq!(
            (beyond[0].0, beyond[1].0),
            (CX - BEYOND_COL, CX + BEYOND_COL)
        );
        assert!(beyond.iter().all(|l| l.1 == 28.));
    }
}
