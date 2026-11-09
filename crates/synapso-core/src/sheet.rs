//! The note as ONE buffer — the half of it that is only reading.
//!
//! A block editor gives every block its own widget, and a shell scopes a
//! selection, a drag and a caret to the widget that owns them — so crossing a
//! block is a bridge somebody has to build and keep repairing. Here the whole
//! note is a single buffer holding its raw markdown, and what used to be a
//! block is a *line kind*, derived from the text on every edit — nothing to
//! keep in sync, because nothing is stored twice.
//!
//! [`parse`] is that derivation, and it is the whole model: one [`Line`] per
//! line of the file, saying what the line is, where its opening syntax ends,
//! and how deep it is nested. The markdown that opens a line (`## `, `- `,
//! `> `, `- [ ] `) stays in the buffer, where the file needs it — a shell draws
//! it at a size of nothing and paints the bullet, the number and the checkbox
//! into the space it leaves.
//!
//! ⚠️ Nothing here knows how tall a row is or which font face it wears. That
//! needs the text layout engine, so it belongs to whichever shell owns one.

use std::path::Path;

use crate::doc::Kind;

/// Two spaces per Tab press, the way the file already writes nesting.
pub const INDENT_SPACES: usize = 2;

/// One line of the buffer, as a renderer needs to see it.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: Kind,
    /// Leading spaces, in bytes. Kept at full size — they are invisible
    /// already, and they are what indents a nested item.
    pub spaces: usize,
    /// End of the syntax that opens the line, in bytes: `spaces..marker` is
    /// what gets drawn at nothing.
    pub marker: usize,
    /// What an `Ordered` line shows. Positional, never what was typed.
    pub ordinal: usize,
    /// Nesting, in Tab presses.
    pub indent: u8,
    /// The size a picture is drawn at, once its file has been measured. `None`
    /// on every other line — and on an image whose file is not there, which
    /// [`measure`] turns back into the paragraph its markdown reads as.
    pub picture: Option<kurbo::Size>,
}

impl Line {
    /// Marker columns that still take up room. Nought for everything the
    /// gutter draws a glyph for — the bullet stands where the `- ` was, and
    /// the row would be indented twice if the `- ` were also holding its
    /// place. A quote is the exception: `> > ` is drawn transparent at full
    /// size, because the *width* of the prefix is what steps each level of
    /// nesting to the right, the way a reader shows it.
    pub fn lead(&self) -> usize {
        match self.kind {
            Kind::Quote => self.marker - self.spaces,
            _ => 0,
        }
    }

    /// A paragraph with nothing but its indentation — what an unparsed line is,
    /// and what a renderer falls back to for a line the parse has not reached.
    pub fn plain(spaces: usize) -> Self {
        Self {
            kind: Kind::Paragraph,
            spaces,
            marker: spaces,
            ordinal: 0,
            indent: (spaces / INDENT_SPACES) as u8,
            picture: None,
        }
    }
}

/// The syntax that opens a line of this kind. Indentation is not part of it —
/// that is the line's own, and `Ordered` always writes `1.` because [`parse`]
/// numbers a list by position, never by what was typed.
pub fn marker_for(kind: &Kind) -> &'static str {
    match kind {
        Kind::Heading(1) => "# ",
        Kind::Heading(2) => "## ",
        Kind::Heading(3) => "### ",
        Kind::Heading(4) => "#### ",
        Kind::Heading(5) => "##### ",
        Kind::Heading(_) => "###### ",
        Kind::Bullet => "- ",
        Kind::Ordered => "1. ",
        Kind::Task(false) => "- [ ] ",
        Kind::Task(true) => "- [x] ",
        Kind::Quote => "> ",
        _ => "",
    }
}

