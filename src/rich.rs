//! The sheet's inline reading beyond the core's: `***bold italic***`,
//! `==highlight==`, `<u>underline</u>`, footnote refs `[^1]`, and wiki links
//! told apart from external ones (the markdown guide in the design). The core
//! still reads everything else; this only claims what it doesn't know.
//!
//! It also paints what a text style can't: the chip behind inline code, the
//! highlight's wash and a wiki link's dashed underline, drawn behind the
//! paragraph from the rects its own layout measured.
use std::ops::Range;

use freya::prelude::*;
use freya_engine::prelude as sk;
use synapso_core::inline;

use crate::theme::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Rich {
    Marker,
    Bold,
    Italic,
    BoldItalic,
    Strike,
    Underline,
    Code,
    Highlight,
    /// `[[note]]`: the app's own link.
    Wiki,
    /// `[label](url)`.
    Link,
    /// `[^1]`, the number.
    FootRef,
}

/// Paint behind a run, in UTF-16 units of the paragraph's text.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Deco {
    Chip,
    Wash,
    Dashed,
}

/// Byte ranges over one line, like [`inline::spans`], in the richer vocabulary.
pub fn spans(line: &str) -> Vec<(Range<usize>, Rich)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut plain = 0; // start of the run the core hasn't read yet
    let mut i = 0;
    let flush = |out: &mut Vec<(Range<usize>, Rich)>, from: usize, to: usize| {
        if to > from {
            out.extend(
                core(&line[from..to])
                    .into_iter()
                    .map(|(r, s)| (r.start + from..r.end + from, s)),
            );
        }
    };
    'scan: while i < b.len() {
        // code is the core's to read: step over it whole (``x`` before `x`,
        // as the core does), so `a == b` in code stays code and the `**`
        // around `**bold `code`**` reach the core in one piece
        for tick in ["``", "`"] {
            if line[i..].starts_with(tick) {
                let from = i + tick.len();
                if let Some(end) = line[from..].find(tick).map(|e| e + from)
                    && end > from
                {
                    i = end + tick.len();
                    continue 'scan;
                }
            }
        }
        // (open, close, style)
        let pairs: [(&str, &str, Rich); 4] = [
            ("***", "***", Rich::BoldItalic),
            ("==", "==", Rich::Highlight),
            ("<u>", "</u>", Rich::Underline),
            ("[^", "]", Rich::FootRef),
        ];
        let mut hit = None;
        for (open, close, style) in pairs {
            if line[i..].starts_with(open) {
                let from = i + open.len();
                if let Some(end) = line[from..].find(close).map(|e| e + from) {
                    // a footnote ref is a short name, not a bracketed sentence
                    let ok = end > from
                        && (style != Rich::FootRef
                            || line[from..end]
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '-'));
                    if ok {
                        hit = Some((open.len(), end, close.len(), style));
                        break;
                    }
                }
            }
        }
        match hit {
            Some((open, end, close, style)) => {
                flush(&mut out, plain, i);
                out.push((i..i + open, Rich::Marker));
                out.push((i + open..end, style));
                out.push((end..end + close, Rich::Marker));
                i = end + close;
                plain = i;
            }
            None => i += line[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
    flush(&mut out, plain, b.len());
    out
}

/// The core's reading, with `[[…]]` split into folding brackets and a wiki link.
fn core(line: &str) -> Vec<(Range<usize>, Rich)> {
    let mut out = Vec::new();
    let raw = inline::spans(line);
    for (k, (r, s)) in raw.iter().enumerate() {
        let piece = &line[r.clone()];
        let rich = match s {
            inline::Style::Marker => Rich::Marker,
            inline::Style::Bold => Rich::Bold,
            inline::Style::Italic => Rich::Italic,
            inline::Style::Strike => Rich::Strike,
            inline::Style::Code => Rich::Code,
            inline::Style::Link => {
                if piece.starts_with("[[") && piece.ends_with("]]") && piece.len() >= 4 {
                    out.push((r.start..r.start + 2, Rich::Marker));
                    out.push((r.start + 2..r.end - 2, Rich::Wiki));
                    out.push((r.end - 2..r.end, Rich::Marker));
                    continue;
                }
                // `[[note|label]]`: the marker before it opened with `[[`
                let wiki = k > 0 && line[raw[k - 1].0.clone()].starts_with("[[");
                if wiki { Rich::Wiki } else { Rich::Link }
            }
        };
        out.push((r.clone(), rich));
    }
    out
}

/// The line with its markers taken out — what a reader sees, for wrapping.
pub fn shown(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut at = 0;
    for (range, style) in spans(body) {
        if style == Rich::Marker {
            out.push_str(&body[at..range.start]);
            at = range.end;
        }
    }
    out.push_str(&body[at..]);
    out
}

/// Paints the decorations behind a paragraph whose layout `holder` receives.
pub fn painter(holder: ParagraphHolder, decos: Vec<(Range<usize>, Deco)>) -> RenderCallback {
    RenderCallback::new(move |ctx| {
        let held = holder.0.borrow();
        let Some(inner) = held.as_ref() else { return };
        let scale = inner.scale_factor as f32;
        let p = c();
        for (range, deco) in &decos {
            let boxes = inner.paragraph.get_rects_for_range(
                range.clone(),
                sk::RectHeightStyle::Tight,
                sk::RectWidthStyle::Tight,
            );
            for b in boxes {
                let r = b.rect;
                let r = sk::SkRect::from_ltrb(
                    r.left / scale,
                    r.top / scale,
                    r.right / scale,
                    r.bottom / scale,
                );
                match deco {
                    Deco::Chip => {
                        let chip = sk::SkRRect::new_rect_xy(r.with_outset((3., 1.)), 3., 3.);
                        ctx.canvas.draw_rrect(chip, &fill(p.surface_inset));
                        ctx.canvas
                            .draw_rrect(chip, &line(p.border_subtle, 1., false));
                    }
                    Deco::Wash => {
                        ctx.canvas.draw_rrect(
                            sk::SkRRect::new_rect_xy(r.with_outset((2., 0.)), 2., 2.),
                            &fill(p.doc_dim[1]),
                        );
                    }
                    Deco::Dashed => {
                        let y = r.bottom + 0.5;
                        ctx.canvas
                            .draw_line((r.left, y), (r.right, y), &line(p.accent, 1., true));
                    }
                }
            }
        }
    })
}

fn fill(color: Color) -> sk::Paint {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(sk::SkColor::from(color));
    paint
}

fn line(color: Color, width: f32, dashed: bool) -> sk::Paint {
    let mut paint = fill(color);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(width);
    if dashed {
        paint.set_path_effect(sk::PathEffect::dash(&[3., 2.], 0.));
    }
    paint
}

/// One row of a pipe table, as the sheet lays it out.
#[derive(Clone, PartialEq, Debug)]
pub struct TableRow {
    pub cells: Vec<String>,
    /// Every column's widest cell, in characters, across the whole table.
    pub widths: Vec<usize>,
    pub aligns: Vec<TextAlign>,
    pub header: bool,
    pub last: bool,
}

/// `|:---|:---:|---:|`, the row under the header.
pub fn is_table_rule(t: &str) -> bool {
    let t = t.trim();
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| matches!(c, '|' | ':' | '-' | ' '))
}

