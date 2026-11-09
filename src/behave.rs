// Toolkit-free, carried over unchanged from the Floem shell; some helpers are kept for the keys still to come.
#![allow(dead_code)]
//! What ⏎, ⇥ and ⌘⏎ mean inside a note: the list that carries on, the table
//! that pads itself and grows a row, the fence that writes its own closer, the
//! checkbox that flips. Every one is a single replacement of the buffer plus
//! where the caret lands — a `format::Edit`, like the toolbar's.
//!
//! The iced shell carried its own copies of all of this (`sheet.rs`:
//! `table_enter`, `close_fence`, `indent`, `table_cell`, `toggle_task`,
//! `exit_fence`), deliberately not shared — gone with it at phase 4 (TRD §5).
//! What must not diverge is the markdown they write, and that comes from
//! `synapso_core::{sheet, table}`.
//!
//! The one thing the shell does that this does not: `paste`, which replaces a
//! line that is nothing but its opening syntax. Noted where it would go.

use std::ops::Range;

use synapso_core::doc::{Kind, MAX_INDENT};
use synapso_core::sheet::{self, INDENT_SPACES, Line};
use synapso_core::table;

use crate::format::Edit;

/// Which line the caret is on, and the bytes that line occupies.
struct At {
    line: usize,
    start: usize,
    end: usize,
}

/// ⚠️ lines are counted with `split('\n')`, not `lines()` — that is how
/// `sheet::parse` indexes them, and an off-by-one between the two would read
/// every rule off the wrong line.
fn at(text: &str, caret: usize) -> At {
    let caret = caret.min(text.len());
    let start = text[..caret].rfind('\n').map_or(0, |nl| nl + 1);
    let end = text[caret..].find('\n').map_or(text.len(), |nl| caret + nl);
    At {
        line: text[..start].matches('\n').count(),
        start,
        end,
    }
}

/// The bytes a run of lines occupies: the first one's start to the last one's
/// end, with the newlines between them and none after.
fn span(text: &str, lines: Range<usize>) -> Range<usize> {
    let (mut start, mut end, mut off) = (0, text.len(), 0);
    for (i, line) in text.split('\n').enumerate() {
        if i == lines.start {
            start = off;
        }
        if i + 1 == lines.end {
            end = off + line.len();
        }
        off += line.len() + 1;
    }
    start..end
}

/// A caret inside a replacement, as an offset into the buffer: `base` is where
/// the replacement starts, `line`/`column` where the caret goes inside it.
fn caret_in(base: usize, new: &str, line: usize, column: usize) -> usize {
    let mut off = base;
    for (i, row) in new.split('\n').enumerate() {
        if i == line {
            return off + column.min(row.len());
        }
        off += row.len() + 1;
    }
    base + new.len()
}

/// ⏎. A table pads and grows, a fence closes, an empty item leaves the list,
/// and anything else carries its marker to the next line. `None` means the
/// editor's own Enter is what was wanted.
pub fn enter(text: &str, caret: usize) -> Option<Edit> {
    let lines = sheet::parse(text);
    let a = at(text, caret);
    if let Some(edit) = table_enter(text, &lines, &a) {
        return Some(edit);
    }
    // a fence that opens gets its closer, so the rest of the file does not
    // become code
    if let Some(edit) = close_fence(text, &lines, &a) {
        return Some(edit);
    }
    let line = lines.get(a.line)?;
    if let Some(edit) = leave_picture(text, line, &a) {
        return Some(edit);
    }
    let raw = text.get(a.start..a.end)?;
    let marker = line.marker.min(raw.len());
    let body = raw.get(marker..)?;

    // ⚠️ a list, and only a list: a rule and a picture are all marker and no
    // words, so "the item is empty" is true of them too — and eating the
    // marker there deletes the line itself.
    if line.kind.is_list() && body.trim().is_empty() && marker > 0 {
        // an empty item nested under another climbs out one level at a time —
        // leaving a list from three deep should not skip the two levels
        // somebody built
        if line.indent > 0
            && let Some(edit) = indent(text, &lines, &a, caret, true)
        {
            return Some(edit);
        }
        // and at the top level it ends the list: the marker goes, leaving the
        // blank line to carry on from rather than another dead bullet
        return Some(Edit {
            range: a.start..a.end,
            text: String::new(),
            caret: a.start,
        });
    }

    let carry = sheet::continuation(line, body);
    let text = format!("\n{carry}");
    Some(Edit {
        caret: caret + text.len(),
        range: caret..caret,
        text,
    })
}

