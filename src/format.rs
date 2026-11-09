// Toolkit-free, carried over unchanged from the Floem shell; some helpers are kept for the keys still to come.
#![allow(dead_code)]
//! Turning a toolbar key into an edit of the note's text: one range to
//! replace, what to put there, and where the caret lands after.
//!
//! The iced shell carried its own copy of this (`editor.rs::apply_format`,
//! `sheet.rs::set_kind`, deliberately not shared — it died with that shell at
//! phase 4, TRD §5). What must not diverge is the MARKERS, and those come from
//! `synapso_core::sheet` — the one function markdown is written with.
//!
//! The shell's version does two things this one does not, both noted where
//! they would go: it unwraps the pair *around* the caret when nothing is
//! selected, and it reaches for a double backtick when the selection already
//! holds a lone one.

use std::ops::Range;

use synapso_core::doc::Kind;
use synapso_core::inline;
use synapso_core::sheet;
use synapso_core::table;

/// One replacement: what to cut, what to put there, and where the caret goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub caret: usize,
}

/// The inline formats, and the marker each one writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Strike,
    Code,
}

impl Format {
    pub fn marker(self) -> &'static str {
        match self {
            Format::Bold => "**",
            Format::Italic => "*",
            Format::Strike => "~~",
            Format::Code => "`",
        }
    }
}

/// Wrap the selection in the format's marker — or take it off, when it is
/// already wearing it. With nothing selected, the markers are written empty
/// and the caret is put between them, ready to type into.
pub fn wrap(text: &str, selection: Range<usize>, format: Format) -> Edit {
    let marker = format.marker();
    let selected = text.get(selection.clone()).unwrap_or_default();
    if selection.is_empty() {
        // the shell also unwraps the pair the caret is standing
        // inside. Add it here when someone misses it.
        return Edit {
            range: selection.start..selection.start,
            text: format!("{marker}{marker}"),
            caret: selection.start + marker.len(),
        };
    }
    // already wrapped, inside the selection: take the markers off
    if selected.len() > marker.len() * 2
        && selected.starts_with(marker)
        && selected.ends_with(marker)
    {
        let inner = &selected[marker.len()..selected.len() - marker.len()];
        return Edit {
            caret: selection.start + inner.len(),
            text: inner.to_string(),
            range: selection,
        };
    }
    // already wrapped, just outside it: take those off instead, or the second
    // press would nest the markers rather than undo the first
    let before = text.get(..selection.start).unwrap_or_default();
    let after = text.get(selection.end..).unwrap_or_default();
    if before.ends_with(marker) && after.starts_with(marker) {
        let range = selection.start - marker.len()..selection.end + marker.len();
        return Edit {
            caret: range.start + selected.len(),
            text: selected.to_string(),
            range,
        };
    }
    let wrapped = format!("{marker}{selected}{marker}");
    Edit {
        caret: selection.start + wrapped.len(),
        text: wrapped,
        range: selection,
    }
}

/// `[words](  )` around the selection, with the caret parked where the address
/// goes — a link is the one inline format whose two halves are not the same
/// marker, so `wrap` cannot express it.
///
/// ⚠️ the shell also UNWRAPS a link the caret is standing in; this does not.
/// Noted rather than done, for the same reason the rest of this module gives.
pub fn link(text: &str, selection: Range<usize>) -> Edit {
    let words = text.get(selection.clone()).unwrap_or_default();
    let wrapped = format!("[{words}]()");
    Edit {
        // between the parens, which is what you type next
        caret: selection.start + wrapped.len() - 1,
        text: wrapped,
        range: selection,
    }
}

/// Is the caret standing in a span of this format? What lights the toolbar
/// key, so the strip says what the words under the caret already are.
///
/// ⚠️ the span, not the markers. `inline::spans` hands `**bold**` back as a
/// `Marker`, a `Bold` and a `Marker`, so a caret on the asterisks themselves
/// is outside the bold run — which is right: touching either end counts, and
/// that is what makes a selection taken WITH its markers light up too.
pub fn active(text: &str, at: Range<usize>, format: Format) -> bool {
    let start = text
        .get(..at.start)
        .map_or(0, |b| b.rfind('\n').map_or(0, |nl| nl + 1));
    let end = text[start..].find('\n').map_or(text.len(), |nl| start + nl);
    let (Some(line), Some(here)) = (text.get(start..end), at.end.min(end).checked_sub(start))
    else {
        return false;
    };
    let there = at.start.saturating_sub(start);
    inline::spans(line).iter().any(|(range, style)| {
        wears(*style) == Some(format) && range.start <= here && there <= range.end
    })
}

