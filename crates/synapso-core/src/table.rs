//! Markdown tables, kept aligned.
//!
//! A table is one block over the raw pipes — the same deal the editor already
//! makes with `**bold**` (see [`crate::inline`]): the syntax stays on screen
//! and fades. What makes it read as a table is the padding. Every font in
//! this app is JetBrains Mono, so columns padded to their widest cell line
//! up and the pipes become the rules of the grid.
//!
//! Realigning happens when the caret leaves a cell — Tab, Enter, opening the
//! note — and never on a keystroke: padding that shifts under the caret
//! jitters, which is why org-mode does it the same way.

use crate::vault::Align;

/// A row is a line that opens with a pipe.
///
/// GFM also reads `a | b`, with no outer pipes, as a table. Wanting
/// the pipe is what keeps a paragraph that merely *contains* one a paragraph.
pub fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Whether these rows are a table at all — a header, a divider under it with
/// the same number of cells, and only then the body.
///
/// ⚠️ Everything that walks a grid has to ask first. [`align`] rewrites row 1
/// as the divider unconditionally, so running it over a run of pipe-shaped
/// lines that has no divider *replaces the second row with dashes* — which is
/// the normal state of a table while somebody is still typing it, and the
/// state after anyone deletes the divider line.
pub fn is_whole(rows: &[&str]) -> bool {
    rows.len() >= 2 && is_divider(rows[1]) && cells(rows[0]).len() == cells(rows[1]).len()
}

/// Which lines are table rows, given which ones are shaped like a row: only
/// those with another one right above or below. A row alone is a paragraph —
/// `| abc | xyz |` or `|test|` on a line of its own is a sentence with pipes
/// in it, and it becomes a table the moment a second row lands next to it.
///
/// ⚠️ Not [`opens`], GFM's header-and-divider, which is what
/// `doc::parse_blocks` reads. This is the rule for a note being DRAWN while
/// it is typed: a header that grows a column before its divider does would
/// flip the whole table back to text under the caret.
pub fn in_a_run(shaped: &[bool]) -> Vec<bool> {
    (0..shaped.len())
        .map(|i| shaped[i] && ((i > 0 && shaped[i - 1]) || shaped.get(i + 1) == Some(&true)))
        .collect()
}

/// The `| --- | :--: |` line under the header — the one that makes it a table.
pub fn is_divider(line: &str) -> bool {
    let cells = cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let body = cell.strip_prefix(':').unwrap_or(cell);
            let body = body.strip_suffix(':').unwrap_or(body);
            !body.is_empty() && body.bytes().all(|b| b == b'-')
        })
}

/// A table opens on two lines: a row, then a divider with the same number of
/// columns. GFM wants the counts to match and so does every other reader —
/// being lenient here would draw a table that GitHub renders as a paragraph.
pub fn opens(header: &str, divider: &str) -> bool {
    is_row(header) && is_divider(divider) && cells(header).len() == cells(divider).len()
}

/// The cells of a row, trimmed, with the outer pipes dropped.
pub fn cells(row: &str) -> Vec<&str> {
    let walls = walls(row);
    let mut out: Vec<&str> = walls
        .windows(2)
        .map(|pair| row[pair[0] + 1..pair[1]].trim())
        .collect();
    // a row that never closes its last cell still has one: `| a | b`
    if let Some(last) = walls.last() {
        let tail = row[last + 1..].trim();
        if !tail.is_empty() {
            out.push(tail);
        }
    }
    out
}

/// Byte offsets of the pipes that are walls. `\|` is an escaped pipe *inside*
/// a cell, not a wall — splitting on one would grow a column out of somebody's
/// text and the realign would then bake it in. The highlighter fades what
/// this returns, so a wall is one thing in both.
pub fn walls(row: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut escaped = false;
    for (at, c) in row.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '|' => out.push(at),
            _ => {}
        }
    }
    out
}

