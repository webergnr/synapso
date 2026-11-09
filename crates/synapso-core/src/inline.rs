//! Inline markdown, read off one line at a time.
//!
//! `**bold**` is two markers around a word, and every shell that wants to draw
//! it bold has to be told which bytes are which. That reading is pure string
//! work — [`spans`] is the whole of it, and [`Style`] is the vocabulary it
//! answers in. What a shell then *does* with a span (a font face, a colour, a
//! size of nothing) is the shell's own half, and lives there.
//!
//! The markers stay in the buffer, because the buffer is the file. What a
//! reader is shown instead is [`shown`] — the same line with the markers that
//! are off screen taken out — and [`shown_offset`] maps a position in the one
//! to a position in the other, which is what keeps a painted chip under the
//! word it was measured for.
//!
//! Two things a per-span style cannot say are named here and painted by the
//! shell: the chip behind `code` and the line through ~~a deletion~~ (see
//! [`decorations`]).

use std::borrow::Cow;
use std::ops::Range;

/// What one run of a line *is*. A shell turns this into a face and a colour;
/// here it is only the reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The `**`, the backtick, the brackets — still there, barely.
    Marker,
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}

/// Symmetric delimiters, longest first so `**` wins over `*` — and so that a
/// double backtick opens one code span holding a backtick, rather than two
/// empty ones around it.
const PAIRS: [(&str, Style); 7] = [
    ("**", Style::Bold),
    ("__", Style::Bold),
    ("~~", Style::Strike),
    ("``", Style::Code),
    ("`", Style::Code),
    ("*", Style::Italic),
    ("_", Style::Italic),
];

