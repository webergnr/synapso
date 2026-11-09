//! Card text metrics — the arithmetic half of the board's note cards.
//!
//! The cards are monospaced, so a column count is all the wrapping needs —
//! no font metrics, no layout engine, which is exactly why this lives here
//! and the painting lives in the shell: a card's height is decided here,
//! not by whatever font the shell happens to draw with.

use crate::theme::TEXT_XS;

const CARD_COL_W: f32 = TEXT_XS * 0.6;
const CARD_LINE_H: f32 = TEXT_XS * 1.4;
/// Card padding above (title) and below (meta) the body.
const CARD_BODY_TOP: f32 = 32.0;
const CARD_BODY_BOTTOM: f32 = 24.0;

pub fn card_columns(width: f32) -> usize {
    (((width - 26.0) / CARD_COL_W) as usize).max(8)
}

pub fn card_lines(height: f32) -> usize {
    (((height - CARD_BODY_TOP - CARD_BODY_BOTTOM) / CARD_LINE_H) as usize).max(1)
}

/// Keep card chrome in its own band. Canvas text wraps downward by default;
/// shortening it first is what makes a narrow title or footer stay inside the
/// fixed-height card instead of leaking through its rounded edge.
pub fn one_line(text: &str, columns: usize) -> String {
    let lines = wrap(text, columns.max(1));
    let mut first = lines.first().cloned().unwrap_or_default();
    if lines.len() > 1 {
        ellipsize(&mut first);
    }
    first
}

/// The height a card needs to show `lines` of body.
pub fn card_height(lines: usize) -> f32 {
    CARD_BODY_TOP + CARD_LINE_H * lines as f32 + CARD_BODY_BOTTOM
}

/// Replace the last character of a clipped line with an ellipsis.
///
/// ⚠️ By CHARACTER, not by byte: `String::truncate` panics off a boundary, and a
/// wrapped line can end on any character — "café" is five bytes, and cutting one
/// off the end lands inside the "é".
pub fn ellipsize(line: &mut String) {
    let cut = line
        .trim_end()
        .char_indices()
        .next_back()
        .map_or(0, |(i, _)| i);
    line.truncate(cut);
    line.push('…');
}

/// Greedy word wrap, honouring the text's own line breaks. A word longer than
/// the column count is split rather than allowed to overflow.
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    for source in text.lines() {
        let source = source.trim_end();
        if source.trim().is_empty() {
            continue;
        }
        let mut line = String::new();
        for word in source.split_whitespace() {
            let mut word = word;
            while word.chars().count() > cols {
                let (head, tail) =
                    word.split_at(word.char_indices().nth(cols).map_or(word.len(), |(i, _)| i));
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                out.push(head.to_string());
                word = tail;
            }
            if line.is_empty() {
                line.push_str(word);
            } else if line.chars().count() + 1 + word.chars().count() <= cols {
                line.push(' ');
                line.push_str(word);
            } else {
                out.push(std::mem::replace(&mut line, word.to_string()));
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_fits_the_columns_and_keeps_the_words() {
        let lines = wrap("the quick brown fox jumps", 10);
        assert_eq!(lines, vec!["the quick", "brown fox", "jumps"]);
        assert!(lines.iter().all(|l| l.chars().count() <= 10));

        // a word longer than the line is split, not overflowed
        assert_eq!(
            wrap("supercalifragilistic", 8),
            vec!["supercal", "ifragili", "stic"]
        );
        // the text's own breaks are kept, blank lines dropped
        assert_eq!(wrap("one\n\ntwo", 10), vec!["one", "two"]);
        assert!(wrap("   ", 10).is_empty());
    }

    #[test]
    fn a_card_is_as_tall_as_the_lines_it_shows() {
        for lines in [1, 3, 12] {
            assert_eq!(card_lines(card_height(lines)), lines);
        }
    }

    #[test]
    fn narrow_card_chrome_is_always_one_line() {
        assert_eq!(one_line("short title", 40), "short title");
        let title = one_line("a title that must shrink", 8);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 8);
        let meta = one_line("missing note — double-click to open", 12);
        assert!(meta.ends_with('…'));
        assert!(meta.chars().count() <= 12);
        assert!(!one_line("café com leite", 5).contains('\n'));
    }
}