/// ⇧⏎. Out of a fence rather than a line break inside it; on a table row it is
/// the same growth ⏎ does, because a plain break cuts the row in half and
/// `| c ` on its own is not a row any more.
pub fn exit(text: &str, caret: usize) -> Option<Edit> {
    let lines = sheet::parse(text);
    let a = at(text, caret);
    if let Some(edit) = lines.get(a.line).and_then(|l| leave_picture(text, l, &a)) {
        return Some(edit);
    }
    if matches!(lines.get(a.line).map(|l| &l.kind), Some(Kind::Code { .. })) {
        // ⚠️ the way out is the END OF THE RUN, not the next line that looks
        // like a fence: searching forward finds the OPENER of an unrelated
        // later block whenever the caret is already on this one's closer.
        let mut end = a.line;
        while end + 1 < lines.len() && matches!(lines[end + 1].kind, Kind::Code { .. }) {
            end += 1;
        }
        let out = span(text, end..end + 1).end;
        return Some(Edit {
            range: out..out,
            text: "\n".into(),
            caret: out + 1,
        });
    }
    table_enter(text, &lines, &a)
}

/// ⇥ / ⇧⇥. A table's cells first, then a list item's nesting. `None` leaves the
/// key to the editor — which inside a fence is right: there a tab IS what was
/// meant, because it is code.
pub fn tab(text: &str, caret: usize, forward: bool) -> Option<Edit> {
    let lines = sheet::parse(text);
    let a = at(text, caret);
    if matches!(lines.get(a.line).map(|l| &l.kind), Some(Kind::Code { .. })) {
        return None;
    }
    table_cell(text, &lines, &a, caret, forward)
        .or_else(|| indent(text, &lines, &a, caret, !forward))
}

/// Whether the caret is inside a fenced block — where a tab is code, and the
/// only place the character itself is what was meant.
pub fn in_code(text: &str, caret: usize) -> bool {
    let a = at(text, caret);
    matches!(
        sheet::parse(text).get(a.line).map(|l| &l.kind),
        Some(Kind::Code { .. })
    )
}

/// ⌘⏎: a task flips, anything else that can hold a checkbox becomes one.
pub fn toggle_task(text: &str, caret: usize) -> Option<Edit> {
    let lines = sheet::parse(text);
    let a = at(text, caret);
    let next = match &lines.get(a.line)?.kind {
        Kind::Task(done) => Kind::Task(!done),
        Kind::Paragraph | Kind::Bullet | Kind::Ordered => Kind::Task(true),
        // the kind carries data a task has nowhere to put: an image would drop
        // its path, a fence its language, a rule has no text at all
        _ => return None,
    };
    crate::format::set_kind(text, caret, &next)
}

/// ⏎ inside a table: the row is finished, so the columns are padded to fit what
/// was typed and a fresh row opens under it. Every face here is monospace, so
/// padded cells *are* the grid.
fn table_enter(text: &str, lines: &[Line], a: &At) -> Option<Edit> {
    let Some(run) = sheet::table_run(lines, a.line) else {
        // a header row on its own — still the paragraph a row alone is
        // (`sheet::parse`): ⏎ builds the divider and the first row under it,
        // so nobody types a line of dashes by hand
        let header = text.get(a.start..a.end)?;
        if lines.get(a.line)?.kind != Kind::Paragraph
            || !sheet::is_table(header.trim_start_matches(' '))
            || table::cells(header).iter().all(|c| c.trim().is_empty())
        {
            return None;
        }
        let built = table::from_header(header).trim_end().to_string();
        let column = built
            .split('\n')
            .nth(2)
            .map_or(0, |row| table::cell_start(row, 0));
        return Some(Edit {
            caret: caret_in(a.start, &built, 2, column),
            range: a.start..a.end,
            text: built,
        });
    };
    let rows: Vec<&str> = text.split('\n').collect();

    // ⚠️ ask before touching the grid: `align` rewrites row 1 as the divider
    // whatever is sitting there, so a run of pipe-shaped lines with no divider
    // under the header would lose its second row outright.
    if !table::is_whole(&rows[run.clone()]) {
        return None;
    }
    if table::cells(rows[a.line])
        .iter()
        .all(|cell| cell.trim().is_empty())
    {
        // the pipes go, and what is left is the blank line to carry on from
        return Some(Edit {
            range: a.start..a.end,
            text: String::new(),
            caret: a.start,
        });
    }

    let mut grown: Vec<String> = rows[run.clone()].iter().map(|r| (*r).to_string()).collect();
    let columns = table::columns(&grown.join("\n"));
    // ⚠️ the divider owns its place: ⏎ on the header cannot open a row above
    // it, or the table stops being one
    let under = if a.line == run.start {
        2
    } else {
        a.line - run.start + 1
    };
    grown.insert(under.min(grown.len()), table::blank_row(columns));
    let aligned = table::align(&grown.join("\n"));
    let column = aligned
        .split('\n')
        .nth(under)
        .map_or(0, |row| table::cell_start(row, 0));
    let where_ = span(text, run);
    Some(Edit {
        caret: caret_in(where_.start, &aligned, under, column),
        range: where_,
        text: aligned,
    })
}

