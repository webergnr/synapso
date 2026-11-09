//! Shortcuts as the platform spells them. The source strings say `⌘`; on
//! Windows and Linux that key is Ctrl, so text reads `Ctrl+` and a shortcut
//! chip draws the design's caret glyph where the Mac draws the cloverleaf.
use freya::prelude::*;

use crate::{icons, theme::*};

pub fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

/// `⌘⇧E` → `Ctrl+⇧E` off the Mac; unchanged on it.
pub fn show(s: &str) -> String {
    if is_mac() {
        s.to_string()
    } else {
        s.replace('⌘', "Ctrl+")
    }
}

/// A bordered shortcut chip: the modifier as an icon, then the rest.
pub fn chip(s: &str) -> impl IntoElement {
    let (has_mod, rest) = match s.strip_prefix('⌘') {
        Some(rest) => (true, rest.to_string()),
        None => (false, s.to_string()),
    };
    rect()
        .horizontal()
        .cross_align(Alignment::Center)
        // the design's kbd: an inset well, as in ⌘K's rows and the esc chip
        .background(c().surface_inset)
        .border(Border::new().fill(c().border_subtle).width(1.))
        .corner_radius(3.)
        .padding((0., 4.))
        .maybe_child(has_mod.then(|| modifier(10., c().text_faint)))
        .child(label().text(rest).font_size(TEXT_XS).color(c().text_faint))
}

/// The modifier glyph alone, for rows that lay their keys out themselves.
pub fn modifier(size: f32, color: Color) -> impl IntoElement {
    icons::icon(
        if is_mac() {
            icons::MOD_MAC
        } else {
            icons::MOD_WIN
        },
        size,
        color,
    )
}

/// The Mac spells Home/End `⌘←`/`⌘→` and the note's ends `⌘↑`/`⌘↓`. The
/// editor only reads the Home/End spelling, where ⌘ (Ctrl elsewhere) means
/// the whole text — so the arrows are handed over as those keys. Off the Mac
/// nothing changes: Home/End are real keys there and Ctrl+← is a word jump.
pub fn line_ends(key: &Key, mods: Modifiers, mac: bool) -> Option<(Key, Modifiers)> {
    if !mac || !mods.contains(Modifiers::META) {
        return None;
    }
    let line = mods - Modifiers::META;
    let whole = line | Modifiers::ctrl_or_meta();
    let (key, mods) = match key {
        Key::Named(NamedKey::ArrowLeft) => (NamedKey::Home, line),
        Key::Named(NamedKey::ArrowRight) => (NamedKey::End, line),
        Key::Named(NamedKey::ArrowUp) => (NamedKey::Home, whole),
        Key::Named(NamedKey::ArrowDown) => (NamedKey::End, whole),
        _ => return None,
    };
    Some((Key::Named(key), mods))
}

#[cfg(test)]
mod tests {
    use super::*;
    use freya::text_edit::{EditorHistory, RopeEditor, TextEditor, TextSelection};

    /// The selection after one press of `key` on the Mac, from a caret at `at`.
    fn press(text: &str, at: usize, key: NamedKey, mods: Modifiers) -> TextSelection {
        let mut ed = RopeEditor::new(
            text.into(),
            TextSelection::new_cursor(at),
            2,
            EditorHistory::new(std::time::Duration::ZERO),
        );
        let (key, mods) =
            line_ends(&Key::Named(key), mods, true).unwrap_or((Key::Named(key), mods));
        ed.process_key(&key, &mods, None, None, false, true, false, false);
        ed.selection().clone()
    }

    #[test]
    fn command_arrows_reach_the_line_and_note_ends_on_the_mac() {
        let text = "first line\nsecond line\nthird";
        // the caret in "sec|ond": the line runs 11..22
        let cmd = Modifiers::META;
        let sel = Modifiers::META | Modifiers::SHIFT;
        assert_eq!(
            press(text, 14, NamedKey::ArrowLeft, cmd),
            TextSelection::Cursor(11)
        );
        assert_eq!(
            press(text, 14, NamedKey::ArrowRight, cmd),
            TextSelection::Cursor(22)
        );
        assert_eq!(
            press(text, 14, NamedKey::ArrowUp, cmd),
            TextSelection::Cursor(0)
        );
        let end = text.len();
        assert_eq!(
            press(text, 14, NamedKey::ArrowDown, cmd),
            TextSelection::Cursor(end)
        );
        // ⇧ selects from the caret to the same ends
        let range = |from, to| TextSelection::Range { from, to };
        assert_eq!(press(text, 14, NamedKey::ArrowLeft, sel), range(14, 11));
        assert_eq!(press(text, 14, NamedKey::ArrowRight, sel), range(14, 22));
        assert_eq!(press(text, 14, NamedKey::ArrowUp, sel), range(14, 0));
        assert_eq!(press(text, 14, NamedKey::ArrowDown, sel), range(14, end));
    }

    #[test]
    fn arrows_keep_their_meaning_without_the_mac_command_key() {
        let left = Key::Named(NamedKey::ArrowLeft);
        // Windows: Ctrl+← is a word jump, Home/End are keys of their own
        assert_eq!(line_ends(&left, Modifiers::CONTROL, false), None);
        assert_eq!(line_ends(&left, Modifiers::META, false), None);
        // the Mac's ⌥← is its word jump
        assert_eq!(line_ends(&left, Modifiers::ALT, true), None);
        assert_eq!(line_ends(&left, Modifiers::empty(), true), None);
        let a = Key::Character("a".into());
        assert_eq!(line_ends(&a, Modifiers::META, true), None);
    }
}