/// Which toolbar key an inline style belongs to. A link and a marker are
/// nobody's key.
fn wears(style: inline::Style) -> Option<Format> {
    Some(match style {
        inline::Style::Bold => Format::Bold,
        inline::Style::Italic => Format::Italic,
        inline::Style::Strike => Format::Strike,
        inline::Style::Code => Format::Code,
        _ => return None,
    })
}

/// Can the line's kind become `to`? The transition policy the toolbar's menus
/// read, so a change that cannot land is shown DISABLED instead of being
/// silently swallowed by [`set_kind`] — ADR-004 §D4's rule applied to text: a
/// control that is visible has to work.
///
/// Pure on purpose: the menu asks it to draw a row, `set_kind` asks it to
/// refuse one, and there is exactly one answer.
pub fn allowed(from: &Kind, to: &Kind) -> bool {
    transforms(from, to) && !same_kind(from, to)
}

/// Can this line's syntax be rewritten at all? [`allowed`] minus the "already
/// that" half — which is a MENU's rule and not an edit's: ⌘⏎ flips `- [x]`
/// back to `- [ ]` through `set_kind`, and that is the same kind twice on
/// purpose (`behave::toggle_task`).
fn transforms(from: &Kind, to: &Kind) -> bool {
    !owns_its_line(from) && !owns_its_line(to)
}

/// A fence, a table, a rule and an image are their own syntax from the first
/// byte to the last. `## ` in front of `| a | b |` is not a heading, it is a
/// broken table — and the four are INSERTED (`fence`, `table`), never turned
/// into, which is why they are refused as a destination too.
fn owns_its_line(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Image(_) | Kind::Table | Kind::Code { .. } | Kind::Rule
    )
}

/// ⚠️ a ticked and an unticked task are the SAME kind here. `- [x] done` is
/// already a task list; offering "Task list" on it would untick the box, which
/// is a different verb wearing this one's label.
pub fn same_kind(a: &Kind, b: &Kind) -> bool {
    matches!((a, b), (Kind::Task(_), Kind::Task(_))) || a == b
}

/// The kind of the line the caret is on — what the menus check a row against.
pub fn kind_at(text: &str, at: usize) -> Kind {
    let start = text
        .get(..at)
        .map_or(0, |b| b.rfind('\n').map_or(0, |nl| nl + 1));
    // ⚠️ the WHOLE note, not the line alone: a fence carries state across
    // lines, so a line inside a code block only reads as `Code` when the
    // opening ``` was parsed with it — and a table's header only reads as
    // `Table` with the row under it, since a row alone is a paragraph.
    line_in(text, start).map_or(Kind::Paragraph, |line| line.kind)
}

/// The parsed line that starts at byte `start`, read with the whole note.
fn line_in(text: &str, start: usize) -> Option<sheet::Line> {
    let n = text.get(..start)?.matches('\n').count();
    sheet::parse(text).into_iter().nth(n)
}

/// Make the line the caret is on a heading, a bullet, a task or a quote.
///
/// `None` when [`allowed`] says the line will not have it, and when the line
/// already IS this kind — left exactly as it is, the same answer the shell
/// gives.
pub fn set_kind(text: &str, at: usize, kind: &Kind) -> Option<Edit> {
    let start = text.get(..at)?.rfind('\n').map_or(0, |nl| nl + 1);
    let end = text.get(at..)?.find('\n').map_or(text.len(), |nl| at + nl);
    let raw = text.get(start..end)?;
    // in context, for the reason `kind_at` gives: a table row reads as one
    // only beside its neighbour
    let line = line_in(text, start)?;
    if !transforms(&line.kind, kind) {
        return None;
    }
    let indent = " ".repeat(line.spaces);
    let words = raw.get(line.marker..).unwrap_or_default();
    let replaced = format!("{indent}{}{words}", sheet::marker_for(kind));
    if replaced == raw {
        return None;
    }
    Some(Edit {
        // the caret keeps its place in the WORDS, not its column: turning a
        // line into a heading must not leave the caret inside the `##`
        caret: start + replaced.len() - words.len() + at.saturating_sub(start + line.marker),
        text: replaced,
        range: start..end,
    })
}