/// A fence that opens and never closes: write its closer and open the caret
/// between the two. ⚠️ the closer is the fence that was typed, character for
/// character — a ```` ```` ```` block must close on four backticks or every
/// other reader swallows the rest of the file.
fn close_fence(text: &str, lines: &[Line], a: &At) -> Option<Edit> {
    let Kind::Code { fence, .. } = &lines.get(a.line)?.kind else {
        return None;
    };
    // the OPENING line, and only that: a closer is a `Code` line too
    if a.line > 0 && matches!(lines[a.line - 1].kind, Kind::Code { .. }) {
        return None;
    }
    // ⚠️ whether this opener has a closer is NOT a question about the run of
    // `Code` lines under it. Markdown pairs fences in order, so an opener
    // typed above an existing block steals THAT block's opener as its closer,
    // and `parse` duly reports a run that closes — while everything between
    // the two turns into code with no way out but typing the closer by hand.
    // Ask the rest of the file instead: if it ends inside a fence, one of the
    // fences down there is already this one's closer.
    if sheet::ends_inside_a_fence(text.get(a.end..)?) {
        return None;
    }
    let opener = text.get(a.start..a.end)?;
    Some(Edit {
        caret: a.end + 1,
        range: a.start..a.end,
        text: format!("{opener}\n\n{fence}"),
    })
}

/// ⏎ on a picture: the line UNDER it opens, and the caret goes there.
///
/// ⚠️ a picture is one line by definition — `![desc](src)` cannot wrap — so a
/// break typed into its description would cut the picture in two and leave
/// half a tag as words. `None` off a picture, so the callers carry on.
fn leave_picture(text: &str, line: &sheet::Line, a: &At) -> Option<Edit> {
    if !matches!(line.kind, Kind::Image(_)) {
        return None;
    }
    let end = a.end.min(text.len());
    Some(Edit {
        range: end..end,
        text: "\n".into(),
        caret: end + 1,
    })
}

/// The description inside a picture's `![…](src)`, as buffer offsets.
///
/// ⚠️ between the brackets and nowhere else. A picture line is ALL syntax —
/// `sheet::parse` gives it `marker = raw.len()` — so a caret on one has no
/// text of its own to sit in, and typing has to land in the description rather
/// than in the middle of a path nobody can see.
pub fn picture_alt(text: &str, caret: usize) -> Option<Range<usize>> {
    let a = at(text, caret);
    if !matches!(sheet::parse(text).get(a.line)?.kind, Kind::Image(_)) {
        return None;
    }
    let raw = text.get(a.start..a.end)?;
    let open = raw.find("![")? + 2;
    // the FIRST `](`, which is what `doc::image_line` splits on — a `]` typed
    // into the description does not move the boundary
    let close = raw[open..].find("](")? + open;
    Some(a.start + open..a.start + close)
}

/// One typed character, when the caret is on a picture: into the description.
///
/// ⚠️ where the caret IS, when the caret is in the description — it is a real
/// caret in a real run now (`shape::shapes` makes the description a cell), so
/// typing in the middle of a word has to land in the middle of that word. From
/// anywhere else on the line there is no honest place but the end.
pub fn describe_picture(text: &str, caret: usize, typed: &str) -> Option<Edit> {
    let alt = picture_alt(text, caret)?;
    let at = if (alt.start..=alt.end).contains(&caret) {
        caret
    } else {
        alt.end
    };
    Some(Edit {
        range: at..at,
        text: typed.to_string(),
        caret: at + typed.len(),
    })
}

