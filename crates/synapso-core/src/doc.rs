//! The note read as blocks — the model, never the thing on screen.
//!
//! Markdown is the input language and the file format. The buffer IS the file
//! (ADR-008): nothing here writes markdown back, so a block is only what
//! search, the canvas cards, the picker and the word count read off a note.
//! Everything here is pure: the sheet reads the buffer line by line in
//! `sheet.rs`, the plumbing lives in the shell.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Paragraph,
    /// 1..=6
    Heading(u8),
    Bullet,
    /// The number is not kept — the sheet counts positionally (`crate::sheet`).
    Ordered,
    Task(bool),
    Quote,
    /// Code, with its language (empty when none was given) and the exact
    /// opening fence — a ```` fence must close on four backticks, or every
    /// other reader swallows the rest of the file (#34).
    ///
    /// ⚠️ An **empty fence is markdown's other code block**: the indented one,
    /// four spaces and no closer. It is the one kind of block whose syntax is
    /// its own indentation, so nothing here can be shortened away.
    Code {
        lang: String,
        fence: String,
    },
    /// A picture, by path relative to the vault. The block's text is the alt,
    /// which doubles as the caption under it.
    Image(String),
    /// `---`: the one block with no text of its own.
    Rule,
    /// A markdown table: the rows verbatim, pipes and all, one block. It is
    /// the padding that makes it read as a grid — see [`crate::table`].
    Table,
}

impl Kind {
    pub fn is_list(&self) -> bool {
        matches!(self, Kind::Bullet | Kind::Ordered | Kind::Task(_))
    }
}

/// How deep a list item can be pushed with Tab. Five is past any real outline.
pub const MAX_INDENT: u8 = 5;

/// A note's blocks as plain text: kind and words.
///
/// Everything that only *reads* a note reads this — search across the vault,
/// the canvas cards, the picker's preview, the ⌘K palette. The buffer itself
/// is the file (ADR-008); this is the one view derived from it.
#[derive(Debug, Default, Clone)]
pub struct Lines {
    pub blocks: Vec<(Kind, String)>,
}

impl Lines {
    pub fn parse(markdown: &str) -> Self {
        let (_, markdown) = split_front(markdown);
        let mut blocks: Vec<(Kind, String)> = parse_blocks(markdown)
            .into_iter()
            .map(|(kind, _, text)| (kind, text))
            .collect();
        // an empty note is one empty paragraph
        if blocks.is_empty() {
            blocks.push((Kind::Paragraph, String::new()));
        }
        Self { blocks }
    }

    pub fn words(&self) -> usize {
        self.blocks
            .iter()
            .map(|(_, text)| text.split_whitespace().count())
            .sum()
    }