/// A block that is INSERTED rather than turned into: it lands on the caret's
/// line when that line is blank, and on a new line under it when it is not —
/// splitting a sentence in half to make room for a table is not what anyone
/// pressing the key meant. `into` is where the caret goes inside the block.
fn insert_block(text: &str, at: usize, block: &str, into: usize) -> Edit {
    let start = text
        .get(..at)
        .map_or(0, |b| b.rfind('\n').map_or(0, |nl| nl + 1));
    let end = text[start..].find('\n').map_or(text.len(), |nl| start + nl);
    if text.get(start..end).unwrap_or_default().trim().is_empty() {
        return Edit {
            range: start..end,
            text: block.to_string(),
            caret: start + into,
        };
    }
    Edit {
        range: end..end,
        text: format!("\n{block}"),
        caret: end + 1 + into,
    }
}

/// A 2×2 table skeleton, caret in the first cell.
///
/// ⚠️ built by `synapso_core::table` and not by hand: the padding is what makes
/// the grid read as a grid (ADR-006), and `align` is the one function that
/// knows the widths. A hand-written `| | |` would be re-padded on the first ⇥
/// and the file would churn.
pub fn table(text: &str, at: usize) -> Edit {
    let grid = table::from_header(&table::blank_row(2));
    let into = grid
        .lines()
        .next()
        .map_or(2, |header| table::cell_start(header, 0));
    insert_block(text, at, &grid, into)
}

