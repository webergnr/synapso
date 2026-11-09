//! ⌘F: one query, every surface.
//!
//! The point of the feature is that a note and a diagram are the same
//! workspace — so a match in a canvas node counts exactly as much as one in
//! the note next to it, and both light up at once. Everything here is pure:
//! the painting lives in `inline.rs` (editor) and `board.rs` (canvas).

use std::ops::Range;

use crate::vault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    Notes,
    Canvas,
}

impl Scope {
    /// In the order the design's segmented control shows them.
    pub const ALL: [Scope; 3] = [Scope::All, Scope::Notes, Scope::Canvas];

    pub fn label(self) -> &'static str {
        match self {
            Scope::All => "all",
            Scope::Notes => "notes",
            Scope::Canvas => "canvas",
        }
    }

    fn takes_notes(self) -> bool {
        matches!(self, Scope::All | Scope::Notes)
    }

    fn takes_canvas(self) -> bool {
        matches!(self, Scope::All | Scope::Canvas)
    }
}

/// What to look for. Cheap to clone and compare — it rides along in the
/// highlighter's settings, which the widget diffs on every draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle {
    pub text: String,
    pub case: bool,
}

impl Needle {
    /// The needle as both sides get compared: lowercased once, unless the
    /// search is case-sensitive.
    fn folded(&self) -> Vec<char> {
        if self.case {
            self.text.chars().collect()
        } else {
            self.text.to_lowercase().chars().collect()
        }
    }

    /// Byte ranges of every match in `haystack`.
    pub fn find_all(&self, haystack: &str) -> Vec<Range<usize>> {
        let needle = self.folded();
        if needle.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut at = 0;
        while at < haystack.len() {
            match match_at(&haystack[at..], &needle, self.case) {
                Some(len) if len > 0 => {
                    out.push(at..at + len);
                    at += len;
                }
                _ => at += haystack[at..].chars().next().map_or(1, char::len_utf8),
            }
        }
        out
    }

    /// Is it in there at all — stops at the first match, and never builds a
    /// list. This is the cheap gate in front of the per-block walk, so it has
    /// to fold case exactly the way `find_all` does: a gate that is stricter
    /// than the walk behind it drops the note before anyone looks at it.
    pub fn hits(&self, haystack: &str) -> bool {
        let needle = self.folded();
        !needle.is_empty()
            && haystack
                .char_indices()
                .any(|(i, _)| match_at(&haystack[i..], &needle, self.case).is_some())
    }
}

/// Does `hay` open with `needle` (already folded)? Gives back how many bytes of
/// `hay` the match ate — folding is per character, so a match can be longer or
/// shorter than the needle (`İ` grows, `ẞ` shrinks).
///
/// ⚠️ Character by character, never `hay.to_lowercase().find(..)`: every offset
/// here indexes into the ORIGINAL string, and an offset taken from a lowercased
/// copy slices it mid-character. Same reasoning, and same shape, as
/// `palette::match_range` — the two search surfaces have to agree, or ⌘F
/// answers "no matches" for a word ⌘K finds.
fn match_at(hay: &str, needle: &[char], case: bool) -> Option<usize> {
    let mut i = 0;
    let mut used = 0;
    for c in hay.chars() {
        if i == needle.len() {
            break;
        }
        // an ASCII letter folds to itself in one character, and notes are mostly
        // ASCII — `char::to_lowercase` walks Unicode tables for every other one
        let mut want = |lowered: char| {
            let ok = i < needle.len() && needle[i] == lowered;
            i += 1;
            ok
        };
        let ok = if case {
            want(c)
        } else if c.is_ascii() {
            want(c.to_ascii_lowercase())
        } else {
            c.to_lowercase().all(&mut want)
        };
        if !ok {
            return None;
        }
        used += c.len_utf8();
    }
    (i == needle.len()).then_some(used)
}

/// One match, addressed by where it lives rather than by a global offset —
/// the surfaces have nothing else in common.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    Note {
        doc: usize,
        /// Line of the file — the note is one buffer, so a line is a line.
        line: usize,
        /// Byte offset in that line.
        at: usize,
    },
    Node {
        id: String,
    },
}

pub struct Find {
    pub query: String,
    pub case: bool,
    pub scope: Scope,
    pub hits: Vec<Hit>,
    pub active: usize,
}

impl Default for Find {
    fn default() -> Self {
        Self::new()
    }
}