    /// Plain text for search.
    pub fn plain(&self) -> String {
        self.blocks
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Everything but the headings — what a card previews, since the card
    /// already carries the note's name above it. It stops once it has `limit`
    /// characters: a card on the canvas shows a dozen lines and is redrawn on
    /// every frame, and reading a whole note to print the top of it is the
    /// difference between a board that pans and one that stutters.
    pub fn body_preview(&self, limit: usize) -> String {
        let mut out = String::new();
        for (_, text) in self
            .blocks
            .iter()
            .filter(|(kind, _)| !matches!(kind, Kind::Heading(_)))
        {
            if out.len() >= limit {
                break;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
        out
    }

    /// The block holding a heading, for `[[note#section]]`.
    pub fn heading_block(&self, heading: &str) -> Option<usize> {
        let target = heading.trim().to_lowercase();
        self.blocks.iter().position(|(kind, text)| {
            matches!(kind, Kind::Heading(_)) && text.trim().to_lowercase() == target
        })
    }

    /// The line wearing the anchor `^id` — what a `[[note#^id]]` card shows,
    /// LIVE: read off the note each time, the way a section is, never copied.
    ///
    /// The marker itself comes off: it is a name, not a word of the line. A
    /// LINE, as `sheet::anchor_offset` lands and `inline::anchor` reads — a
    /// name on the first line of a paragraph names that line, not the
    /// paragraph — and never inside a fence (`^x` in code is a caret).
    pub fn block(&self, id: &str) -> Option<String> {
        let target = id.trim().trim_start_matches('^').to_lowercase();
        if target.is_empty() {
            return None;
        }
        self.blocks
            .iter()
            .filter(|(kind, _)| !matches!(kind, Kind::Code { .. }))
            .flat_map(|(_, text)| text.lines())
            .find_map(|line| {
                let (range, found) = crate::inline::anchor(line)?;
                (found.to_lowercase() == target).then(|| line[..range.start].trim().to_string())
            })
    }

    /// Everything under a heading, up to the next one of the same or higher
    /// level — what a section card quotes.
    pub fn section(&self, heading: &str, limit: usize) -> Option<String> {
        let start = self.heading_block(heading)?;
        let Kind::Heading(level) = self.blocks[start].0 else {
            return None;
        };
        let mut out = String::new();
        for (_, text) in self.blocks[start + 1..]
            .iter()
            .take_while(|(kind, _)| !matches!(kind, Kind::Heading(l) if *l <= level))
        {
            if out.len() >= limit {
                break;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
        Some(out.trim().to_string())
    }
}

// --- markdown -> blocks --------------------------------------------------

/// The frontmatter at the top of `md`, if there is one, and everything after.
///
/// a document that opens with `---` is only frontmatter when the
/// next line has something on it — a horizontal rule is followed by a blank
/// line — and when a closing `---` actually turns up. Anything else is a rule,
/// which is what it used to always be read as.
fn split_front(md: &str) -> (Option<&str>, &str) {
    let Some(rest) = md.strip_prefix("---\n") else {
        return (None, md);
    };
    if rest.starts_with('\n') || rest.starts_with("---") {
        return (None, md);
    }
    let mut at = 4;
    for line in rest.split_inclusive('\n') {
        at += line.len();
        if line.trim_end() == "---" {
            // the blank line under it belongs to the frontmatter too, or it
            // would come back as an empty paragraph and then not come back
            let tail = &md[at..];
            let blank = tail.len() - tail.trim_start_matches('\n').len();
            return (Some(&md[..at + blank]), &md[at + blank..]);
        }
    }
    (None, md)
}

pub fn parse_blocks(markdown: &str) -> Vec<(Kind, u8, String)> {
    let mut out: Vec<(Kind, u8, String)> = Vec::new();
    let mut lines = markdown.lines().peekable();
    let mut paragraph: Vec<String> = Vec::new();
    // whether a blank line stands between here and the block before it
    let mut broken = true;

    let flush = |paragraph: &mut Vec<String>, out: &mut Vec<(Kind, u8, String)>| {
        if !paragraph.is_empty() {
            out.push((Kind::Paragraph, 0, paragraph.join("\n")));
            paragraph.clear();
        }
    };

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let width = indent_width(line);
        let indent = (width / 2).min(MAX_INDENT as usize) as u8;
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut out);
            broken = true;
            continue;
        }
        let adjoining = !broken;
        broken = false;
        // four spaces of indent is code to every other reader: `    # x` is
        // not a heading and an indented ``` opens no fence. Unless the line
        // hangs right under a list item — that deep it is the item's own
        // continuation, handled below. Fences never take that escape hatch:
        // folded into the item they would come back shallower and open for
        // real on the next parse, so they stay verbatim instead.
        if width >= 4
            && !(adjoining
                && paragraph.is_empty()
                && out.last().is_some_and(|(kind, _, _)| kind.is_list())
                && !trimmed.starts_with("```")
                && !trimmed.starts_with("~~~"))
        {
            // the run, gathered without swallowing the line that ends it
            let mut run = vec![line.to_string()];
            // ⚠️ everything that decides this is *not* a block goes in here,
            // before a line is swallowed: blank lines are only held back once
            // it is certain they belong to a code block. Deciding late ate the
            // blank line that ends a list item's own prose, and the sentence
            // after it walked into the item (codex, #65).
            let mut alone = !adjoining
                && paragraph.is_empty()
                && !out.last().is_some_and(|(kind, _, _)| kind.is_list());
            // blank lines held back until it is known whether the block goes on
            let mut held = 0;
            while let Some(next) = lines.peek() {
                if next.trim().is_empty() {
                    if !alone {
                        break; // a blank line closes anything that is not a block
                    }
                    held += 1;
                    lines.next();
                    continue;
                }
                if indent_width(next) < 4 {
                    if held == 0 {
                        alone = false; // words up against it: this is not a block
                    }
                    break;
                }
                // ⚠️ an indented chunk after a blank line is the SAME block —
                // markdown says so, and typing Enter inside one has to leave
                // it whole rather than cut it in two on the next read
                run.extend(std::iter::repeat_n(String::new(), held));
                held = 0;
                run.push(lines.next().unwrap_or_default().to_string());
            }
            // the blanks swallowed on the way out still separate what follows
            // — and they are only ever swallowed on the way to a code block,
            // so the paragraph below never loses one.
            // nothing reads `adjoining` after a code block today, so
            // no test holds this line down; leaving the flag lying about the
            // blank line is a trap for the next branch that asks.
            broken = held > 0;
            // markdown's oldest block, and the only one whose syntax *is* its
            // indentation. Never right under a list item: four spaces there is
            // the item's own second paragraph, which is prose, not code.
            if alone {
                out.push((
                    Kind::Code {
                        lang: String::new(),
                        fence: String::new(),
                    },
                    0,
                    dedent(&run),
                ));
            } else {
                paragraph.extend(run);
            }
            continue;
        }
        // a `~~~` fence is kept whole and unread — the markdown inside it is
        // not markdown, and parsing it would rewrite someone's code sample.
        // Only a run at least as long as the opener closes it, so a `~~~`
        // sample can sit inside a `~~~~` span.
        if trimmed.starts_with("~~~") {
            flush(&mut paragraph, &mut out);
            let run = trimmed.chars().take_while(|c| *c == '~').count();
            let mut fenced = vec![line.to_string()];
            for line in lines.by_ref() {
                fenced.push(line.to_string());
                if closes_fence(line, b'~', run) {
                    break;
                }
            }
            out.push((Kind::Paragraph, 0, fenced.join("\n")));
            continue;
        }
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        if ticks >= 3 {
            flush(&mut paragraph, &mut out);
            let mut code = Vec::new();
            for line in lines.by_ref() {
                if closes_fence(line, b'`', ticks) {
                    break;
                }
                code.push(line.to_string());
            }
            out.push((
                Kind::Code {
                    lang: trimmed[ticks..].trim().to_string(),
                    fence: trimmed[..ticks].to_string(),
                },
                0,
                code.join("\n"),
            ));
            continue;
        }
        // a table is two lines before it is one: a row, and the divider
        // under it. It interrupts an open paragraph, which is what GFM says
        // and what makes a table pasted under a sentence come back a table.
        if is_row_start(trimmed, lines.peek().copied()) {
            flush(&mut paragraph, &mut out);
            let mut rows = vec![trimmed.trim().to_string()];
            while lines.peek().is_some_and(|next| crate::table::is_row(next)) {
                rows.push(lines.next().unwrap_or_default().trim().to_string());
            }
            // ⚠️ the depth is the table's: a table under a list item is that
            // item's, and writing it back at the margin would move it out of
            // the list on the next unrelated save (codex, #63)
            out.push((Kind::Table, indent, crate::table::align(&rows.join("\n"))));
            continue;
        }
        if let Some((alt, src)) = image_line(trimmed) {
            flush(&mut paragraph, &mut out);
            out.push((Kind::Image(src), 0, alt));
            continue;
        }
        // `---` under an open paragraph is a setext heading's underline, not a
        // rule — it stays in the paragraph and round-trips verbatim, the way
        // setext `===` always has
        if let Some(mark) = rule_run(trimmed) {
            if mark != '-' || paragraph.is_empty() {
                flush(&mut paragraph, &mut out);
                out.push((Kind::Rule, 0, String::new()));
                continue;
            }
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            flush(&mut paragraph, &mut out);
            out.push((
                Kind::Heading(level as u8),
                0,
                trimmed[level + 1..].trim().to_string(),
            ));
            continue;
        }
        if let Some((kind, rest)) = strip_task(trimmed) {
            flush(&mut paragraph, &mut out);
            out.push((kind, indent, rest));
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            flush(&mut paragraph, &mut out);
            out.push((Kind::Bullet, indent, rest.to_string()));
            continue;
        }
        if let Some((digits, rest)) = trimmed.split_once(". ") {
            if is_ordered_marker(digits) {
                flush(&mut paragraph, &mut out);
                out.push((Kind::Ordered, indent, rest.to_string()));
                continue;
            }
        }
        if let Some(rest) = trimmed
            .strip_prefix("> ")
            .or_else(|| trimmed.strip_prefix(">"))
        {
            flush(&mut paragraph, &mut out);
            // consecutive `>` lines are one quote, not one quote per line —
            // saving them apart put a blank line between and split the block
            match out.last_mut() {
                Some((Kind::Quote, _, text)) if adjoining => {
                    text.push('\n');
                    text.push_str(rest);
                }
                _ => out.push((Kind::Quote, 0, rest.to_string())),
            }
            continue;
        }
        // markdown's lazy continuation: an adjoining line under a list item or
        // a quote belongs to it — accumulated as a paragraph it left the block
        match out.last_mut() {
            Some((kind, _, text))
                if adjoining
                    && paragraph.is_empty()
                    && (kind.is_list() || *kind == Kind::Quote) =>
            {
                text.push('\n');
                text.push_str(trimmed);
            }
            _ => paragraph.push(line.to_string()),
        }
    }
    flush(&mut paragraph, &mut out);
    out
}

/// Does a table start here — this line and the one under it?
fn is_row_start(line: &str, next: Option<&str>) -> bool {
    next.is_some_and(|next| crate::table::opens(line, next))
}

/// How many columns a line opens with. Whatever the writer used, two spaces
/// is one level here — and so is a Tab, or an outline written with Tabs comes
/// back flat.
fn indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum()
}

/// An indented block without the four columns that made it code. Deeper
/// indentation inside it is the writer's, and it stays.
fn dedent(run: &[String]) -> String {
    run.iter()
        .map(|line| {
            let mut left = 4;
            let mut at = 0;
            for c in line.chars() {
                let width = match c {
                    '\t' => 2,
                    ' ' => 1,
                    _ => break,
                };
                if width > left {
                    break;
                }
                left -= width;
                at += c.len_utf8();
                if left == 0 {
                    break;
                }
            }
            &line[at..]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A horizontal rule: three or more `-`, `*` or `_`, and nothing else but
/// spaces. `----` is one to every reader; only `---` was one here (#63).
fn rule_run(line: &str) -> Option<char> {
    let mark = line.chars().find(|c| !c.is_whitespace())?;
    if !matches!(mark, '-' | '*' | '_') {
        return None;
    }
    let mut run = 0;
    for c in line.chars() {
        if c == mark {
            run += 1;
        } else if !c.is_whitespace() {
            return None;
        }
    }
    (run >= 3).then_some(mark)
}

/// A closing fence is the marker alone — no info string, at most three
/// columns deep, and no shorter a run than the opener. Anything else is the
/// fence's own content: `~~~~x` and a four-space-indented closer both stay
/// inside, or the verbatim span starts getting parsed as markdown.
fn closes_fence(line: &str, marker: u8, run: usize) -> bool {
    let rest = line.trim_start();
    let depth: usize = line[..line.len() - rest.len()]
        .chars()
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum();
    let rest = rest.trim_end();
    depth < 4 && rest.len() >= run && rest.bytes().all(|b| b == marker)
}

/// `1.` opens a list; `2020.` opens a sentence about a year.
///
/// three digits, not the nine CommonMark allows. The number is not kept —
/// the sheet counts positionally — so a year read as a marker is a year that
/// every reader of the note loses. Raise the cap the day a list legitimately
/// passes 999.
fn is_ordered_marker(digits: &str) -> bool {
    !digits.is_empty() && digits.len() <= 3 && digits.chars().all(|c| c.is_ascii_digit())
}

/// `![alt](src)` on a line of its own — a picture is a block, not a word.
pub fn image_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("![")?;
    let (alt, rest) = rest.split_once("](")?;
    let src = rest.strip_suffix(')')?;
    (!src.is_empty()).then(|| (alt.to_string(), src.to_string()))
}

fn strip_task(line: &str) -> Option<(Kind, String)> {
    for (prefix, done) in [
        ("- [ ] ", false),
        ("- [x] ", true),
        ("- [X] ", true),
        ("* [ ] ", false),
        ("* [x] ", true),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some((Kind::Task(done), rest.to_string()));
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Kind> {
        Lines::parse(src)
            .blocks
            .into_iter()
            .map(|(kind, _)| kind)
            .collect()
    }

    fn indents(src: &str) -> Vec<u8> {
        parse_blocks(src)
            .into_iter()
            .map(|(_, indent, _)| indent)
            .collect()
    }

    /// `[[note#^id]]` on a card shows the block wearing the name — the words
    /// only, the marker off — and nothing when no block wears it.
    #[test]
    fn a_block_card_reads_the_anchored_block_without_its_marker() {
        let lines = Lines::parse(
            "# Title\n\nintro\n\nthe gateway holds it\nand the token too ^gate-1\n\n```\ncode ^gate-2\n```\n- item ^Gate-3\n",
        );
        assert_eq!(lines.block("gate-1").as_deref(), Some("and the token too"));
        // written with the `^` or without, and case-blind, as the link is
        assert_eq!(lines.block("^GATE-1"), lines.block("gate-1"));
        assert_eq!(lines.block("gate-3").as_deref(), Some("item"));
        assert_eq!(
            lines.block("gate-2"),
            None,
            "a caret inside a fence is not a name"
        );
        assert_eq!(lines.block("gate"), None, "a name is not a prefix");
        assert_eq!(lines.block(""), None);
    }

    /// A card reads the top of a note, not the note — the difference is a
    /// frame that draws in 7ms instead of 760.
    #[test]
    fn a_preview_stops_once_it_has_enough() {
        let lines = Lines::parse(SAMPLE);
        let whole = lines.body_preview(usize::MAX);
        let short = lines.body_preview(20);
        assert!(short.len() < whole.len(), "{short:?}");
        assert!(whole.starts_with(&short), "{short:?}");
        assert!(short.len() >= 20, "{short:?}"); // enough for the first line

        // a section is bounded the same way, and still ends inside itself
        let section = lines.section("Token exchange", 20).unwrap();
        assert!(lines
            .section("Token exchange", usize::MAX)
            .unwrap()
            .starts_with(&section));
    }

    const SAMPLE: &str = "\
# Auth Flow

Every request passes through the gateway.
It is minted once.

## Token exchange

- [ ] refresh rotation
- [x] revoke on unlink

1. first
2. second

> the gateway is the only component

---

```rust
fn exchange() {}
```
";

    /// ⚠️ The blank line under a list item's own prose is that paragraph's
    /// end. Swallowed while looking for more indented lines, it merged the
    /// sentence after it into the item.
    #[test]
    fn a_blank_line_after_a_list_items_prose_is_not_swallowed() {
        let blocks = Lines::parse("- item\n\n    second\n\nafter\n").blocks;
        assert_eq!(blocks.len(), 3, "the blank line was eaten");
    }

    /// ⚠️ An empty line inside an indented block is the block's own: read as
    /// a blank line it would cut the block in two.
    #[test]
    fn a_blank_line_inside_an_indented_block_stays_inside_it() {
        let blocks = Lines::parse("    one\n\n    two\n").blocks;
        assert_eq!(blocks.len(), 1, "the block was cut in two");
        assert_eq!(blocks[0].1, "one\n\ntwo");
        // but a blank line and then words at the margin end it, as always
        assert_eq!(Lines::parse("    one\n\ntwo\n").blocks.len(), 2);
    }

    /// ⚠️ Three is the number: `--` is two dashes somebody typed.
    #[test]
    fn a_rule_is_three_marks_and_never_two() {
        for src in ["----\n", "___\n", "* * *\n", "- - -\n", "-------\n"] {
            assert_eq!(kinds(src)[0], Kind::Rule, "{src:?}");
        }
        for src in ["--\n", "**\n", "-- not a rule\n", "- item\n"] {
            assert_ne!(kinds(src)[0], Kind::Rule, "{src:?}");
        }
    }

    /// The Markdown syntax document, by John Gruber — the corpus every reader
    /// is measured against, and the one Weber handed over (#63). What it is
    /// held to: the reading — every block the document is about is the block
    /// it says it is.
    #[test]
    fn the_markdown_syntax_document_reads() {
        let kinds = kinds(include_str!("fixtures/markdown-syntax.md"));
        let count = |want: &dyn Fn(&Kind) -> bool| kinds.iter().filter(|k| want(k)).count();
        assert_eq!(count(&|k| *k == Kind::Rule), 1, "`----` is a rule");
        assert_eq!(
            count(&|k| matches!(k, Kind::Code { fence, .. } if fence.is_empty())),
            3,
            "the indented code blocks"
        );
        assert_eq!(
            count(&|k| matches!(k, Kind::Code { fence, .. } if !fence.is_empty())),
            1,
            "the fenced one"
        );
        assert_eq!(count(&|k| *k == Kind::Quote), 5, "the blockquotes");
        assert_eq!(count(&|k| matches!(k, Kind::Heading(_))), 13, "the headers");
        assert_eq!(count(&|k| k.is_list()), 47, "the list items");
    }

    #[test]
    fn a_table_under_a_list_item_stays_under_it() {
        let blocks = parse_blocks("- item\n\n  | a | b |\n  | - | - |\n");
        assert_eq!(blocks[1].0, Kind::Table);
        assert_eq!(blocks[1].1, 1, "the table left the item");
    }

    #[test]
    fn a_table_is_one_block_and_reads_aligned() {
        let blocks = Lines::parse("| a | bb |\n| --- | --- |\n| ccc | d |\n").blocks;
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0],
            (
                Kind::Table,
                "| a   | bb  |\n| --- | --- |\n| ccc | d   |".into()
            )
        );

        // no divider, no table — GitHub reads this as a paragraph, so do we
        assert_eq!(kinds("| a | b |\n| c | d |\n")[0], Kind::Paragraph);
        // and one interrupts a paragraph, which is where a pasted table lands
        assert_eq!(
            kinds("intro\n| a | b |\n| - | - |\n"),
            vec![Kind::Paragraph, Kind::Table]
        );
    }

    /// Markdown this editor has no block for. It reads as a paragraph and is
    /// never rewritten — the buffer is the file.
    const FOREIGN: &str = "\
---
title: Adapter Design
tags: [auth, draft]
---

Setext heading
==============

| column | other |
| ------ | ----- |
| a      | b     |

<div class=\"callout\">
  raw html, untouched
</div>

> a quote that runs
> across two lines

~~~
- this is a code sample, not a bullet
~~~

Text with a [ref link][1] and a footnote.[^1]

[1]: https://example.com
[^1]: the note itself
";

    #[test]
    fn frontmatter_is_kept_out_of_the_blocks() {
        let lines = Lines::parse(FOREIGN);
        // the metadata is not a block: search and the cards never see it
        assert!(!lines.plain().contains("tags:"));
        assert!(matches!(lines.blocks[0].0, Kind::Paragraph));

        // `---` with a blank line under it is still a horizontal rule
        assert_eq!(kinds("---\n\nafter\n")[0], Kind::Rule);
        // and an unclosed opener is not frontmatter either — it is the rule it
        // has always been
        assert_eq!(kinds("---\ntitle: x\n\nbody\n")[0], Kind::Rule);
    }

    #[test]
    fn every_construct_lands_in_the_right_block() {
        assert_eq!(
            kinds(SAMPLE),
            vec![
                Kind::Heading(1),
                Kind::Paragraph,
                Kind::Heading(2),
                Kind::Task(false),
                Kind::Task(true),
                Kind::Ordered,
                Kind::Ordered,
                Kind::Quote,
                Kind::Rule,
                Kind::Code {
                    lang: "rust".into(),
                    fence: "```".into()
                },
            ]
        );
        // a wrapped paragraph is one block, newline and all
        assert_eq!(
            Lines::parse(SAMPLE).blocks[1].1,
            "Every request passes through the gateway.\nIt is minted once."
        );
    }

    #[test]
    fn a_pasted_picture_is_a_block_of_its_own() {
        const NOTE: &str = "# Shot\n\n![the login screen](assets/pasted.png)\n\nafter\n";
        assert_eq!(
            kinds(NOTE),
            vec![
                Kind::Heading(1),
                Kind::Image("assets/pasted.png".into()),
                Kind::Paragraph,
            ]
        );
        assert_eq!(Lines::parse(NOTE).blocks[1].1, "the login screen");
        // a bare `![` is not a picture, and neither is one without a source
        assert_eq!(kinds("![oops]()\n")[0], Kind::Paragraph);
    }

    #[test]
    fn nesting_is_read_off_the_indent() {
        const NESTED: &str = "\
- alpha
  - nested
  - nested two
- back out

1. one
  1. inner
  2. inner two
2. two
";
        assert_eq!(indents(NESTED), vec![0, 1, 1, 0, 0, 1, 1, 0]);
    }

    #[test]
    fn sections_come_from_the_blocks() {
        let lines = Lines::parse(SAMPLE);
        assert_eq!(lines.heading_block("token exchange"), Some(2));
        let section = lines.section("Token exchange", usize::MAX).unwrap();
        assert!(section.starts_with("refresh rotation"));
        assert!(section.contains("fn exchange"));
        assert_eq!(lines.words(), lines.plain().split_whitespace().count());
    }

    /// AUDIT n14 — three cases where reading a file somebody else wrote lost
    /// content. The serializer they were found in is gone (ADR-008: the
    /// buffer is the file); the parse half of each still has to hold, or
    /// search and the cards read the wrong note.
    #[test]
    fn audit_n14_a_tab_indented_outline_keeps_its_nesting() {
        // an outline written with Tabs — indent counts spaces, so a Tab read as
        // one character was half a level and rounded down to none
        assert_eq!(
            indents("- alpha\n\t- nested\n\t\t- deeper\n"),
            vec![0, 1, 2],
            "the nesting was flattened"
        );
    }

    #[test]
    fn audit_n14_a_year_is_not_a_list_marker() {
        assert_eq!(kinds("2020. was a year\n")[0], Kind::Paragraph);
        // and a real list marker still is one
        assert_eq!(kinds("7. seven\n")[0], Kind::Ordered);
    }

    #[test]
    fn audit_n14_a_code_block_keeps_its_trailing_blank_lines() {
        // inside a fence the blank lines at the end are the writer's code
        let blocks = Lines::parse("```\nfn a() {}\n\n\n```\n").blocks;
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "fn a() {}\n\n");
    }

    // #34 — files the parser used to misread. Every one of these is a file
    // some other tool wrote, read as the blocks it actually holds.

    #[test]
    fn issue_34_a_fence_remembers_its_own_length() {
        assert_eq!(
            Lines::parse("````\ncode\n````\n").blocks,
            vec![(
                Kind::Code {
                    lang: String::new(),
                    fence: "````".into(),
                },
                "code".into()
            )]
        );
    }

    #[test]
    fn issue_34_a_fence_inside_a_longer_fence_stays_inside() {
        let blocks = Lines::parse("````\n```\ninner\n```\n````\n\nafter\n").blocks;
        // one code block holding the inner fence, then the paragraph — the
        // inner ``` used to close the outer ```` and swallow the file
        assert_eq!(blocks.len(), 2, "the inner fence closed the outer");
        assert_eq!(blocks[0].1, "```\ninner\n```");
    }

    #[test]
    fn issue_34_a_shorter_tilde_run_does_not_close_a_longer_span() {
        let blocks = Lines::parse("~~~~\n~~~\nstill inside\n~~~~\n\nafter\n").blocks;
        assert_eq!(blocks.len(), 2, "the shorter run closed the span");
        assert_eq!(blocks[0].1, "~~~~\n~~~\nstill inside\n~~~~");
    }

    #[test]
    fn issue_34_a_list_items_second_line_stays_in_the_item() {
        let blocks = Lines::parse("- a\n  b\n").blocks;
        assert_eq!(blocks.len(), 1, "the continuation left the item");
        assert_eq!(blocks[0].1, "a\nb");
    }

    #[test]
    fn issue_34_a_lazy_line_under_a_list_item_folds_into_it() {
        let blocks = Lines::parse("- a\nb\n").blocks;
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "a\nb");
    }