/// Every line's kind, in one pass. Only a fence carries state across lines.
pub fn parse(markdown: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    // ordinals are positional: a run of `Ordered` lines at one indent counts up
    let mut counters: Vec<usize> = Vec::new();

    let mut raws = markdown.split('\n').peekable();
    while let Some(raw) = raws.next() {
        let spaces = raw.len() - raw.trim_start_matches(' ').len();
        let body = &raw[spaces..];

        if let Some(open) = &fence {
            let closing = body.starts_with(open.as_str());
            out.push(Line {
                kind: Kind::Code {
                    lang: String::new(),
                    fence: open.clone(),
                },
                spaces: 0,
                marker: if closing { raw.len() } else { 0 },
                ordinal: 0,
                indent: 0,
                picture: None,
            });
            if closing {
                fence = None;
            }
            continue;
        }

        if let Some(open) = opening_fence(body) {
            fence = Some(open.clone());
            counters.clear();
            out.push(Line {
                kind: Kind::Code {
                    lang: body[open.len()..].trim().to_string(),
                    fence: open,
                },
                spaces: 0,
                marker: raw.len(),
                ordinal: 0,
                indent: 0,
                picture: None,
            });
            continue;
        }

        // a picture is the whole line: nothing in it is text to read, so all of
        // it is drawn at nothing and the picture is painted into the row that
        // leaves behind
        if let Some((_, src)) = crate::doc::image_line(body) {
            counters.clear();
            out.push(Line {
                kind: Kind::Image(src),
                spaces,
                marker: raw.len(),
                ordinal: 0,
                indent: 0,
                picture: None,
            });
            continue;
        }

        let level = (spaces / INDENT_SPACES) as u8;
        let line = if let Some(heading) = heading_level(body) {
            counters.clear();
            Line {
                kind: Kind::Heading(heading),
                spaces,
                marker: spaces + heading as usize + 1,
                ordinal: 0,
                indent: 0,
                picture: None,
            }
        } else if let Some(done) = task_state(body) {
            Line {
                kind: Kind::Task(done),
                spaces,
                marker: spaces + 6,
                ordinal: 0,
                indent: level,
                picture: None,
            }
        } else if is_bullet(body) {
            Line {
                kind: Kind::Bullet,
                spaces,
                marker: spaces + 2,
                ordinal: 0,
                indent: level,
                picture: None,
            }
        } else if let Some(digits) = ordered_digits(body) {
            let depth = level as usize;
            if counters.len() <= depth {
                counters.resize(depth + 1, 0);
            }
            counters[depth] += 1;
            counters.truncate(depth + 1);
            Line {
                kind: Kind::Ordered,
                spaces,
                marker: spaces + digits + 2,
                ordinal: counters[depth],
                indent: level,
                picture: None,
            }
        } else if body.starts_with("> ") {
            counters.clear();
            // `> > >` is three levels and one bar each — all of it is syntax,
            // and the depth is what the bars are counted from
            let mut rest = body;
            let mut depth = 0u8;
            while let Some(inner) = rest.strip_prefix("> ") {
                depth += 1;
                rest = inner;
            }
            Line {
                kind: Kind::Quote,
                spaces,
                marker: spaces + body.len() - rest.len(),
                ordinal: 0,
                indent: depth - 1,
                picture: None,
            }
        } else if is_rule(body) {
            counters.clear();
            Line {
                kind: Kind::Rule,
                spaces: 0,
                marker: raw.len(),
                ordinal: 0,
                indent: 0,
                picture: None,
            }
        // a row alone is a paragraph: `|test|` on its own line is words with pipes
        // in them until a second row lands next to it (`table::in_a_run`) — and
        // it counts like a paragraph too, so a list either side of it carries on
        } else if is_table(body)
            && (out.last().is_some_and(|l: &Line| l.kind == Kind::Table)
                || raws
                    .peek()
                    .is_some_and(|n| is_table(n.trim_start_matches(' '))))
        {
            counters.clear();
            Line {
                kind: Kind::Table,
                spaces,
                marker: spaces,
                ordinal: 0,
                indent: 0,
                picture: None,
            }
        } else {
            if body.trim().is_empty() {
                counters.clear();
            }
            Line::plain(spaces)
        };
        out.push(line);
    }

    if out.is_empty() {
        out.push(Line::plain(0));
    }
    out
}