/// The table with every column padded to its widest cell, `:---:` honoured.
///
/// Idempotent: aligning an aligned table gives it back unchanged, which is
/// what lets this run on every Tab without the file churning.
pub fn align(table: &str) -> String {
    let grid: Vec<Vec<&str>> = table.lines().map(cells).collect();
    if grid.len() < 2 {
        return table.to_string();
    }
    let rules: Vec<Option<Align>> = grid[1].iter().map(|cell| rule_of(cell)).collect();
    let columns = grid.iter().map(Vec::len).max().unwrap_or(0);
    // `---` is the divider's floor, never its width: it stretches to whatever
    // the cells above and below it need
    let mut widths = vec![3usize; columns];
    for (i, row) in grid.iter().enumerate() {
        if i == 1 {
            continue;
        }
        for (c, cell) in row.iter().enumerate() {
            widths[c] = widths[c].max(cell.chars().count());
        }
    }

    // ⚠️ a table can live inside a list item, and everything before the first
    // wall is that item's indentation. Every row here is rebuilt from `"| "`,
    // so without this one ⇥ or ⏎ in a cell moved the whole table out of the
    // list in the FILE — `span` replaces the lines whole, indentation and all.
    let indent = table
        .lines()
        .next()
        .map_or("", |first| &first[..first.len() - first.trim_start().len()]);

    let mut out = String::new();
    for (i, row) in grid.iter().enumerate() {
        out.push_str(indent);
        for (c, width) in widths.iter().enumerate() {
            let rule = rules.get(c).copied().flatten();
            out.push_str("| ");
            if i == 1 {
                out.push_str(&divider(rule, *width));
            } else {
                // a short row gets the columns it was missing, so the grid
                // stays rectangular instead of ending mid-air
                out.push_str(&pad(row.get(c).copied().unwrap_or(""), *width, rule));
            }
            out.push(' ');
        }
        out.push_str("|\n");
    }
    out.pop();
    out
}

/// The `|:--:|` rule for one divider cell: which way its column's text sits.
/// Public because a renderer that draws the cells itself has to place them —
/// the padding this module writes only aligns the text for a reader that sees
/// the pipes.
pub fn rule_of(cell: &str) -> Option<Align> {
    match (cell.starts_with(':'), cell.ends_with(':')) {
        (true, true) => Some(Align::Center),
        (true, false) => Some(Align::Left),
        (false, true) => Some(Align::Right),
        (false, false) => None,
    }
}

fn divider(rule: Option<Align>, width: usize) -> String {
    let dashes = |n: usize| "-".repeat(n);
    match rule {
        None => dashes(width),
        Some(Align::Left) => format!(":{}", dashes(width - 1)),
        Some(Align::Right) => format!("{}:", dashes(width - 1)),
        Some(Align::Center) => format!(":{}:", dashes(width - 2)),
    }
}

fn pad(cell: &str, width: usize, rule: Option<Align>) -> String {
    // a char is a column. True for the notes anyone writes here and
    // for the whole ASCII the syntax is made of; CJK and emoji are the known
    // ceiling, and a table with one in it lands a column off.
    let slack = width.saturating_sub(cell.chars().count());
    let spaces = |n: usize| " ".repeat(n);
    match rule.unwrap_or(Align::Left) {
        Align::Left => format!("{cell}{}", spaces(slack)),
        Align::Right => format!("{}{cell}", spaces(slack)),
        Align::Center => format!("{}{cell}{}", spaces(slack / 2), spaces(slack - slack / 2)),
    }
}

/// The table a header row opens: the row, the divider that makes it one, and
/// an empty row to type into — what Enter on `| a | b |` leaves behind.
pub fn from_header(header: &str) -> String {
    let columns = cells(header).len().max(1);
    let divider = format!("|{}", " --- |".repeat(columns));
    align(&format!("{header}\n{divider}\n{}", blank_row(columns)))
}