/// ⌫ around a picture. Three cases, and the middle one is the whole point:
///
/// - in a picture's description — take the character before the caret, the way
///   ⌫ works in any caption field. At the very start of one there is nothing
///   left to take, and the picture is what goes.
/// - on a picture with none — the picture goes, whole. There is nothing else
///   left on the line to take back.
/// - at the very start of a NON-EMPTY line under one — the picture goes,
///   whole. This is how you reach a picture with ⌫ at all, and eating just
///   the newline would glue that paragraph onto the end of a URL.
///
/// ⚠️ a BLANK line under a picture is none of the three: there is nothing on
/// it to glue anywhere, so ⌫ there is the ordinary one and the caret lands
/// back in the description. Taking the picture instead made "close this gap"
/// delete the thing the gap was under.
pub fn erase_picture(text: &str, caret: usize) -> Option<Edit> {
    let lines = sheet::parse(text);
    let a = at(text, caret);
    let line = if matches!(lines.get(a.line)?.kind, Kind::Image(_)) {
        // ⚠️ the character before the CARET, not the last one written: the
        // caret is real in the description now (it is a cell of the plan) and
        // can be standing anywhere in it. At the very START of one there is
        // nothing left in it to take back, and what is left is the picture.
        let alt = picture_alt(text, caret)?;
        let to = caret.clamp(alt.start, alt.end);
        if to > alt.start {
            let last = text[..to].chars().next_back()?;
            let from = to - last.len_utf8();
            return Some(Edit {
                range: from..to,
                text: String::new(),
                caret: from,
            });
        }
        a.line
    } else if caret == a.start
        && a.line > 0
        // ⚠️ and the line has something ON it. Taking the picture is the
        // guard against gluing a paragraph onto the end of a URL, and a BLANK
        // line has nothing to glue: eating the picture there destroys the
        // thing you were about to write next to, on a keystroke that plainly
        // means "close this gap". The plain ⌫ takes the newline instead and
        // the caret comes to rest in the description.
        && a.end > a.start
        && matches!(lines.get(a.line - 1)?.kind, Kind::Image(_))
    {
        a.line - 1
    } else {
        return None;
    };
    let at = span(text, line..line + 1);
    // the newline AFTER it goes with the line, so nothing is left standing
    // where the picture was. At the end of the file there is none, and the one
    // before it goes instead rather than leaving a blank line behind.
    let range = if at.end < text.len() {
        at.start..at.end + 1
    } else {
        at.start.saturating_sub(1)..at.end
    };
    Some(Edit {
        caret: range.start,
        range,
        text: String::new(),
    })
}

/// ⇥ / ⇧⇥ on a list item. Indent is leading spaces, so nesting is an edit like
/// any other — and the rule is the block editor's: an item can only nest under
/// one that is already there, never more than a level below it.
fn indent(text: &str, lines: &[Line], a: &At, caret: usize, out: bool) -> Option<Edit> {
    let line = lines.get(a.line)?;
    if !line.kind.is_list() {
        return None;
    }
    let level = line.indent;
    let next = if out {
        level.saturating_sub(1)
    } else {
        let ceiling = a
            .line
            .checked_sub(1)
            .and_then(|i| lines.get(i))
            .filter(|above| above.kind.is_list())
            .map_or(0, |above| (above.indent + 1).min(MAX_INDENT));
        if ceiling > level {
            level + 1
        } else {
            return None;
        }
    };
    if next == level {
        return None;
    }
    let body = text.get(a.start + line.spaces..a.end)?;
    let pad = " ".repeat(next as usize * INDENT_SPACES);
    // ⚠️ the caret rides the difference in SPACES, not in levels: `indent` is
    // `spaces / 2`, so a line opening with an odd run of them would move by one
    // column less than its words did
    let shift = pad.len() as isize - line.spaces as isize;
    let column = (caret - a.start).saturating_add_signed(shift);
    let replaced = format!("{pad}{body}");
    Some(Edit {
        caret: a.start + column.min(replaced.len()),
        range: a.start..a.end,
        text: replaced,
    })
}