/// The run of table lines `at` belongs to — a table is a paragraph's worth of
/// lines, and padding one row means padding all of them.
pub fn table_run(lines: &[Line], at: usize) -> Option<std::ops::Range<usize>> {
    if !matches!(lines.get(at)?.kind, Kind::Table) {
        return None;
    }
    let mut start = at;
    while start > 0 && matches!(lines[start - 1].kind, Kind::Table) {
        start -= 1;
    }
    let mut end = at;
    while end + 1 < lines.len() && matches!(lines[end + 1].kind, Kind::Table) {
        end += 1;
    }
    Some(start..end + 1)
}

/// Size every picture against the file it points at.
///
/// The parse and the measurement are two passes because they read two
/// different things: the kind comes out of the text, the height comes off the
/// disk. Once per edit, not once per frame.
///
/// ⚠️ An image whose file is not there goes back to being a paragraph, so its
/// markdown shows as the text it is — which is both honest and the only way to
/// fix the path by typing.
pub fn measure(lines: &mut [Line], vault: &Path) {
    for line in lines {
        let Kind::Image(src) = &line.kind else {
            continue;
        };
        if vault.join(src).is_file() {
            line.picture = Some(crate::vault::image_size(vault, src));
        } else {
            // ⚠️ the KIND stays `Image` — it is what the markdown says, and the
            // menu decides what a line can become by reading it. Only the
            // drawing changes: with no size there is no picture to paint, so
            // the marker stops hiding and the raw `![alt](src)` shows instead.
            line.marker = line.spaces;
        }
    }
}

fn opening_fence(body: &str) -> Option<String> {
    for token in ["```", "~~~"] {
        if body.starts_with(token) {
            let run = body
                .chars()
                .take_while(|c| *c == token.chars().next().unwrap())
                .count();
            return Some(token.chars().next().unwrap().to_string().repeat(run));
        }
    }
    None
}

fn heading_level(body: &str) -> Option<u8> {
    let hashes = body.chars().take_while(|c| *c == '#').count();
    ((1..=6).contains(&hashes) && body[hashes..].starts_with(' ')).then_some(hashes as u8)
}

fn task_state(body: &str) -> Option<bool> {
    let rest = body
        .strip_prefix("- ")
        .or_else(|| body.strip_prefix("* "))
        .or_else(|| body.strip_prefix("+ "))?;
    match rest.get(..4)? {
        "[ ] " => Some(false),
        "[x] " | "[X] " => Some(true),
        _ => None,
    }
}

fn is_bullet(body: &str) -> bool {
    ["- ", "* ", "+ "].iter().any(|m| body.starts_with(m))
}

/// Digits of `12. `, when that is how the line opens.
fn ordered_digits(body: &str) -> Option<usize> {
    let digits = body.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && body[digits..].starts_with(". ")).then_some(digits)
}

fn is_rule(body: &str) -> bool {
    let trimmed = body.trim_end();
    ["---", "***", "___"].iter().any(|token| {
        trimmed.len() >= 3 && trimmed.chars().all(|c| c == token.chars().next().unwrap())
    })
}

/// Shaped like a table row: opens and closes on a pipe. Shape is not kind —
/// [`parse`] makes it a `Table` only beside another one.
pub fn is_table(body: &str) -> bool {
    body.starts_with('|') && body.trim_end().ends_with('|')
}

/// The marker a new line inherits from the one Enter was pressed on: a list
/// carries on, a heading does not.
pub fn continuation(line: &Line, text: &str) -> String {
    let indent = " ".repeat(line.spaces);
    match &line.kind {
        // an empty item ends the list instead of growing it
        _ if text.trim().is_empty() => String::new(),
        Kind::Bullet => format!("{indent}- "),
        Kind::Task(_) => format!("{indent}- [ ] "),
        Kind::Ordered => format!("{indent}{}. ", line.ordinal + 1),
        Kind::Quote => format!("{indent}{}", "> ".repeat(usize::from(line.indent) + 1)),
        _ => indent,
    }
}