    #[test]
    fn issue_34_a_lazy_line_under_a_quote_stays_in_the_quote() {
        let blocks = Lines::parse("> a\nb\n").blocks;
        assert_eq!(blocks, vec![(Kind::Quote, "a\nb".into())]);
    }

    #[test]
    fn issue_34_a_setext_h2_is_not_a_rule() {
        // the underline stays inside the open paragraph, the way `===` does
        let blocks = Lines::parse("Title\n---\n").blocks;
        assert_eq!(blocks.len(), 1, "the heading was split in two");
        // alone on its line, `---` is still the rule it has always been
        assert_eq!(kinds("---\n\nafter\n")[0], Kind::Rule);
    }

    #[test]
    fn issue_34_indented_code_lookalikes_are_not_markdown() {
        // four spaces is code to every other reader, and it is code here now:
        // the heading, the rule and the list marker stay characters
        for src in ["    # x\n", "    ---\n", "    2. x\n"] {
            assert_eq!(
                kinds(src)[0],
                Kind::Code {
                    lang: String::new(),
                    fence: String::new(),
                },
                "{src:?}"
            );
        }
        // ⚠️ words up against the run mean it never stood alone: it stays the
        // verbatim paragraph it always was
        assert!(kinds("    ```\nnot a fence\n")
            .iter()
            .all(|k| *k == Kind::Paragraph));

        // and right under a list item the same depth is the item's own second
        // paragraph — prose, not code
        assert_eq!(kinds("- a\n\n    still the item's\n")[1], Kind::Paragraph);
        // the hanging kind, adjoining, is the item's own line
        assert_eq!(Lines::parse("- a\n\t\t- deep\n").blocks.len(), 2);
    }

    #[test]
    fn issue_34_an_indented_fence_under_a_list_item_opens_nothing() {
        // the list escape hatch is for nested items and continuation text —
        // handed to the fence parser, an indented ``` swallowed the document
        let lines = Lines::parse("- a\n    ```\nafter\n");
        assert!(
            lines
                .blocks
                .iter()
                .all(|(kind, _)| !matches!(kind, Kind::Code { .. })),
            "the indented fence opened a code block"
        );
        assert!(lines.plain().contains("```"), "the line was eaten");
    }

    #[test]
    fn issue_34_a_tilde_closer_takes_no_info_string() {
        // `~~~x` is span content, not a closer — closed there, the rest of
        // the verbatim sample gets parsed as markdown
        assert!(
            kinds("~~~\n~~~ not a closer\n- not a bullet\n~~~\n")
                .iter()
                .all(|k| *k == Kind::Paragraph),
            "the span was closed early and its content parsed"
        );
    }

    #[test]
    fn issue_34_an_indented_closer_is_fence_content() {
        // four columns deep it is the sample's own code, not the closer
        let blocks = Lines::parse("```\n    ```\nstill code\n```\n").blocks;
        assert_eq!(blocks.len(), 1, "the fence closed early");
    }
}