/// Byte ranges over one line — what a highlighter must hand back.
pub fn spans(line: &str) -> Vec<(Range<usize>, Style)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;

    'scan: while i < b.len() {
        // [[note]] / [[canvas: node]] — the mention, brackets included. With
        // `[[note|label]]` the label is the part that reads as the link and
        // the target fades back with the brackets.
        if at(b, i, b"[[") {
            if let Some(end) = find(b, i + 2, b"]]") {
                match find(b, i + 2, b"|").filter(|bar| *bar < end) {
                    Some(bar) => {
                        out.push((i..bar + 1, Style::Marker));
                        out.push((bar + 1..end, Style::Link));
                        out.push((end..end + 2, Style::Marker));
                    }
                    None => out.push((i..end + 2, Style::Link)),
                }
                i = end + 2;
                continue;
            }
        }
        // [label](url): the label reads as the link, the machinery fades
        if b[i] == b'[' {
            if let Some(close) = find(b, i + 1, b"]") {
                if at(b, close + 1, b"(") {
                    if let Some(paren) = find(b, close + 2, b")") {
                        out.push((i..i + 1, Style::Marker));
                        out.push((i + 1..close, Style::Link));
                        out.push((close..paren + 1, Style::Marker));
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }
        for (delimiter, style) in PAIRS {
            let d = delimiter.as_bytes();
            // `_` inside a word is a name, not emphasis (snake_case survives)
            if d[0] == b'_' && i > 0 && (b[i - 1] as char).is_alphanumeric() {
                continue;
            }
            if !at(b, i, d) {
                continue;
            }
            let open = i + d.len();
            let Some(end) = find(b, open, d) else {
                continue;
            };
            if end == open {
                continue; // `****` wraps nothing
            }
            out.push((i..open, Style::Marker));
            out.push((open..end, style));
            out.push((end..end + d.len(), Style::Marker));
            i = end + d.len();
            continue 'scan;
        }
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    // ⚠️ `^id` at the end of a line is a NAME, not prose — it is what
    // `[[note#^id]]` lands on. Syntax, so it fades with the rest of the
    // syntax; skipped when something else already claimed those bytes, which
    // is what a `^` inside a code span is.
    if let Some((range, _)) = anchor(line) {
        if out.last().is_none_or(|(last, _)| last.end <= range.start) {
            out.push((range, Style::Marker));
        }
    }
    out
}

/// The block anchor a line wears: `^id` at its very end, after a space.
///
/// Answers the whole marker — the space in front of it included, so hiding it
/// leaves no gap — and the id on its own.
///
/// ⚠️ a line has no identity of its own: its number moves and its words are
/// edited, so neither can be linked to. An anchor is a name somebody WROTE
/// into the file, which is the only thing about a line that holds still. Same
/// spelling as Obsidian's, so a vault opened there follows the same links.
///
/// ⚠️ never the whole line. A lone `^id` on its own line is Obsidian's way of
/// naming the block ABOVE it, and a name that points somewhere else is worse
/// than no name — it reads as an anchor and lands a line off.
pub fn anchor(line: &str) -> Option<(Range<usize>, &str)> {
    let end = line.trim_end();
    let (space, blank) = end.char_indices().rev().find(|(_, c)| c.is_whitespace())?;
    let id = end[space + blank.len_utf8()..].strip_prefix('^')?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        .then_some((space..line.len(), id))
}

fn at(b: &[u8], i: usize, needle: &[u8]) -> bool {
    b.len() >= i + needle.len() && &b[i..i + needle.len()] == needle
}

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > b.len() {
        return None;
    }
    b[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// One line as it is actually laid out: the markers that vanish are drawn at
/// nothing, so as far as width and wrapping go they are not there.
///
/// ⚠️ Borrowed when the line has no markers, which is most of them — this is
/// called per line per frame by whatever is painted alongside the text.
pub fn shown(line: &str) -> Cow<'_, str> {
    without(line, gone(line, true))
}

/// The markers that are not on screen. `only_vanishing` is the editor's
/// question — there `*` is still standing (see [`vanishes`]) and still takes a
/// column. A preview drawn as plain text asks the other one: nothing can be
/// bold there anyway, so all of it goes.
fn gone(line: &str, only_vanishing: bool) -> Vec<Range<usize>> {
    spans(line)
        .into_iter()
        .filter(|(range, style)| {
            *style == Style::Marker && (!only_vanishing || vanishes(&line[range.clone()]))
        })
        .map(|(range, _)| range)
        .collect()
}

fn without(line: &str, cuts: Vec<Range<usize>>) -> Cow<'_, str> {
    if cuts.is_empty() {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    let mut cut = 0;
    for range in cuts {
        out.push_str(&line[cut..range.start]);
        cut = range.end;
    }
    out.push_str(&line[cut..]);
    Cow::Owned(out)
}

/// Where raw byte `at` ends up in [`shown`] — a position counted in raw bytes
/// drifts right by every marker in front of it, which is a chip painted next
/// to the word instead of under it.
pub fn shown_offset(line: &str, at: usize) -> usize {
    let gone: usize = gone(line, true)
        .into_iter()
        .map(|range| {
            if range.end <= at {
                range.len()
            } else {
                // a marker the position lands inside counts only up to it
                at.saturating_sub(range.start)
            }
        })
        .sum();
    at - gone
}

/// The text without its markers — for previews drawn as plain text, where
/// nothing can be bold anyway.
pub fn strip(text: &str) -> String {
    text.lines()
        .map(|line| without(line, gone(line, false)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The mention under the cursor: `[[…]]` containing char index `column`.
/// Clicking it follows the link, the way Obsidian's live preview does. What
/// comes back is the *target* — everything left of the optional `|label`.
pub fn mention_at(line: &str, column: usize) -> Option<String> {
    let cursor = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(byte, _)| byte);
    let b = line.as_bytes();
    let mut i = 0;
    while let Some(start) = find(b, i, b"[[") {
        let end = find(b, start + 2, b"]]")?;
        if (start..end + 2).contains(&cursor) {
            let inner = &line[start + 2..end];
            let target = inner.split('|').next().unwrap_or(inner);
            return Some(target.trim().to_string());
        }
        i = end + 2;
    }
    None
}

/// Whether a marker may be taken off the screen.
///
/// ⚠️ `*` and `__` stay. **A `text_editor` cannot render two font faces on one
/// line** — cosmic-text resolves the face ONCE per shaping run, from the attrs
/// at the run's first byte (`shape_run`, cosmic-text 0.15), while colour and
/// size are read per glyph. That is why our fork's `Format::size` works and
/// `Format::font` never has: a heading is bold because the whole line asks for
/// bold, and `**this**` is not, because the line around it asks for regular.
///
/// So bold and italic are, on screen, only their markers. Hiding those would
/// erase the emphasis rather than draw it. Everything else has a signal that
/// does not need a second face — the chip, the strike, the colour — and can
/// let its syntax go.
///
/// ⚠️ This rule is the current shell's, not markdown's: on parley the emphasis
/// is actually drawn, so the marker can go and this whole function goes with it
/// (docs/floem-migration-trd.md §4.1).
pub fn vanishes(marker: &str) -> bool {
    !marker.chars().all(|c| c == '*' || c == '_')
}

/// What a per-span style cannot say, and something has to paint: the chip
/// behind inline code, and the line through a deletion. Byte ranges over one
/// line, the markers left out — the chip hugs the word, not the backticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    Code,
    Strike,
}

pub fn decorations(line: &str) -> Vec<(Range<usize>, Decoration)> {
    spans(line)
        .into_iter()
        .filter_map(|(range, style)| {
            Some((
                range,
                match style {
                    Style::Code => Decoration::Code,
                    Style::Strike => Decoration::Strike,
                    _ => return None,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(line: &str) -> Vec<(&str, Style)> {
        spans(line)
            .into_iter()
            .map(|(range, style)| (&line[range], style))
            .collect()
    }

    #[test]
    fn markers_and_the_words_they_wrap_get_their_own_spans() {
        assert_eq!(
            styled("a **b** c"),
            vec![
                ("**", Style::Marker),
                ("b", Style::Bold),
                ("**", Style::Marker)
            ]
        );
        assert_eq!(
            styled("`x`"),
            vec![
                ("`", Style::Marker),
                ("x", Style::Code),
                ("`", Style::Marker)
            ]
        );
        // `**` is tried before `*`, so bold never reads as two italics
        assert_eq!(styled("**b**")[1].1, Style::Bold);
        assert_eq!(styled("*i*")[1].1, Style::Italic);
    }

    #[test]
    fn what_is_not_emphasis_stays_plain() {
        assert!(styled("snake_case_name").is_empty());
        assert!(styled("****").is_empty());
        assert!(styled("an * unclosed").is_empty());
        assert!(styled("plain text").is_empty());
        // bytes, not chars: an accent before the marker must not split a span
        assert_eq!(styled("café **b**")[1].0, "b");
    }

    #[test]
    fn both_link_shapes_are_recognised() {
        assert_eq!(
            styled("see [[auth-flow#Token]] now"),
            vec![("[[auth-flow#Token]]", Style::Link)]
        );
        assert_eq!(
            styled("[label](http://x)"),
            vec![
                ("[", Style::Marker),
                ("label", Style::Link),
                ("](http://x)", Style::Marker)
            ]
        );
    }

    #[test]
    fn stripping_leaves_the_words_and_takes_the_syntax() {
        assert_eq!(strip("a **b** and `c`"), "a b and c");
        assert_eq!(strip("see [label](http://x)"), "see label");
        // a mention keeps its brackets: they are what it looks like
        assert_eq!(strip("see [[auth-flow]]"), "see [[auth-flow]]");
        assert_eq!(strip("plain\nlines"), "plain\nlines");
    }

    #[test]
    fn a_labelled_mention_shows_the_label_and_follows_the_target() {
        assert_eq!(
            styled("see [[auth-flow|the flow]] here"),
            vec![
                ("[[auth-flow|", Style::Marker),
                ("the flow", Style::Link),
                ("]]", Style::Marker),
            ]
        );
        assert_eq!(
            mention_at("see [[auth-flow|the flow]] here", 18).as_deref(),
            Some("auth-flow")
        );
        // and a preview keeps the label, not the file name
        assert_eq!(strip("see [[auth-flow|the flow]]"), "see the flow");
    }

    /// A line can be given a name, and the name is syntax: it is what
    /// `[[note#^id]]` lands on, and a reader should never see it.
    #[test]
    fn a_line_can_wear_a_name() {
        assert_eq!(
            anchor("the gateway holds the secret ^gate-1"),
            Some((28..36, "gate-1"))
        );
        // the space in front goes too, so hiding it leaves no gap
        assert_eq!(
            shown("the gateway holds the secret ^gate-1"),
            "the gateway holds the secret"
        );
        // trailing blanks are not part of anything
        assert_eq!(anchor("named  ^a1   ").map(|(_, id)| id), Some("a1"));
        // ⚠️ a lone `^id` names the block ABOVE it in Obsidian — landing on
        // this line would be a line off, so it is not an anchor here
        assert_eq!(anchor("^a1"), None);
        // and none of these is a name
        assert_eq!(anchor("two to the ^"), None);
        assert_eq!(anchor("2 ^ 3"), None);
        assert_eq!(anchor("a caret ^in the middle of it"), None);
        assert_eq!(anchor("code ^like this"), None);
        assert_eq!(anchor(""), None);
        // ⚠️ inside a code span the bytes are already spoken for: a `^` there
        // is a caret somebody is quoting, not a name
        assert_eq!(anchor("run `xargs ^abc`"), None);
    }

    /// ⚠️ caught on screen: `shown` took *every* marker out, so with a `**`
    /// still standing on the line the strike was painted two columns early —
    /// over the asterisks instead of the word.
    #[test]
    fn a_marker_that_is_still_on_screen_still_takes_a_column() {
        let line = "**b** ~~s~~";
        assert_eq!(shown(line), "**b** s", "the tildes go, the asterisks stay");
        assert_eq!(shown_offset(line, 8), 6, "column 6 of what is drawn");
        // a preview is plain text where nothing can be bold: all of it goes
        assert_eq!(strip(line), "b s");
    }

    #[test]
    fn what_has_to_be_painted_is_the_word_without_its_syntax() {
        let of = |line: &str| {
            decorations(line)
                .into_iter()
                .map(|(r, what)| (line[r].to_string(), what))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            of("run `cargo test` now"),
            vec![("cargo test".into(), Decoration::Code)]
        );
        assert_eq!(of("~~gone~~"), vec![("gone".into(), Decoration::Strike)]);
        // bold and italic are a per-span style's to draw; nothing to paint
        assert!(of("**b** and *i*").is_empty());
    }

    #[test]
    fn a_click_inside_the_brackets_finds_the_mention() {
        let line = "see [[auth-flow]] and [[canvas: token store]]";
        assert_eq!(mention_at(line, 8).as_deref(), Some("auth-flow"));
        assert_eq!(mention_at(line, 4).as_deref(), Some("auth-flow")); // on the `[`
        assert_eq!(mention_at(line, 25).as_deref(), Some("canvas: token store"));
        assert_eq!(mention_at(line, 2), None);
        assert_eq!(mention_at("no mention here", 3), None);
    }
}