impl Find {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            case: false,
            scope: Scope::All,
            hits: Vec::new(),
            active: 0,
        }
    }

    pub fn needle(&self) -> Option<Needle> {
        (!self.query.is_empty()).then(|| Needle {
            text: self.query.clone(),
            case: self.case,
        })
    }

    /// The needle only when the surface is in scope — so "notes" really does
    /// stop painting the canvas, and the other way round.
    pub fn needle_for(&self, notes: bool) -> Option<Needle> {
        let wanted = if notes {
            self.scope.takes_notes()
        } else {
            self.scope.takes_canvas()
        };
        wanted.then(|| self.needle()).flatten()
    }

    pub fn current(&self) -> Option<&Hit> {
        self.hits.get(self.active)
    }

    /// Wraps around, like every find bar ever written.
    pub fn step(&mut self, by: i32) {
        if self.hits.is_empty() {
            return;
        }
        let n = self.hits.len() as i32;
        self.active = (self.active as i32 + by).rem_euclid(n) as usize;
    }

    /// What the bar prints beside the query — the design's own words
    /// (`design/Workspace.dc.html:320`: `7 matches`, and `5 matches` once the
    /// scope drops the canvas).
    ///
    /// ⚠️ no running `3 of 7`, even though the arrows walk the list. The design
    /// says WHICH match you are on by painting that one brighter — `hlRing`
    /// over `--search-hl-active` — and a number that says the same thing in
    /// words is the kind of chrome this design spends its space not having.
    pub fn summary(&self) -> String {
        summary(&self.query, self.hits.len())
    }
}

/// The same words, said by a shell that keeps its own list of hits — the floem
/// side walks the whole VAULT, so each of its hits carries the file it is in
/// and they do not fit in `Find::hits`. One wording for both, or the two
/// binaries would each round the same sentence their own way.
pub fn summary(query: &str, hits: usize) -> String {
    match (query.is_empty(), hits) {
        (true, _) => String::new(),
        (_, 0) => "no matches".into(),
        // "matchs" is what `plural` would produce, and English disagrees
        (_, 1) => "1 match".into(),
        (_, n) => format!("{n} matches"),
    }
}

/// Where a note hit sits in the buffer.
///
/// A [`Hit::Note`] is addressed by line because that is how a note reads;
/// everything that MOVES a caret counts bytes from the top of the file. This
/// is the one conversion between them, so the two can never drift apart in
/// two callers' arithmetic.
///
/// ⚠️ `+ 1` per line for the `\n` that `split` ate — without it every hit
/// lands one byte earlier per line above it, which looks right on line 1 and
/// is wildly wrong by line 40. And a line past the end clamps to the end
/// rather than running off: the file is allowed to change between the scan
/// that found the hit and the ⏎ that walks to it.
pub fn offset(markdown: &str, line: usize, at: usize) -> usize {
    let mut base = 0;
    for (n, text) in markdown.split('\n').enumerate() {
        if n == line {
            return base + at.min(text.len());
        }
        base += text.len() + 1;
    }
    markdown.len()
}

/// The two sides of a note ⌘F reads. `Doc` is app state and lives in the
/// shell — this is all of it the scan touches, so the shell implements the
/// trait and core stays free of it.
pub trait Scannable {
    /// The whole note as one string — the cheap gate in front of the walk.
    fn plain(&self) -> &str;
    /// The raw file: what the writer sees is what search has to point at.
    fn markdown(&self) -> &str;
}