/// Whether the text runs out while a fence is still open.
///
/// ⚠️ this is the one thing [`parse`] carries between lines that no single
/// [`Line`] can spell. An opener and a closer are the same three characters;
/// which one a line IS depends entirely on what came before it. So a caller
/// holding a `Vec<Line>` cannot answer this by looking at them.
///
/// One extra line is parsed onto the end: if it comes back as code, the fence
/// that was open when the text ran out never closed.
pub fn ends_inside_a_fence(markdown: &str) -> bool {
    matches!(
        parse(&format!("{markdown}\n"))
            .last()
            .map(|line| &line.kind),
        Some(Kind::Code { .. })
    )
}

/// Where `[[note#section]]` lands: the byte offset of that heading's words.
///
/// ⚠️ over the RAW buffer, and NOT [`crate::doc::Lines::heading_block`]. That
/// one answers a BLOCK index, which means nothing to a surface that is one
/// buffer and parks its caret on an offset — and it reads a note whose
/// markers have been consumed, so it cannot say where in the file the words
/// actually are. The comparison is the same on both sides (trimmed,
/// case-blind), so the two halves of `#section` agree on what matches.
///
/// ⚠️ the offset is AFTER the `## `, not the start of the line: the caret is
/// going to sit there, and a caret parked on the syntax reads as a mistake.
///
/// The FIRST heading that matches wins. Markdown lets a note repeat a
/// heading and gives nothing to tell two apart — Obsidian resolves the same
/// way, and a link into a duplicated section is ambiguous in the file, not
/// here.
pub fn heading_offset(markdown: &str, heading: &str) -> Option<usize> {
    let target = heading.trim().to_lowercase();
    if target.is_empty() {
        return None;
    }
    let mut at = 0;
    for (raw, line) in markdown.split('\n').zip(parse(markdown)) {
        // ⚠️ clamped: `marker` indexes the line it was parsed from, and this
        // walks the same split — but an offset out of a slice is a panic, and
        // a link is not worth one.
        let from = line.marker.min(raw.len());
        if matches!(line.kind, Kind::Heading(_)) && raw[from..].trim().to_lowercase() == target {
            return Some(at + from);
        }
        // `+ 1` is the newline `split` took off. The last line has none, and
        // never needs the number.
        at += raw.len() + 1;
    }
    None
}

/// Where `[[note#^id]]` lands: the line that carries the anchor `^id`.
///
/// ⚠️ the anchor has to be WRITTEN in the file — see [`crate::inline::anchor`]
/// for why a line cannot be pointed at any other way.
///
/// ⚠️ never one inside a fence: `^done` in a shell snippet is a caret, not a
/// name. The same reason [`heading_offset`] parses instead of scanning for
/// `#`.
pub fn anchor_offset(markdown: &str, id: &str) -> Option<usize> {
    let target = id.trim().trim_start_matches('^').to_lowercase();
    if target.is_empty() {
        return None;
    }
    let mut at = 0;
    for (raw, line) in markdown.split('\n').zip(parse(markdown)) {
        // clamped for the reason `heading_offset` clamps: a slice out of
        // bounds is a panic, and a link is not worth one
        let from = line.marker.min(raw.len());
        if !matches!(line.kind, Kind::Code { .. })
            && crate::inline::anchor(raw).is_some_and(|(_, found)| found.to_lowercase() == target)
        {
            return Some(at + from);
        }
        at += raw.len() + 1;
    }
    None
}

/// What "send as live block" learns about the line holding the caret: the
/// name it wears, or the one to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveBlock {
    /// The line is already anchored: use the name, write nothing.
    Named(String),
    /// Append ` ^id` at `at` — the line's end — and use `id`.
    Write { id: String, at: usize },
}