/// How many cells the widest row of this table has — what a new row needs.
pub fn columns(table: &str) -> usize {
    table.lines().map(|row| cells(row).len()).max().unwrap_or(0)
}

/// An empty row of `columns` cells.
pub fn blank_row(columns: usize) -> String {
    format!("|{}", " |".repeat(columns.max(1)))
}

/// The cell the caret is in, counting from zero. A caret sitting on a wall
/// belongs to the cell that wall opens, so Tab out of it goes forward.
///
/// ⚠️ `byte`, not a character count: what the shell hands back as the cursor's
/// `column` is a byte index into the line (cosmic-text's `Cursor::index`), so
/// a `ç` above the caret shifts it by two.
pub fn cell_at(row: &str, byte: usize) -> usize {
    walls(row)
        .iter()
        .take_while(|wall| **wall < byte)
        .count()
        .saturating_sub(1)
}

/// Where cell `n`'s text starts, as a **byte** offset — after the wall and
/// after the padding, so the caret lands on the words and not on the
/// scaffolding. Bytes because that is what `text_editor::Position::column`
/// is, and putting the caret in a cell is a `move_to`.
pub fn cell_start(row: &str, n: usize) -> usize {
    let walls = walls(row);
    let Some(wall) = walls.get(n) else {
        return row.len();
    };
    let stop = walls.get(n + 1).copied().unwrap_or(row.len());
    let text = &row[wall + 1..stop];
    // an empty cell is all padding, and the far wall is no place for a caret:
    // it goes just inside, where the first letter will land
    let lead = if text.trim().is_empty() {
        1.min(text.len())
    } else {
        text.len() - text.trim_start().len()
    };
    wall + 1 + lead
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "| Peça | Qtd | Preço |\n| --- | --- | ---: |\n| Filtro | 2 | 38,00 |";

    #[test]
    fn a_table_opens_on_a_row_and_the_divider_under_it() {
        assert!(opens("| a | b |", "| --- | :-: |"));
        assert!(opens("| a | b |", "|---|---|"));
        // one column short: GFM reads this as two paragraphs, and so do we
        assert!(!opens("| a | b |", "| --- |"));
        assert!(!opens("| a | b |", "| --- | x |"));
        assert!(!opens("a | b", "| --- | --- |"));
        // a lone rule is a rule, not a divider
        assert!(!is_divider("---"));
        assert!(!is_divider("| |"));
    }

    #[test]
    fn an_escaped_pipe_is_text_and_never_a_column() {
        assert_eq!(cells(r"| a \| b | c |"), vec![r"a \| b", "c"]);
        assert_eq!(columns(r"| a \| b | c |"), 2);
    }

    #[test]
    fn aligning_pads_every_column_to_its_widest_cell() {
        assert_eq!(
            align(TABLE),
            "| Peça   | Qtd | Preço |\n\
             | ------ | --- | ----: |\n\
             | Filtro | 2   | 38,00 |"
        );
        // ⚠️ run on every Tab — a second pass may not move a thing, or the
        // file would churn and the caret with it
        assert_eq!(align(&align(TABLE)), align(TABLE));
    }

    #[test]
    fn a_divider_left_long_does_not_hold_the_column_open() {
        // ⚠️ the dashes are not content: measured with the cells they would
        // pin a column open at whatever width someone once typed
        assert_eq!(
            align("| a | b |\n| -------- | --- |\n| x | y |"),
            "| a   | b   |\n| --- | --- |\n| x   | y   |"
        );
    }

    #[test]
    fn the_divider_says_where_the_words_sit() {
        let table = "| a | b | c |\n| :-- | :-: | --: |\n| xxxx | yyyy | zzzz |";
        assert_eq!(
            align(table),
            "| a    |  b   |    c |\n\
             | :--- | :--: | ---: |\n\
             | xxxx | yyyy | zzzz |"
        );
    }

    #[test]
    fn a_short_row_gets_the_columns_it_was_missing() {
        let ragged = "| a | b |\n| --- | --- |\n| x |";
        assert_eq!(align(ragged), "| a   | b   |\n| --- | --- |\n| x   |     |");
    }

    #[test]
    fn the_caret_lands_on_the_words_of_the_cell_it_was_sent_to() {
        let row = "| Filtro | 2   |  38,00 |";
        assert_eq!(cell_start(row, 0), 2); // on the F
        assert_eq!(cell_start(row, 1), 11); // on the 2
        assert_eq!(cell_start(row, 2), 18); // past the right-aligned padding
        assert_eq!(&row[18..19], "3");

        // cell_at reads bytes back off the cursor; this row is all ASCII, so
        // the two units line up here and part ways in `um_ç_no_meio`
        assert_eq!(cell_at(row, 2), 0);
        assert_eq!(cell_at(row, 0), 0); // the row's first wall is still cell 0
        assert_eq!(cell_at(row, 11), 1);
        assert_eq!(cell_at(row, 18), 2);
        // "| Peça | Qtd |": the caret on the Q reads back as byte 10, and the
        // Q is the 9th character — one unit each way, both of them right
        assert_eq!(cell_at("| Peça | Qtd |", 10), 1);
        // ⚠️ 10 and not 9: `ç` spends two bytes, and this is a byte offset
        assert_eq!(cell_start("| Peça | Qtd |", 1), 10);

        // an empty cell is all padding: the caret goes just inside it, never
        // out at the far wall
        assert_eq!(cell_start("| |", 0), 2);
        assert_eq!(cell_start("|      |     |", 0), 2);
        assert_eq!(cell_start("|      |     |", 1), 9);
        assert_eq!(blank_row(3), "| | | |");
        assert_eq!(cells(&blank_row(3)).len(), 3);
    }

    /// A table can live inside a list item. Every row here is rebuilt from
    /// `"| "`, and `behave::span` replaces the lines whole — so one ⇥ or ⏎ in
    /// a cell moved the table out of the list in the file, one level up.
    #[test]
    fn an_aligned_table_keeps_the_indentation_it_came_with() {
        let indented = "  | a | b |\n  | --- | --- |\n  | 1 | 2 |";
        let aligned = align(indented);
        assert!(
            aligned.lines().all(|row| row.starts_with("  |")),
            "the table left the list:\n{aligned}"
        );
        // the cells are still aligned inside that indentation
        assert!(aligned.contains("| a "), "{aligned}");
        // and still idempotent, indentation and all — this runs on every Tab
        assert_eq!(align(&aligned), aligned);

        // a table at the margin is untouched
        let flat = "| a | b |\n| --- | --- |\n| 1 | 2 |";
        assert!(align(flat).lines().all(|row| row.starts_with("| ")));
        // a blank row inserted by `table_enter` has no indentation of its own
        // and still lands inside the list
        let grown = format!("  | a | b |\n  | --- | --- |\n{}", blank_row(2));
        assert!(
            align(&grown).lines().all(|row| row.starts_with("  |")),
            "{}",
            align(&grown)
        );
    }

    /// A row alone is a paragraph: `| abc | xyz |` or `|test|` on a line of
    /// its own is a sentence with pipes in it, and it turns into a table the
    /// moment a second row lands next to it — above or below.
    #[test]
    fn a_row_alone_is_a_paragraph_and_a_second_row_makes_the_table() {
        assert_eq!(in_a_run(&[true]), vec![false], "a lone row");
        assert_eq!(in_a_run(&[false, true, false]), vec![false; 3]);
        // the header being typed: still alone, still words
        assert_eq!(in_a_run(&[true, false]), vec![false, false]);
        // the second row lands under it, and both are the table
        assert_eq!(in_a_run(&[true, true]), vec![true, true]);
        assert_eq!(
            in_a_run(&[false, true, true, true, false, true]),
            vec![false, true, true, true, false, false],
            "a lone row after a table is not that table's"
        );
        assert!(in_a_run(&[]).is_empty());
    }
}