pub fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| shown(c.trim())).collect()
}

/// The table the line at `i` belongs to, read off the lines around it.
pub fn table_row(lines: &[String], is_table: impl Fn(usize) -> bool, i: usize) -> TableRow {
    let mut start = i;
    while start > 0 && is_table(start - 1) {
        start -= 1;
    }
    let mut end = i;
    while end + 1 < lines.len() && is_table(end + 1) {
        end += 1;
    }
    let mut widths: Vec<usize> = vec![];
    let mut aligns: Vec<TextAlign> = vec![];
    for line in &lines[start..=end] {
        if is_table_rule(line) {
            aligns = cells_raw(line)
                .iter()
                .map(|c| {
                    let c = c.trim();
                    match (c.starts_with(':'), c.ends_with(':')) {
                        (true, true) => TextAlign::Center,
                        (false, true) => TextAlign::End,
                        _ => TextAlign::Start,
                    }
                })
                .collect();
            continue;
        }
        for (k, cell) in cells(line).iter().enumerate() {
            let n = cell.chars().count();
            if k >= widths.len() {
                widths.push(n);
            } else {
                widths[k] = widths[k].max(n);
            }
        }
    }
    aligns.resize(widths.len(), TextAlign::Start);
    TableRow {
        cells: cells(&lines[i]),
        widths,
        aligns,
        header: i == start,
        last: i == end,
    }
}

fn cells_raw(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styles(line: &str) -> Vec<(&str, Rich)> {
        spans(line)
            .into_iter()
            .map(|(r, s)| (&line[r], s))
            .filter(|(_, s)| *s != Rich::Marker)
            .collect()
    }

    #[test]
    fn reads_the_guide_inline_line() {
        let line =
            "**b**, ***bi***, ==hl==, <u>u</u>, `a == b`, [x](http://e) [[auth-flow]] note[^1].";
        assert_eq!(
            styles(line),
            vec![
                ("b", Rich::Bold),
                ("bi", Rich::BoldItalic),
                ("hl", Rich::Highlight),
                ("u", Rich::Underline),
                ("a == b", Rich::Code),
                ("x", Rich::Link),
                ("auth-flow", Rich::Wiki),
                ("1", Rich::FootRef),
            ]
        );
        assert_eq!(shown(line), "b, bi, hl, u, a == b, x auth-flow note1.");
    }

    #[test]
    fn a_double_backtick_span_holds_a_backtick() {
        assert_eq!(styles("``a ` b``"), vec![("a ` b", Rich::Code)]);
    }

    #[test]
    fn code_inside_bold_leaves_the_bold_whole() {
        let line = "**bold `code`**";
        assert_eq!(styles(line), vec![("bold `code`", Rich::Bold)]);
        assert_eq!(shown(line), "bold `code`");
    }

    #[test]
    fn table_alignment_and_widths() {
        let lines: Vec<String> = [
            "| a | status | ms |",
            "| :--- | :---: | ---: |",
            "| sled | stable | 0.4 |",
        ]
        .map(String::from)
        .to_vec();
        let row = table_row(&lines, |_| true, 2);
        assert_eq!(row.widths, vec![4, 6, 3]);
        assert_eq!(
            row.aligns,
            vec![TextAlign::Start, TextAlign::Center, TextAlign::End]
        );
        assert!(row.last && !row.header);
    }
}