/// The block anchor for the line holding `at`, or why the line cannot wear
/// one — in the words the status bar shows. The only writer of anchors this
/// app has: opt-in, one line, at the caret; a photo (quote card) is the
/// default and asks nothing of the file (ADR-003).
pub fn live_block(markdown: &str, at: usize) -> Result<LiveBlock, &'static str> {
    let at = at.min(markdown.len());
    let mut from = 0;
    for (raw, line) in markdown.split('\n').zip(parse(markdown)) {
        let to = from + raw.len();
        if at <= to {
            // ⚠️ the same two refusals `anchor_offset` reads by: a fence never
            // wears a name, and a blank line has nothing to name
            if matches!(line.kind, Kind::Code { .. }) {
                return Err("a code line cannot be anchored");
            }
            if raw.trim().is_empty() {
                return Err("nothing on this line to anchor");
            }
            return Ok(match crate::inline::anchor(raw) {
                Some((_, id)) => LiveBlock::Named(id.to_string()),
                None => LiveBlock::Write {
                    id: fresh_anchor(markdown),
                    at: to,
                },
            });
        }
        from = to + 1;
    }
    Err("nothing on this line to anchor")
}

/// A name no line of the note wears yet: six of `[a-z0-9]`, Obsidian's own
/// shape for the ones it writes.
///
/// ⚠️ `RandomState` and not a crate: it seeds itself per process and steps per
/// instance, which is all a name that only has to be UNLIKELY needs.
pub fn fresh_anchor(markdown: &str) -> String {
    use std::hash::{BuildHasher, Hasher, RandomState};
    loop {
        let mut n = RandomState::new().build_hasher().finish();
        let id: String = (0..6)
            .map(|_| {
                let d = (n % 36) as u8;
                n /= 36;
                char::from(if d < 10 { b'0' + d } else { b'a' + d - 10 })
            })
            .collect();
        if anchor_offset(markdown, &id).is_none() {
            return id;
        }
    }
}