/// Every match in the workspace, notes first, in reading order.
pub fn scan(find: &Find, docs: &[impl Scannable], nodes: &[vault::Node]) -> Vec<Hit> {
    let _bench = crate::bench::Span::new("find.scan");
    let Some(needle) = find.needle() else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    if find.scope.takes_notes() {
        for (doc, note) in docs.iter().enumerate() {
            // the note as one string is already there (Doc::plain); asking it
            // first turns a hundred notes' worth of block reads into one scan
            if !needle.hits(note.plain()) {
                continue;
            }
            // the raw file, not the derived tree: what the writer sees in the
            // buffer is what a search has to be able to point at
            for (line, text) in note.markdown().split('\n').enumerate() {
                for range in needle.find_all(text) {
                    hits.push(Hit::Note {
                        doc,
                        line,
                        at: range.start,
                    });
                }
            }
        }
    }
    if find.scope.takes_canvas() {
        for node in nodes {
            if needle.hits(node.caption())
                || node.excerpt.as_deref().is_some_and(|e| needle.hits(e))
            {
                hits.push(Hit::Node {
                    id: node.id.clone(),
                });
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn needle(text: &str, case: bool) -> Needle {
        Needle {
            text: text.into(),
            case,
        }
    }

    #[test]
    fn matching_is_case_insensitive_until_it_is_not() {
        let hay = "Auth flow, oauth, AUTH";
        assert_eq!(needle("auth", false).find_all(hay).len(), 3);
        // case-sensitively, only the tail of "oauth" is lowercase "auth"
        assert_eq!(needle("auth", true).find_all(hay), vec![12..16]);
        assert!(needle("", false).find_all(hay).is_empty());
        assert!(needle("nothing", false).find_all(hay).is_empty());
    }

    /// AUDIT n14 (in `night14-evidence-no-fix.patch`, off the list) — ⌘F folded
    /// case with `eq_ignore_ascii_case` while ⌘K used `to_lowercase`. The two
    /// search surfaces disagreed the moment case had to be folded on a
    /// non-ASCII letter — `AÇÃO` against `ação` — and ⌘F answered "no matches"
    /// for a word that is right there on screen.
    #[test]
    fn find_and_the_palette_agree_on_accents() {
        const HAY: &str = "plano de ação urgente";
        let acao = needle("AÇÃO", false);
        assert!(acao.hits(HAY), "⌘F, case-insensitive, misses ação");
        let ranges = acao.find_all(HAY);
        assert_eq!(ranges.len(), 1);
        assert_eq!(
            &HAY[ranges[0].clone()],
            "ação",
            "the range came out crooked"
        );

        // and case-sensitive stays case-sensitive
        assert!(!needle("AÇÃO", true).hits(HAY));
        assert!(needle("ação", true).hits(HAY));

        // what the palette already did, now on both sides
        assert!(needle("İ", false).hits("İstanbul") || needle("i̇", false).hits("İstanbul"));
    }

    #[test]
    fn a_match_never_splits_a_character() {
        // the needle's bytes appear inside a multi-byte char's encoding
        let hay = "café ☕ auth";
        let ranges = needle("auth", false).find_all(hay);
        assert_eq!(ranges.len(), 1);
        assert_eq!(&hay[ranges[0].clone()], "auth");
        // and an accented needle still matches itself
        assert_eq!(needle("café", false).find_all(hay).len(), 1);
    }

    /// The conversion ⏎ walks through: a hit is addressed by line, a caret by
    /// byte. Anything off here parks the caret in the wrong word — or, on a
    /// note with accents, inside a character.
    #[test]
    fn a_line_and_a_column_come_back_out_as_one_offset() {
        const NOTE: &str = "# Auth\n\nthe gateway\nção here\n";
        assert_eq!(offset(NOTE, 0, 2), 2);
        // line 2 starts after "# Auth\n" (7) and the blank line (1)
        assert_eq!(offset(NOTE, 2, 4), 12);
        assert_eq!(&NOTE[offset(NOTE, 2, 4)..offset(NOTE, 2, 11)], "gateway");
        // and a multi-byte line counts BYTES the whole way: `ção` is five of
        // them, so "here" opens at 6 and not at the fourth character
        assert_eq!(&NOTE[offset(NOTE, 3, 6)..offset(NOTE, 3, 10)], "here");
        // past the end of the line clamps to the end of the line, and past the
        // end of the file to the end of the file — the note is allowed to have
        // changed between the scan and the ⏎
        assert_eq!(offset(NOTE, 0, 99), 6);
        assert_eq!(offset(NOTE, 99, 0), NOTE.len());
    }

    #[test]
    fn stepping_wraps_in_both_directions() {
        let mut find = Find::new();
        find.hits = vec![Hit::Node { id: "a".into() }, Hit::Node { id: "b".into() }];
        find.step(1);
        assert_eq!(find.active, 1);
        find.step(1);
        assert_eq!(find.active, 0);
        find.step(-1);
        assert_eq!(find.active, 1);
    }

    #[test]
    fn one_query_reaches_both_surfaces_and_the_scope_narrows_it() {
        // never opened, which is how most of a vault stays: search has to
        // reach it anyway
        struct Note {
            text: &'static str,
        }
        impl Scannable for Note {
            fn plain(&self) -> &str {
                self.text
            }
            fn markdown(&self) -> &str {
                self.text
            }
        }
        let doc = Note {
            text: "# Auth\n\nthe token is minted once\n",
        };
        let node = vault::Node::new(
            "1".into(),
            vault::Shape::Rect,
            kurbo::Rect::from_origin_size(kurbo::Point::ORIGIN, kurbo::Size::new(10.0, 10.0)),
            "token store",
        );

        let mut find = Find::new();
        find.query = "token".into();
        find.hits = scan(
            &find,
            std::slice::from_ref(&doc),
            std::slice::from_ref(&node),
        );
        assert_eq!(
            find.hits,
            vec![
                Hit::Note {
                    doc: 0,
                    line: 2,
                    at: 4
                },
                Hit::Node { id: "1".into() },
            ]
        );

        find.scope = Scope::Canvas;
        let only_canvas = scan(
            &find,
            std::slice::from_ref(&doc),
            std::slice::from_ref(&node),
        );
        assert_eq!(only_canvas, vec![Hit::Node { id: "1".into() }]);
        // and the painter is told to skip the surface that is out of scope
        assert!(find.needle_for(true).is_none());
        assert!(find.needle_for(false).is_some());
    }
}
