//! Every word the app says to a person, in one table per language.
//!
//! Same shape as `theme::THEMES`: a table, and adding a language is a line that
//! shows up in the picker by itself. The struct is why — a language that forgets
//! a string does not compile, so there is no missing key to discover at runtime
//! and no `t!("typo")` quietly rendering its own key.
//!
//! ⚠️ Anything that carries a value is a **function**, not a template with
//! `{}`: the compiler checks the arity and the types, and a translator who needs
//! the pieces in another order can build the whole sentence instead of gluing
//! two halves. Plurals are functions for the same reason — `crate::plural` used
//! to append `s` and English itself already broke it (`1 match` / `2 matches`;
//! see `find::summary`).
//!
//! See `docs/i18n-trd.md` for the plan this is phase 1 of.

use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Strings {
    // editor — no open tab
    pub no_open_notes: &'static str,
    pub open_note: &'static str,
    pub open_note_shortcuts: &'static str,

    // status bar — plain
    pub note_gone: &'static str,
    pub undone: &'static str,
    pub nothing_to_undo: &'static str,
    pub nothing_to_redo: &'static str,
    pub only_text_becomes_task: &'static str,
    pub saved: &'static str,
    pub reloaded: &'static str,
    // status bar — the unsaved dot's label
    pub chip_unsaved: &'static str,
    pub chip_saved: &'static str,
    pub nothing_to_copy: &'static str,
    pub pasted_as_text: &'static str,
    pub open_note_before_pasting: &'static str,

    // status bar — with a value
    pub did_not_load: fn(&str, &str) -> String,
    pub changed_on_disk: fn(&str) -> String,
    pub save_failed: fn(&str) -> String,
    pub prefs_save_failed: fn(&str) -> String,
    pub canvas_save_failed: fn(&str) -> String,
    pub no_note_named: fn(&str) -> String,
    pub no_canvas_node_named: fn(&str) -> String,
    pub closed: fn(&str) -> String,
    pub created: fn(&str) -> String,
    pub could_not_create: fn(&str) -> String,
    pub saved_note: fn(&str) -> String,
    pub pasted: fn(&str) -> String,
    pub note_exists: fn(&str) -> String,
    pub renamed_to: fn(&str) -> String,
    pub rename_failed: fn(&str) -> String,
    pub delete_failed: fn(&str) -> String,
    pub empty_vault_at: fn(&str) -> String,
    pub notes_in_vault: fn(usize, &str) -> String,
    pub reloaded_dropped: fn(&str) -> String,
    pub no_canvas_named: fn(&str) -> String,
    pub card_for: fn(&str) -> String,
    pub quote_from: fn(&str) -> String,

    /// ⚠️ These four carry the *reason* another message already wrote, and the
    /// order is load-bearing: the status bar truncates at the END, so the news
    /// has to come first. A translation may rewrite the whole sentence — that is
    /// the point of it being a function — but it must not bury the head.
    pub left_unsaved: fn(&str, &str) -> String,
    pub keeps_its_tab: fn(&str, &str) -> String,
    pub unsaved_in: fn(&str, &str) -> String,
    pub not_created: fn(&str, &str) -> String,
    pub not_renamed: fn(&str) -> String,
    pub not_deleted: fn(&str) -> String,

    // counted things — the plural belongs to the language, not to a helper
    pub words: fn(usize) -> String,
    pub copied_refs: fn(usize) -> String,
    pub pasted_nodes: fn(usize) -> String,
    pub commands: fn(usize) -> String,
    pub results: fn(usize) -> String,
    pub lines: fn(usize) -> String,
    pub nodes: fn(usize) -> String,
    pub edges: fn(usize) -> String,
    pub cards: fn(usize) -> String,
}