/// Where `[[note#…]]` lands, whichever half of the syntax it is: `^id` names a
/// line, anything else names a heading.
pub fn section_offset(markdown: &str, section: &str) -> Option<usize> {
    match section.trim().strip_prefix('^') {
        Some(id) => anchor_offset(markdown, id),
        None => heading_offset(markdown, section),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(markdown: &str) -> Vec<Kind> {
        parse(markdown).into_iter().map(|line| line.kind).collect()
    }

    /// "send as live block" names the caret's line once: a named line keeps
    /// its name, a plain one gets a fresh one at its end, and a fence or a
    /// blank line gets a refusal instead of a marker.
    #[test]
    fn a_live_block_names_the_line_once_and_refuses_what_cannot_be_named() {
        let note = "intro\n\nthe gateway holds it ^gate-1\nthe token too\n```\ncode\n```\n";
        assert_eq!(
            live_block(note, note.find("gateway").unwrap()),
            Ok(LiveBlock::Named("gate-1".into()))
        );
        let Ok(LiveBlock::Write { id, at }) = live_block(note, note.find("token").unwrap()) else {
            panic!("a plain line gets a fresh name");
        };
        assert_eq!(
            at,
            note.find("the token too").unwrap() + "the token too".len()
        );
        assert_eq!(id.len(), 6);
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
        assert!(
            live_block(note, note.find("code").unwrap()).is_err(),
            "a fence"
        );
        assert!(live_block(note, 6).is_err(), "a blank line");
        assert!(live_block("", 0).is_err(), "an empty note");
        // the fresh name is a name `[[note#^id]]` would follow
        let written = format!("{}{} ^{id}{}", &note[..at], "", &note[at..]);
        assert_eq!(
            anchor_offset(&written, &id),
            Some(note.find("the token too").unwrap())
        );
    }

    /// `[[note#^id]]` lands on the line wearing that name — the one thing
    /// about a line that holds still while it is edited.
    #[test]
    fn a_block_link_lands_on_the_line_that_wears_the_name() {
        let note = "intro\n\n- the gateway holds it ^gate-1\n\n## Token store\nbody ^b2\n";
        let at = anchor_offset(note, "gate-1").expect("the anchor is there");
        // ⚠️ past the `- `, on the words: a caret parked on the bullet reads
        // as a mistake, the same place `heading_offset` lands
        assert_eq!(&note[at..at + 11], "the gateway");
        // written with the `^` or without it, and case-blind
        assert_eq!(anchor_offset(note, "^GATE-1"), Some(at));
        assert_eq!(anchor_offset(note, "  gate-1 "), Some(at));
        assert_eq!(anchor_offset(note, "gate"), None, "a name is not a prefix");
        assert_eq!(anchor_offset(note, ""), None);
        assert_eq!(anchor_offset(note, "^"), None);

        // ⚠️ a `^done` in a shell snippet is a caret, not a name
        let fenced = "```\nrm -f ^b2\n```\nthe real one ^b2\n";
        let real = anchor_offset(fenced, "b2").expect("the one outside the fence");
        assert_eq!(&fenced[real..real + 8], "the real");

        // and the two halves of `#` share one door
        assert_eq!(section_offset(note, "^gate-1"), Some(at));
        assert_eq!(
            section_offset(note, "Token store"),
            heading_offset(note, "Token store"),
            "anything that is not a `^` is still a heading"
        );
    }

    /// `[[note#section]]` lands on the heading's WORDS, in the raw buffer —
    /// the number a caret can be parked on.
    #[test]
    fn a_section_link_finds_its_heading_in_the_buffer() {
        let note = "intro\n\n## Token store\n\nbody\n\n### Rotation\nmore\n";
        let at = heading_offset(note, "Token store").expect("the heading is there");
        // after the `## `, on the T
        assert_eq!(&note[at..at + 11], "Token store");
        assert_eq!(&note[at - 3..at], "## ");

        // case-blind and space-blind, the way `Lines::heading_block` matches
        assert_eq!(heading_offset(note, "  token STORE  "), Some(at));
        // a deeper heading is still a heading
        let deeper = heading_offset(note, "Rotation").expect("### counts too");
        assert_eq!(&note[deeper..deeper + 8], "Rotation");

        // ⚠️ body text that reads like the heading is NOT the heading
        assert_eq!(
            heading_offset("Token store\n\n## other\n", "Token store"),
            None
        );
        assert_eq!(heading_offset(note, "nowhere"), None);
        assert_eq!(
            heading_offset(note, "  "),
            None,
            "an empty section names nothing"
        );

        // a `#` inside a fence is code, not a heading — `parse` already knows
        let fenced = "```\n## Token store\n```\n## Token store\n";
        let real = heading_offset(fenced, "Token store").expect("the one outside the fence");
        assert!(
            real > fenced.find("```\n## Token").unwrap() + 20,
            "took the fenced one"
        );
    }

    #[test]
    fn every_line_says_what_it_is() {
        let kinds = kinds("# one\n\ntext\n\n- a\n  - b\n1. x\n2. y\n- [ ] t\n- [x] d\n> q\n---\n");
        assert_eq!(
            kinds,
            vec![
                Kind::Heading(1),
                Kind::Paragraph,
                Kind::Paragraph,
                Kind::Paragraph,
                Kind::Bullet,
                Kind::Bullet,
                Kind::Ordered,
                Kind::Ordered,
                Kind::Task(false),
                Kind::Task(true),
                Kind::Quote,
                Kind::Rule,
                // `split` leaves the empty string after the final newline
                Kind::Paragraph,
            ]
        );
    }

    #[test]
    fn the_marker_is_exactly_the_syntax_that_opens_the_line() {
        for (line, marker) in [
            ("# h", 2),
            ("### h", 4),
            ("- item", 2),
            ("  - nested", 4),
            ("12. item", 4),
            ("- [ ] task", 6),
            ("- [x] task", 6),
            ("> quote", 2),
            ("plain", 0),
        ] {
            let parsed = &parse(line)[0];
            assert_eq!(parsed.marker, marker, "{line:?}");
            // what is left is what the reader sees
            assert!(!line[parsed.marker..].starts_with(' '), "{line:?}");
        }
    }

    /// What the menu writes to turn a line into another kind, and what [`parse`]
    /// then reads back out of it — the two have to be each other's inverse.
    #[test]
    fn the_syntax_a_kind_is_written_with_parses_back_as_that_kind() {
        for kind in [
            Kind::Heading(1),
            Kind::Heading(6),
            Kind::Bullet,
            Kind::Ordered,
            Kind::Task(false),
            Kind::Task(true),
            Kind::Quote,
        ] {
            let line = format!("{}words", marker_for(&kind));
            assert_eq!(parse(&line)[0].kind, kind, "{line:?}");
        }
        // and the kinds whose syntax is not a prefix have nothing to write
        assert_eq!(marker_for(&Kind::Table), "");
        assert_eq!(marker_for(&Kind::Rule), "");
    }

    #[test]
    fn a_hash_without_a_space_is_not_a_heading() {
        assert_eq!(kinds("#tag\n")[0], Kind::Paragraph);
        assert_eq!(kinds("####### seven\n")[0], Kind::Paragraph);
    }

    #[test]
    fn numbering_is_positional_and_restarts_after_a_break() {
        let lines = parse("5. a\n5. b\n5. c\n\n9. d\n");
        let ordinals: Vec<usize> = lines.iter().map(|line| line.ordinal).collect();
        assert_eq!(ordinals, vec![1, 2, 3, 0, 1, 0]);
    }

    #[test]
    fn nesting_counts_in_tab_presses() {
        let lines = parse("- top\n  - one\n    - two\n");
        assert_eq!(
            lines.iter().map(|line| line.indent).collect::<Vec<_>>(),
            vec![0, 1, 2, 0],
            "and the empty string after the final newline"
        );
    }

    #[test]
    fn a_fence_holds_everything_until_it_closes() {
        let kinds = kinds("text\n```rs\n# not a heading\n- not a bullet\n```\nafter\n");
        assert!(matches!(kinds[1], Kind::Code { .. }));
        assert!(matches!(kinds[2], Kind::Code { .. }));
        assert!(matches!(kinds[3], Kind::Code { .. }));
        assert!(matches!(kinds[4], Kind::Code { .. }));
        assert_eq!(kinds[5], Kind::Paragraph, "the fence let go");
    }

    #[test]
    fn a_list_carries_on_and_an_empty_item_does_not() {
        let bullet = &parse("- item")[0];
        assert_eq!(continuation(bullet, "item"), "- ");
        let nested = &parse("  - item")[0];
        assert_eq!(continuation(nested, "item"), "  - ");
        let task = &parse("- [x] done")[0];
        assert_eq!(
            continuation(task, "done"),
            "- [ ] ",
            "a new task is not done"
        );
        let ordered = &parse("3. third")[0];
        assert_eq!(
            continuation(ordered, "third"),
            "2. ",
            "positional, not typed"
        );
        let heading = &parse("# title")[0];
        assert_eq!(continuation(heading, "title"), "");
        // an empty item ends the list rather than growing it
        assert_eq!(continuation(bullet, ""), "");
    }

    #[test]
    fn a_table_run_is_every_line_of_the_grid_and_nothing_after_it() {
        let lines = parse("| a | bbbb |\n| --- | --- |\n| ccc | d |\n");
        assert_eq!(table_run(&lines, 1), Some(0..3), "header, divider, row");
        assert_eq!(table_run(&lines, 3), None, "the empty line after it");
    }

    #[test]
    fn a_picture_is_the_whole_line_and_a_missing_one_is_a_paragraph() {
        let dir = std::env::temp_dir().join(format!("synapso-sheet-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        image::RgbaImage::new(84, 42)
            .save(dir.join("shot.png"))
            .expect("write the picture");

        let mut lines = parse("![the login screen](shot.png)\n![gone](missing.png)\n");
        assert_eq!(lines[0].kind, Kind::Image("shot.png".into()));
        assert_eq!(
            lines[0].marker, 29,
            "all of it is syntax, so all of it hides"
        );

        measure(&mut lines, &dir);
        assert_eq!(lines[0].picture, Some(kurbo::Size::new(84.0, 42.0)));

        // a picture that is not there shows its markdown instead, which is the
        // only way to see the path and fix it. ⚠️ It is still an `Image`: the
        // kind is what the line SAYS, and the menu reads the kind to decide
        // what a line may become — a missing file must not make `![](x.png)`
        // convertible into a task.
        assert_eq!(lines[1].kind, Kind::Image("missing.png".into()));
        assert_eq!(lines[1].marker, 0, "nothing hides, so the path shows");
        assert_eq!(lines[1].picture, None);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `> > >` is three levels, and the whole prefix is syntax. Reading only
    /// the first `> ` left the rest standing in the text as literal `>`.
    #[test]
    fn a_quote_knows_how_deep_it_is_nested() {
        let lines = parse("> one\n> > two\n> > > three\nplain");
        assert_eq!(
            lines.iter().map(|l| l.kind.clone()).collect::<Vec<_>>(),
            vec![Kind::Quote, Kind::Quote, Kind::Quote, Kind::Paragraph]
        );
        assert_eq!(
            lines.iter().take(3).map(|l| l.indent).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "one bar per level"
        );
        // the whole prefix is the marker, or the extra `>` would be text
        assert_eq!(
            lines.iter().take(3).map(|l| l.marker).collect::<Vec<_>>(),
            vec![2, 4, 6]
        );
        // the prefix's own width is what a shell steps the nesting by
        assert_eq!(
            lines.iter().take(3).map(Line::lead).collect::<Vec<_>>(),
            vec![2, 4, 6]
        );
        // and a line the gutter draws a glyph for holds no place at all
        assert_eq!(parse("- item")[0].lead(), 0);
        // ⏎ at the end of a nested quote stays at that level
        assert_eq!(continuation(&lines[2], "three"), "> > > ");
        assert_eq!(continuation(&lines[0], "one"), "> ");
    }

    /// A pipe line on its own is a paragraph, drawn and edited as the words it
    /// is: a table needs a second row next to it. While the header is being
    /// typed it is one line, and it becomes a table when the next row exists.
    #[test]
    fn a_lone_pipe_line_is_a_paragraph_until_a_second_row_lands() {
        assert_eq!(kinds("| abc | xyz |"), vec![Kind::Paragraph]);
        assert_eq!(kinds("|test|"), vec![Kind::Paragraph]);
        assert_eq!(
            kinds("intro\n|test|\nmore"),
            vec![Kind::Paragraph; 3],
            "between two sentences"
        );
        // nothing of it is hidden: a paragraph's marker is its indentation
        let lone = &parse("  |test|")[0];
        assert_eq!((lone.kind.clone(), lone.marker), (Kind::Paragraph, 2));
        assert_eq!(table_run(&parse("|test|"), 0), None);

        // the second row makes both of them the table — a divider not needed
        assert_eq!(kinds("| a | b |\n|"), vec![Kind::Table, Kind::Table]);
        assert_eq!(
            kinds("| a | b |\n| c | d |\n\n|x|"),
            vec![Kind::Table, Kind::Table, Kind::Paragraph, Kind::Paragraph]
        );

        // a table whose other rows are deleted goes back to being words
        assert_eq!(kinds("| a | b |\n"), vec![Kind::Paragraph; 2]);
        // and a pipe line inside a fence is still code
        assert!(matches!(kinds("```\n|a|\n```")[1], Kind::Code { .. }));
    }

    #[test]
    fn a_lone_pipe_line_between_list_items_keeps_the_count_going() {
        let ordinals = |md: &str| parse(md).iter().map(|l| l.ordinal).collect::<Vec<_>>();
        // it is a paragraph, so it counts like one
        assert_eq!(ordinals("1. a\n| note |\n2. b"), vec![1, 0, 2]);
        assert_eq!(
            ordinals("1. a\n| note |\n2. b"),
            ordinals("1. a\nnote\n2. b")
        );
        // a real table still ends the list
        assert_eq!(ordinals("1. a\n| x |\n| y |\n2. b"), vec![1, 0, 0, 1]);
    }
}