/// An empty fence, caret on the line between the backticks — the Blocks menu's
/// "Code block", which is an insert and not a transition (`allowed` refuses
/// `Kind::Code` as a destination, and this is why).
pub fn fence(text: &str, at: usize) -> Edit {
    insert_block(text, at, "```\n\n```", 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(text: &str, edit: &Edit) -> String {
        let mut out = text.to_string();
        out.replace_range(edit.range.clone(), &edit.text);
        out
    }

    #[test]
    fn wrapping_a_selection_and_pressing_again_undoes_it() {
        let text = "uma palavra aqui";
        let sel = 4..11; // "palavra"
        let bold = wrap(text, sel.clone(), Format::Bold);
        assert_eq!(applied(text, &bold), "uma **palavra** aqui");

        // the same words, now selected WITH their markers
        let text = "uma **palavra** aqui";
        let off = wrap(text, 4..15, Format::Bold);
        assert_eq!(applied(text, &off), "uma palavra aqui");

        // and selected WITHOUT them, which is what a second press really sees
        let off = wrap(text, 6..13, Format::Bold);
        assert_eq!(applied(text, &off), "uma palavra aqui");
    }

    #[test]
    fn with_nothing_selected_the_caret_lands_between_the_markers() {
        let edit = wrap("abc", 3..3, Format::Code);
        assert_eq!(applied("abc", &edit), "abc``");
        assert_eq!(edit.caret, 4, "between the two backticks");
    }

    #[test]
    fn a_link_parks_the_caret_where_the_address_goes() {
        let text = "see the docs";
        let edit = link(text, 4..12); // "the docs"
        assert_eq!(applied(text, &edit), "see [the docs]()");
        assert_eq!(edit.caret, "see [the docs](".len());

        // and with nothing selected it is an empty pair, caret still in the
        // parens — the address is what you have and the words come after
        let edit = link("", 0..0);
        assert_eq!(applied("", &edit), "[]()");
        assert_eq!(edit.caret, 3);
    }

    #[test]
    fn a_line_becomes_a_heading_and_the_caret_stays_in_the_words() {
        let text = "just words\nsecond line\n";
        let edit = set_kind(text, 5, &Kind::Heading(2)).expect("plain text takes a heading");
        assert_eq!(applied(text, &edit), "## just words\nsecond line\n");
        // the caret was 5 bytes into the words, and still is
        assert_eq!(&applied(text, &edit)[edit.caret..edit.caret + 5], "words");
    }

    #[test]
    fn a_heading_becomes_a_bullet_without_keeping_the_hashes() {
        let text = "## a title";
        let edit = set_kind(text, 4, &Kind::Bullet).unwrap();
        assert_eq!(applied(text, &edit), "- a title");
    }

    #[test]
    fn the_lines_that_own_themselves_refuse() {
        // a rule, a fence and a table are their own syntax
        assert!(set_kind("---", 0, &Kind::Bullet).is_none());
        assert!(set_kind("```rust", 0, &Kind::Quote).is_none());
        assert!(set_kind("| a | b |\n| c | d |", 0, &Kind::Heading(1)).is_none());
        // and a line already wearing the kind is left alone
        assert!(set_kind("- item", 3, &Kind::Bullet).is_none());
    }

    /// What the toolbar's B / I / `<>` light up on: the caret's own span, and
    /// nothing on the next line.
    #[test]
    fn the_toolbar_lights_on_the_span_the_caret_is_in() {
        let text = "plain **bold** and `code` here\nsecond line\n";
        let at = |needle: &str| text.find(needle).expect("in the fixture");

        let bold = at("bold");
        assert!(active(text, bold + 1..bold + 1, Format::Bold));
        assert!(!active(text, bold + 1..bold + 1, Format::Italic));
        assert!(!active(text, bold + 1..bold + 1, Format::Code));
        // the caret in the `code` chip lights the other key instead
        let code = at("code");
        assert!(active(text, code..code, Format::Code));
        assert!(!active(text, code..code, Format::Bold));
        // plain words light nothing
        let plain = at("and");
        assert!(!active(text, plain..plain, Format::Bold));
        assert!(!active(text, plain..plain, Format::Code));

        // a selection taken WITH its markers still counts — that selection is
        // exactly what a second ⌘B unwraps
        assert!(active(text, at("**")..at("bold") + 6, Format::Bold));

        // ⚠️ the caret's LINE and no other: a span above must not light the
        // key for a line that has none
        let second = at("second");
        assert!(!active(text, second..second, Format::Bold));
        // and neither end of the note panics
        assert!(!active(text, 0..0, Format::Bold));
        assert!(!active(text, text.len()..text.len(), Format::Bold));
    }

    /// The transition policy, as a table. Every row is a menu row that has to
    /// be offered or greyed, and getting one wrong is a key that mangles a
    /// line — the reason `allowed` exists at all instead of `set_kind`
    /// answering `None` after the click.
    #[test]
    fn the_transition_policy_offers_only_what_lands() {
        let code = || Kind::Code {
            lang: String::new(),
            fence: "```".into(),
        };
        let table = [
            // (from, to, offered)
            (Kind::Paragraph, Kind::Heading(2), true),
            (Kind::Heading(1), Kind::Heading(2), true),
            (Kind::Heading(2), Kind::Bullet, true),
            (Kind::Bullet, Kind::Ordered, true),
            (Kind::Quote, Kind::Task(false), true),
            (Kind::Task(false), Kind::Quote, true),
            // a line already wearing the kind has nothing to do
            (Kind::Heading(2), Kind::Heading(2), false),
            (Kind::Bullet, Kind::Bullet, false),
            // ⚠️ and a ticked task is ALREADY a task list
            (Kind::Task(true), Kind::Task(false), false),
            // the four that own their line refuse, whichever end they are on
            (code(), Kind::Heading(1), false),
            (Kind::Table, Kind::Heading(1), false),
            (Kind::Rule, Kind::Bullet, false),
            (Kind::Image("a.png".into()), Kind::Quote, false),
            (Kind::Paragraph, code(), false),
            (Kind::Paragraph, Kind::Table, false),
            (Kind::Paragraph, Kind::Rule, false),
        ];
        for (from, to, offered) in table {
            assert_eq!(
                allowed(&from, &to),
                offered,
                "{from:?} → {to:?} should be {}",
                if offered { "offered" } else { "greyed" }
            );
        }
    }

    /// `set_kind` and the menus read the same rule: what is greyed is also
    /// what a click could not have done.
    #[test]
    fn what_the_policy_greys_set_kind_refuses() {
        assert!(allowed(&Kind::Paragraph, &Kind::Heading(2)));
        assert!(set_kind("just words", 0, &Kind::Heading(2)).is_some());
        assert!(!allowed(&Kind::Table, &Kind::Heading(2)));
        assert!(set_kind("| a | b |\n| c | d |", 0, &Kind::Heading(2)).is_none());
        // ⚠️ the ONE place the two part company: a menu will not offer "Task
        // list" on a task, but ⌘⏎ still has to flip the box through here.
        assert!(!allowed(&Kind::Task(true), &Kind::Task(false)));
        let ticked = "- [x] done";
        let edit = set_kind(ticked, 8, &Kind::Task(false)).expect("⌘⏎ still unticks");
        assert_eq!(applied(ticked, &edit), "- [ ] done");
    }

    /// The kind under the caret, which is what the menu checks its rows
    /// against — and the fence is the one that needs the lines above it.
    #[test]
    fn the_caret_reads_the_kind_of_its_own_line() {
        let text = "# title\n- item\n\n```rust\nfn main() {}\n```\n";
        let at = |needle: &str| text.find(needle).expect("in the fixture");
        assert_eq!(kind_at(text, at("title")), Kind::Heading(1));
        assert_eq!(kind_at(text, at("item")), Kind::Bullet);
        // ⚠️ INSIDE the fence, which only a parse of the lines above can see
        assert!(matches!(kind_at(text, at("fn main")), Kind::Code { .. }));
        assert_eq!(kind_at("", 0), Kind::Paragraph);
    }

    #[test]
    fn a_table_lands_as_a_grid_with_the_caret_in_the_first_cell() {
        // on a blank line it takes the line
        let edit = table("", 0);
        let grid = applied("", &edit);
        let rows: Vec<&str> = grid.lines().collect();
        assert_eq!(rows.len(), 3, "header, divider, one row to type into");
        assert!(synapso_core::table::is_divider(rows[1]), "{:?}", rows[1]);
        assert_eq!(synapso_core::table::cells(rows[0]).len(), 2, "2 columns");
        // the caret is in the first cell and not on a wall
        assert_eq!(&grid[edit.caret..=edit.caret], " ");
        assert_eq!(synapso_core::table::cell_at(rows[0], edit.caret), 0);

        // ⚠️ on a line with words it goes UNDER them, never through them
        let text = "a sentence";
        let edit = table(text, 3);
        assert!(applied(text, &edit).starts_with("a sentence\n|"));
    }

    #[test]
    fn a_fence_opens_with_the_caret_between_the_backticks() {
        let edit = fence("", 0);
        let out = applied("", &edit);
        assert_eq!(out, "```\n\n```");
        assert_eq!(edit.caret, 4, "the empty line inside the fence");
        // and `allowed` keeps the menus from offering it as a transition
        assert!(!allowed(
            &Kind::Paragraph,
            &Kind::Code {
                lang: String::new(),
                fence: "```".into()
            }
        ));
    }

    #[test]
    fn indentation_survives_the_change() {
        let text = "    - deep item";
        let edit = set_kind(text, 8, &Kind::Task(false)).unwrap();
        assert_eq!(applied(text, &edit), "    - [ ] deep item");
    }

    /// A row alone is a paragraph, so the menus treat it as one; a row of a
    /// real table still owns its line — and both answers need the line under
    /// it, which is where the table's second row is.
    #[test]
    fn a_lone_pipe_line_takes_a_heading_and_a_table_row_still_refuses() {
        let lone = "|test|";
        assert_eq!(kind_at(lone, 0), Kind::Paragraph);
        let edit = set_kind(lone, 0, &Kind::Heading(2)).expect("words take a heading");
        assert_eq!(applied(lone, &edit), "## |test|");

        let table = "| a | b |\n| - | - |";
        assert_eq!(
            kind_at(table, 0),
            Kind::Table,
            "the header, read with its divider"
        );
        assert!(set_kind(table, 0, &Kind::Heading(2)).is_none());
    }
}