pub static EN: Strings = Strings {
    no_open_notes: "no open notes",
    open_note: "open note",
    open_note_shortcuts: "⌘P  open note      ⌘N  new note      ⌘K  search",

    note_gone: "that note is gone",
    undone: "undone",
    nothing_to_undo: "nothing to undo",
    nothing_to_redo: "nothing to redo",
    only_text_becomes_task: "only a line of text becomes a task",
    saved: "saved",
    reloaded: "reloaded from disk",
    chip_unsaved: "unsaved",
    chip_saved: "saved",
    nothing_to_copy: "nothing named to copy",
    pasted_as_text: "pasted as text",
    open_note_before_pasting: "open a note before pasting",

    did_not_load: |name, why| format!("{name} did not load ({why}) — not saving"),
    changed_on_disk: |name| format!("{name} changed on disk — not overwritten (⌘K → reload vault)"),
    save_failed: |e| format!("save failed: {e}"),
    prefs_save_failed: |e| format!("workspace preferences were not saved: {e}"),
    canvas_save_failed: |e| format!("canvas save failed: {e}"),
    no_note_named: |name| format!("no note named “{name}”"),
    no_canvas_node_named: |name| format!("no canvas node named “{name}”"),
    closed: |name| format!("closed {name}"),
    created: |name| format!("created {name}"),
    could_not_create: |e| format!("could not create note: {e}"),
    saved_note: |name| format!("saved {name}"),
    pasted: |file| format!("pasted {file}"),
    note_exists: |name| format!("a note named {name} already exists"),
    renamed_to: |name| format!("renamed to {name}"),
    rename_failed: |e| format!("rename failed: {e}"),
    delete_failed: |e| format!("delete failed: {e}"),
    empty_vault_at: |path| format!("empty vault at {path}"),
    // ⚠️ the plural of "note" is spelled HERE, not in a shared field: a language
    // that copied a field reference would silently pluralize in English
    notes_in_vault: |n, path| format!("{} in {path}", plural(n, "note", "notes")),
    reloaded_dropped: |names| format!("reloaded — unsaved edits dropped in {names}"),
    no_canvas_named: |board| format!("no canvas named \u{201c}{board}\u{201d}"),
    card_for: |name| format!("card for {name}"),
    quote_from: |note| format!("quote from {note}"),

    left_unsaved: |name, why| format!("{name} left unsaved — {why}"),
    keeps_its_tab: |name, why| format!("{name} keeps its tab — {why}"),
    unsaved_in: |names, why| format!("unsaved in {names} — {why}"),
    not_created: |name, why| format!("{name} not created — {why}"),
    not_renamed: |why| format!("not renamed — {why}"),
    not_deleted: |why| format!("not deleted — {why}"),

    words: |n| plural(n, "word", "words"),
    copied_refs: |n| format!("copied — {}", plural(n, "reference", "references")),
    pasted_nodes: |n| format!("pasted — {}", plural(n, "node", "nodes")),
    commands: |n| plural(n, "command", "commands"),
    results: |n| plural(n, "result", "results"),
    lines: |n| plural(n, "line", "lines"),
    nodes: |n| plural(n, "node", "nodes"),
    edges: |n| plural(n, "edge", "edges"),
    cards: |n| plural(n, "card", "cards"),
};

/// Two-form plural, for the languages that have exactly two. A language with
/// more forms writes its own function instead of reaching for this.
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `(tag, name shown to a person, strings)`. The tag is what a config file or
/// an OS locale is matched against; the name is what the picker shows, always
/// in its own language.
pub const LANGS: &[(&str, &str, &Strings)] = &[("en", "English", &EN)];

/// ⚠️ Process-wide, like `SYNAPSO_VAULT` in the tests: one window, one user, and
/// the shell rebuilds the view every frame, so a change repaints with no
/// bookkeeping.
/// The tests pin it through the same lock the harness already holds.
static LANG: AtomicUsize = AtomicUsize::new(0);

/// The strings in force. Call it at the point of use, never cache it.
pub fn s() -> &'static Strings {
    LANGS[LANG.load(Ordering::Relaxed)].2
}

/// Out-of-range picks English rather than panicking: a config file or an OS
/// locale is somebody else's data, and a stale index must not take the app down.
///
/// ⚠️ Nothing in the binary calls this yet, and that is the honest state: with
/// one language there is nothing to switch to. Phase 3 of the TRD adds the ⌘K
/// row and the OS-locale probe that do call it.
#[cfg_attr(not(test), allow(dead_code))]
pub fn set(index: usize) {
    LANG.store(
        if index < LANGS.len() { index } else { 0 },
        Ordering::Relaxed,
    );
}

/// English, whatever the current language is — the tests assert against this so
/// they check the *message*, not a copy of its text that drifts. Not gated
/// behind `cfg(test)`: the shell's own tests call it through the re-export,
/// and a dependency is never compiled with that cfg.
#[cfg_attr(not(test), allow(dead_code))]
pub fn en() -> &'static Strings {
    &EN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_answers_for_every_string() {
        // the struct is the completeness check at compile time; this only proves
        // the table is wired and nothing is empty
        for (tag, name, strings) in LANGS {
            assert!(!tag.is_empty() && !name.is_empty());
            assert!(!strings.no_open_notes.is_empty(), "{tag}");
            assert!(!strings.open_note_before_pasting.is_empty(), "{tag}");
            assert!(
                !(strings.prefs_save_failed)("disk full").is_empty(),
                "{tag}"
            );
            assert!(!strings.undone.is_empty(), "{tag}");
            assert!(!(strings.closed)("x").is_empty(), "{tag}");
            assert!((strings.words)(1) != (strings.words)(2), "{tag}: no plural");
        }
    }

    #[test]
    fn an_index_from_outside_falls_back_to_english() {
        set(999);
        assert_eq!(s().undone, EN.undone);
        set(0);
    }

    #[test]
    fn the_composed_messages_lead_with_the_news() {
        // the status bar truncates at the end, so the note's name and what
        // happened have to survive a cut
        let why = (EN.changed_on_disk)("zeta");
        for line in [
            (EN.left_unsaved)("zeta", &why),
            (EN.not_created)("beta", &why),
            (EN.keeps_its_tab)("zeta", &why),
            (EN.unsaved_in)("zeta, beta", &why),
        ] {
            assert!(line.len() > why.len(), "the reason is the whole message");
            assert!(!line.starts_with(&why), "the reason came first: {line}");
        }
    }
}