/// ⇥ / ⇧⇥ inside a table: the next cell, with the columns snapped to what has
/// been typed. ⇥ off the last cell grows the table — the fastest way anyone
/// fills one in.
fn table_cell(text: &str, lines: &[Line], a: &At, caret: usize, forward: bool) -> Option<Edit> {
    let run = sheet::table_run(lines, a.line)?;
    let all: Vec<&str> = text.split('\n').collect();
    // ⚠️ same question as in `table_enter`, same reason
    if !table::is_whole(&all[run.clone()]) {
        return None;
    }
    let mut rows: Vec<String> = all[run.clone()].iter().map(|r| (*r).to_string()).collect();
    let columns = table::columns(&rows.join("\n"));
    let here = a.line - run.start;
    let cell = table::cell_at(&rows[here], caret - a.start);

    // ⚠️ the divider is scaffolding: a click can drop the caret into the
    // dashes, so the row is left from wherever in it the caret stands —
    // stepping cell by cell would walk it along the dashes.
    let (mut row, cell) = if here == 1 {
        if forward {
            (2, 0)
        } else {
            (0, columns.saturating_sub(1))
        }
    } else if forward {
        match cell + 1 {
            next if next < columns => (here, next),
            _ => (if here == 0 { 2 } else { here + 1 }, 0),
        }
    } else {
        match cell.checked_sub(1) {
            Some(previous) => (here, previous),
            None if here == 0 => (0, 0),
            None => (
                if here == 2 { 0 } else { here - 1 },
                columns.saturating_sub(1),
            ),
        }
    };
    if row >= rows.len() {
        rows.push(table::blank_row(columns));
        row = rows.len() - 1;
    }
    let aligned = table::align(&rows.join("\n"));
    let column = aligned
        .split('\n')
        .nth(row)
        .map_or(0, |r| table::cell_start(r, cell));
    let where_ = span(text, run);
    Some(Edit {
        caret: caret_in(where_.start, &aligned, row, column),
        range: where_,
        text: aligned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buffer after the edit, and where the caret ended up in it.
    fn applied(text: &str, edit: &Edit) -> (String, usize) {
        let mut out = text.to_string();
        out.replace_range(edit.range.clone(), &edit.text);
        assert!(
            edit.caret <= out.len() && out.is_char_boundary(edit.caret),
            "the caret has to land inside the text it made: {} of {}",
            edit.caret,
            out.len()
        );
        (out, edit.caret)
    }

    /// Where the caret is, spelled out: the text with a `|` in it.
    fn shown(text: &str, edit: &Edit) -> String {
        let (out, caret) = applied(text, edit);
        format!("{}|{}", &out[..caret], &out[caret..])
    }

    /// A table nested in a list item. ⇥ and ⏎ both rebuild the whole run and
    /// `span` replaces those lines whole — so the realigned table came back at
    /// the margin and the table left the list, in the FILE, on one keystroke.
    #[test]
    fn tab_and_enter_leave_a_nested_table_inside_its_list_item() {
        let text = "- item\n  | a | b |\n  | --- | --- |\n  | 1 | 2 |";
        let caret = text.find("a |").unwrap();

        let (after, _) = applied(text, &tab(text, caret, true).unwrap());
        assert!(
            after.lines().skip(1).all(|row| row.starts_with("  |")),
            "⇥ moved the table out of the list:\n{after}"
        );
        assert!(after.starts_with("- item\n"), "{after}");

        let (after, _) = applied(text, &enter(text, caret).unwrap());
        assert!(
            after.lines().skip(1).all(|row| row.starts_with("  |")),
            "⏎ moved the table out of the list:\n{after}"
        );
        // the row ⏎ opened is in there too, and inside the item
        assert_eq!(after.lines().count(), 5, "{after}");
    }

    #[test]
    fn a_list_carries_on_and_an_empty_item_ends_it() {
        let text = "- one";
        let edit = enter(text, text.len()).unwrap();
        assert_eq!(shown(text, &edit), "- one\n- |");

        // the next ⏎, with nothing typed into the item
        let text = "- one\n- ";
        let edit = enter(text, text.len()).unwrap();
        assert_eq!(shown(text, &edit), "- one\n|");
    }

    #[test]
    fn a_task_carries_an_open_box_and_a_number_counts_up() {
        let text = "- [x] done";
        assert_eq!(
            shown(text, &enter(text, text.len()).unwrap()),
            "- [x] done\n- [ ] |"
        );
        let text = "1. one\n2. two";
        assert_eq!(
            shown(text, &enter(text, text.len()).unwrap()),
            "1. one\n2. two\n3. |"
        );
    }

    /// ⚠️ the regression this guard exists for: a rule and a picture are all
    /// marker and no words, so "the item is empty" is true of them too.
    #[test]
    fn a_rule_and_a_picture_are_not_empty_list_items() {
        for text in ["---", "![alt](p.png)"] {
            let edit = enter(text, text.len()).unwrap();
            let (out, _) = applied(text, &edit);
            assert_eq!(out, format!("{text}\n"), "{text:?} must survive ⏎");
        }
    }

    #[test]
    fn an_item_nested_deep_climbs_out_one_level_at_a_time() {
        let text = "- one\n  - two\n    - ";
        let edit = enter(text, text.len()).unwrap();
        assert_eq!(shown(text, &edit), "- one\n  - two\n  - |");
    }

    #[test]
    fn tab_nests_only_under_an_item_that_is_already_there() {
        // nothing above it: the top level is as far left as it goes, and ⇥
        // cannot invent a parent
        let text = "- only";
        assert!(tab(text, text.len(), true).is_none());

        let text = "- one\n- two";
        let edit = tab(text, text.len(), true).unwrap();
        assert_eq!(shown(text, &edit), "- one\n  - two|");

        // and back out again
        let text = "- one\n  - two";
        let edit = tab(text, text.len(), false).unwrap();
        assert_eq!(shown(text, &edit), "- one\n- two|");
    }

    #[test]
    fn inside_a_fence_the_tab_is_left_to_the_editor() {
        let text = "```rust\nfn main() {}\n```";
        assert!(tab(text, 12, true).is_none(), "there a tab is code");
    }

    #[test]
    fn a_fence_that_opens_gets_its_closer_and_keeps_its_length() {
        let text = "````\n";
        let edit = enter(text, 4).unwrap();
        assert_eq!(shown(text, &edit), "````\n|\n````\n");

        // one that already closes needs nothing
        let text = "```\ncode\n```";
        assert!(close_fence(text, &sheet::parse(text), &at(text, 3)).is_none());
    }

    /// ⚠️ the bug this exists for: an opener typed ABOVE an existing block was
    /// left unclosed, because `parse` pairs it with that block's opener and so
    /// reports a run that "already closes". Everything between the two became
    /// code, and the only way out was typing the closer by hand.
    #[test]
    fn an_opener_typed_above_another_block_still_gets_its_own_closer() {
        let text = "words\n```\n\n```rs\ncode\n```";
        let (out, caret) = applied(text, &enter(text, 9).unwrap());
        assert_eq!(out, "words\n```\n\n```\n\n```rs\ncode\n```");
        assert_eq!(
            &out[..caret],
            "words\n```\n",
            "the caret is inside the new block"
        );
        assert_eq!(out.matches("```").count(), 4, "one pair added, none stolen");

        // and the block below is untouched: its own opener is still an opener,
        // so `code` is still code — which is what it stopped being
        let kinds: Vec<_> = sheet::parse(&out).into_iter().map(|l| l.kind).collect();
        assert_eq!(kinds[4], Kind::Paragraph, "the new block let go");
        assert!(
            matches!(kinds[6], Kind::Code { .. }),
            "`code` is still code"
        );
    }

    #[test]
    fn a_header_row_builds_its_own_divider_and_first_row() {
        let text = "| a | bbbb |";
        let (out, caret) = applied(text, &enter(text, text.len()).unwrap());
        let rows: Vec<&str> = out.split('\n').collect();
        assert_eq!(rows.len(), 3, "header, divider, row");
        assert!(table::is_divider(rows[1]));
        // the caret opens in the new row's first cell
        assert!(caret > out.find('\n').unwrap() + rows[1].len());
        assert_eq!(out.as_bytes()[caret - 1], b' ');
    }

    #[test]
    fn enter_in_a_table_pads_the_columns_and_opens_a_row() {
        let text = "| a | bbbb |\n| --- | --- |\n| ccc | d |\n";
        let edit = enter(text, text.find("ccc").unwrap()).unwrap();
        let (out, _) = applied(text, &edit);
        let rows: Vec<&str> = out.split('\n').collect();
        assert_eq!(rows.len(), 5, "the run grew by a row, the blank line stays");
        // the grid is padded: every row is the same width
        let widths: Vec<usize> = rows[..4].iter().map(|r| r.len()).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "padded to one width: {widths:?}"
        );

        // and a row with nothing in it ends the table instead of growing it
        let text = "| a | b |\n| - | - |\n|  |  |";
        let edit = enter(text, text.len() - 1).unwrap();
        let (out, _) = applied(text, &edit);
        assert_eq!(out, "| a | b |\n| - | - |\n");
    }

    #[test]
    fn tab_walks_the_cells_and_grows_the_table_off_the_last_one() {
        let text = "| a | b |\n| - | - |\n| c | d |";
        let first = text.rfind('c').unwrap();
        let edit = tab(text, first, true).unwrap();
        let (out, caret) = applied(text, &edit);
        assert_eq!(&out[caret..caret + 1], "d", "the next cell along");

        // ⇥ off the last cell opens a row under it, caret in its FIRST cell
        let last = text.rfind('d').unwrap();
        let (out, caret) = applied(text, &tab(text, last, true).unwrap());
        let rows: Vec<&str> = out.split('\n').collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            caret,
            out.rfind('\n').unwrap() + 1 + table::cell_start(rows[3], 0)
        );
    }

    /// ⚠️ the caret can be clicked into the divider's dashes; stepping cell by
    /// cell from there would walk it along them.
    #[test]
    fn tab_out_of_the_divider_lands_in_a_real_row() {
        let text = "| a | b |\n| - | - |\n| c | d |";
        let divider = text.find("| -").unwrap() + 3;
        let (out, caret) = applied(text, &tab(text, divider, true).unwrap());
        assert_eq!(&out[caret..caret + 1], "c", "forward, into the first row");
        let (out, caret) = applied(text, &tab(text, divider, false).unwrap());
        assert_eq!(&out[caret..caret + 1], "b", "back, into the header's last");
    }

    #[test]
    fn a_checkbox_flips_and_a_paragraph_grows_one() {
        let text = "- [ ] todo";
        let (out, _) = applied(text, &toggle_task(text, 8).unwrap());
        assert_eq!(out, "- [x] todo");
        let (back, _) = applied(&out, &toggle_task(&out, 8).unwrap());
        assert_eq!(back, "- [ ] todo");

        let text = "just words";
        let (out, _) = applied(text, &toggle_task(text, 2).unwrap());
        assert_eq!(out, "- [x] just words");

        // and the kinds with nowhere to put one refuse
        for text in ["---", "![a](p.png)", "| a | b |\n| c | d |"] {
            assert!(toggle_task(text, 1).is_none(), "{text:?}");
        }
    }

    #[test]
    fn shift_enter_leaves_the_fence_by_the_end_of_its_own_run() {
        // ⚠️ two blocks: the way out of the first is ITS closer, not the
        // opener of the second
        let text = "```\none\n```\n\n```\ntwo\n```";
        let edit = exit(text, text.find("one").unwrap()).unwrap();
        let (out, caret) = applied(text, &edit);
        assert_eq!(&out[..caret], "```\none\n```\n");
        assert_eq!(out.matches("```").count(), 4, "no fence was invented");
    }

    /// A picture line is ALL syntax — `marker = raw.len()` — so there is no
    /// text on it for a caret to stand in and none for a keystroke to join.
    /// Everything typed on one belongs to its description.
    #[test]
    fn typing_on_a_picture_writes_its_description() {
        let text = "words\n![](assets/shot.png)\nmore";
        let on = text.find("![").unwrap();
        assert_eq!(
            shown(text, &describe_picture(text, on, "b").unwrap()),
            "words\n![b|](assets/shot.png)\nmore"
        );

        // the caret lands inside the description, so the next one carries on
        let (out, caret) = applied(text, &describe_picture(text, on, "b").unwrap());
        assert_eq!(
            shown(&out, &describe_picture(&out, caret, "o").unwrap()),
            "words\n![bo|](assets/shot.png)\nmore"
        );

        // ⚠️ from anywhere on the line, including the middle of the path: there
        // it goes to the end, because there is nowhere honest for a caret to be
        let inside = text.find("shot").unwrap();
        assert_eq!(
            shown(text, &describe_picture(text, inside, "x").unwrap()),
            "words\n![x|](assets/shot.png)\nmore"
        );
        // and nothing on a line that is not one
        assert!(describe_picture(text, 0, "x").is_none());
    }

    /// ⌫ on the BLANK line under a picture takes the blank line, and the
    /// caret lands back in the description. The rule below exists so a
    /// paragraph is never glued onto the end of a URL — a blank line has
    /// nothing to glue, and eating the picture instead is a keystroke that
    /// destroys the thing you were about to type next to.
    #[test]
    fn backspace_on_the_blank_line_under_a_picture_goes_back_to_the_description() {
        let text = "one\n![a photo](assets/shot.png)\n\ntwo";
        let blank = text.find("\n\ntwo").unwrap() + 1;
        assert!(
            erase_picture(text, blank).is_none(),
            "the blank line under a picture is not the picture's to take"
        );

        // …so the plain ⌫ takes the newline, and the caret comes to rest at
        // the end of the picture line — which is drawn inside the description
        let caret = blank - 1;
        let after = format!("{}{}", &text[..caret], &text[blank..]);
        assert_eq!(after, "one\n![a photo](assets/shot.png)\ntwo");

        // and the NEXT ⌫ from there is the one that starts on the description
        assert_eq!(
            shown(&after, &erase_picture(&after, caret).unwrap()),
            "one\n![a phot|](assets/shot.png)\ntwo"
        );
    }

    /// ⚠️ the description is edited character by character, but the picture
    /// itself goes whole — including from the line under it, which is how ⌫
    /// reaches a picture at all.
    #[test]
    fn backspace_takes_the_description_apart_and_then_the_picture() {
        let text = "one\n![a photo](assets/shot.png)\ntwo";
        // ⚠️ the character before the CARET, wherever the caret is standing
        let end = text.find("](assets").unwrap();
        assert_eq!(
            shown(text, &erase_picture(text, end).unwrap()),
            "one\n![a phot|](assets/shot.png)\ntwo",
            "at the end of the description, its last character"
        );
        let mid = text.find("photo").unwrap();
        assert_eq!(
            shown(text, &erase_picture(text, mid).unwrap()),
            "one\n![a|photo](assets/shot.png)\ntwo",
            "in the middle, the one in front of it"
        );
        // and at the very start there is nothing left in it to take back
        let start = text.find("a photo").unwrap();
        assert_eq!(
            shown(text, &erase_picture(text, start).unwrap()),
            "one\n|two"
        );

        // with nothing left to take back, the picture is what goes
        let bare = "one\n![](assets/shot.png)\ntwo";
        assert_eq!(
            shown(
                bare,
                &erase_picture(bare, bare.find("![").unwrap()).unwrap()
            ),
            "one\n|two",
            "the line and the newline that held it up"
        );

        // and from the start of the line below, whatever the description says
        let below = text.find("two").unwrap();
        assert_eq!(
            shown(text, &erase_picture(text, below).unwrap()),
            "one\n|two"
        );

        // ⚠️ only at the START of that line — a ⌫ mid-word is a ⌫ mid-word
        assert!(erase_picture(text, below + 1).is_none());
        // and an ordinary line is left to the ordinary rule
        assert!(erase_picture(text, 2).is_none());
    }

    /// ⚠️ the reason the description is a cell of the plan and not something
    /// the painter draws: ⌘F counts what is DRAWN, and a description drawn
    /// nowhere but at paint time was text a reader can see and the bar could
    /// not find.
    #[test]
    fn the_description_is_where_the_buffer_says_it_is() {
        let text = "one\n![a gateway photo](assets/shot.png)\ntwo";
        let alt = picture_alt(text, text.find("gateway").unwrap()).unwrap();
        assert_eq!(&text[alt.clone()], "a gateway photo");
        assert_eq!(alt.start, text.find("a gateway").unwrap());

        // a `]` typed into a description does not move the boundary — the
        // split is on the first `](`, the way `doc::image_line` reads it
        let odd = "![a ] b](assets/shot.png)";
        assert_eq!(&odd[picture_alt(odd, 3).unwrap()], "a ] b");

        // and a line that is not a picture has no description at all
        assert!(picture_alt("plain words", 2).is_none());
        assert!(picture_alt("[a link](to.md)", 2).is_none());
    }

    /// The last line of a file has no newline after it; taking the one BEFORE
    /// it instead is what stops a picture at the end from leaving a blank line
    /// where it was.
    #[test]
    fn a_picture_at_the_end_of_the_file_leaves_no_gap() {
        let text = "words\n![](assets/shot.png)";
        let edit = erase_picture(text, text.len() - 1).unwrap();
        assert_eq!(shown(text, &edit), "words|");
    }

    #[test]
    fn a_caret_that_is_not_on_a_line_of_its_own_is_left_alone() {
        // an empty buffer is one empty paragraph, and every rule reads it that
        // way: ⏎ breaks the line, ⇥ has no list to nest, ⌘⏎ starts a task
        assert!(enter("", 0).is_some(), "⏎ on nothing is still a line break");
        assert!(tab("", 0, true).is_none());
        assert_eq!(shown("", &toggle_task("", 0).unwrap()), "- [x] |");
    }
}
