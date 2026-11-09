//! What the app reopens with: the notes left on the rail, the boards left in
//! the board switcher, the note and the board last looked at. Boards are never
//! tabs. Kept in the vault's sidecar: what was closed (a tab off the rail, a
//! board out of the switcher) and the documents most recently opened.
//!
//! ⚠️ both lists key each file as its colour does ([`color_key`]), so
//! `plan.md` and `plan.canvas` close apart. The Floem shell kept bare names;
//! the first restore moves those onto every file of that name, which is what
//! they meant there.
use std::path::Path;

use synapso_core::vault::Prefs;

use crate::index::{DocEntry, DocKind, Index, color_key};

pub struct Session {
    pub tabs: Vec<String>,
    /// The boards left open: the board switcher's list.
    pub canvases: Vec<String>,
    pub note: String,
    pub canvas: String,
}

/// The rail and the two panes for `index`'s vault, from its sidecar.
pub fn restore(index: &Index) -> Session {
    let mut prefs = Prefs::load(&index.root);
    let keys: Vec<String> = index.docs.iter().map(|d| color_key(&d.rel)).collect();
    let docs: Vec<(&str, &str)> = index
        .docs
        .iter()
        .zip(&keys)
        .map(|(d, key)| (d.stem.as_str(), key.as_str()))
        .collect();
    // a sidecar that cannot be written keeps its names and migrates again
    // next launch; nothing else writes those lists
    if prefs.migrate_names(&docs) {
        let _ = prefs.save(&index.root);
    }
    let open: Vec<&DocEntry> = index
        .docs
        .iter()
        .filter(|d| prefs.is_open(&color_key(&d.rel)))
        .collect();
    // the most recently opened of a kind, else the first one open
    let last = |kind: DocKind| {
        prefs
            .recent()
            .iter()
            .find_map(|key| {
                open.iter()
                    .find(|d| d.kind == kind && color_key(&d.rel) == *key)
            })
            .or_else(|| open.iter().find(|d| d.kind == kind))
            .map(|d| d.rel.clone())
            .unwrap_or_default()
    };
    let of = |kind: DocKind| -> Vec<String> {
        open.iter()
            .filter(|d| d.kind == kind)
            .map(|d| d.rel.clone())
            .collect()
    };
    Session {
        note: last(DocKind::Note),
        canvas: last(DocKind::Canvas),
        // ⚠️ notes only: a board is reached through the board switcher, which
        // lists `canvases`, and the last one open comes back via `canvas`
        tabs: of(DocKind::Note),
        canvases: of(DocKind::Canvas),
    }
}

/// The board to show when `gone` leaves the screen: the most recently opened
/// other open one, else the first open, else none. `exists` is asked because a
/// trash runs before the index reloads, so the index still lists what it moved.
pub fn next_canvas(index: &Index, gone: &str, exists: impl Fn(&str) -> bool) -> String {
    let prefs = Prefs::load(&index.root);
    let boards: Vec<&DocEntry> = index
        .docs
        .iter()
        .filter(|d| {
            d.kind == DocKind::Canvas
                && d.rel != gone
                && prefs.is_open(&color_key(&d.rel))
                && exists(&d.rel)
        })
        .collect();
    prefs
        .recent()
        .iter()
        .find_map(|key| boards.iter().find(|d| color_key(&d.rel) == *key))
        .or(boards.first())
        .map(|d| d.rel.clone())
        .unwrap_or_default()
}

/// A document went onto the rail (and was opened) or came off it.
pub fn remember(root: &Path, rel: &str, open: bool) {
    let key = color_key(rel);
    let mut prefs = Prefs::load(root);
    prefs.set_open(&key, open);
    if open {
        prefs.touch(&key);
    }
    let _ = prefs.save(root);
}

/// A document or folder was renamed: its place on the rail and in the
/// recents follows, and a folder's takes what is inside along.
pub fn renamed(root: &Path, from: &str, to: &str) {
    let mut prefs = Prefs::load(root);
    prefs.rename(&color_key(from), &color_key(to));
    let _ = prefs.save(root);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("synapso-session-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, text) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    /// Boards are not rail tabs, yet the last one opened is the one that
    /// comes back on launch.
    #[test]
    fn canvases_stay_off_the_rail_but_the_last_one_reopens() {
        let root = vault(
            "rail",
            &[
                ("a.md", ""),
                ("b.md", ""),
                ("one.canvas", "{}"),
                ("two.canvas", "{}"),
            ],
        );
        remember(&root, "one.canvas", true);
        remember(&root, "two.canvas", true);
        remember(&root, "b.md", true);
        let s = restore(&Index::load(&root));
        assert_eq!(s.tabs, ["a.md", "b.md"]);
        assert_eq!(s.note, "b.md");
        assert_eq!(s.canvas, "two.canvas");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The board switcher lists the open boards: a closed one leaves the
    /// list, an opened one joins it, and a relaunch reads both back.
    #[test]
    fn open_canvases_join_leave_and_persist() {
        let root = vault(
            "open",
            &[
                ("a.md", ""),
                ("one.canvas", "{}"),
                ("dir/two.canvas", "{}"),
                ("three.canvas", "{}"),
            ],
        );
        let all = ["dir/two.canvas", "one.canvas", "three.canvas"];
        assert_eq!(restore(&Index::load(&root)).canvases, all);
        remember(&root, "dir/two.canvas", false);
        remember(&root, "three.canvas", false);
        assert_eq!(restore(&Index::load(&root)).canvases, ["one.canvas"]);
        remember(&root, "three.canvas", true);
        let s = restore(&Index::load(&root));
        assert_eq!(s.canvases, ["one.canvas", "three.canvas"]);
        assert_eq!(s.canvas, "three.canvas");
        assert_eq!(s.tabs, ["a.md"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A note and a board of one name, or two boards of one name in two
    /// folders, close apart — and a folder rename keeps what was closed in it.
    #[test]
    fn files_of_one_name_close_apart() {
        let root = vault(
            "apart",
            &[
                ("plan.md", ""),
                ("plan.canvas", "{}"),
                ("a/plan.canvas", "{}"),
                ("b/plan.canvas", "{}"),
            ],
        );
        remember(&root, "plan.canvas", false);
        remember(&root, "a/plan.canvas", false);
        let s = restore(&Index::load(&root));
        assert_eq!(s.tabs, ["plan.md"]);
        assert_eq!(s.canvases, ["b/plan.canvas"]);
        std::fs::rename(root.join("a"), root.join("c")).unwrap();
        renamed(&root, "a/", "c/");
        assert_eq!(restore(&Index::load(&root)).canvases, ["b/plan.canvas"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A sidecar the Floem shell wrote, by bare name, reopens as it did there
    /// and is rewritten by file, its colours untouched.
    #[test]
    fn a_sidecar_keyed_by_name_reopens_as_it_was() {
        let root = vault(
            "named",
            &[
                ("plan.md", ""),
                ("plan.canvas", "{}"),
                ("x.md", ""),
                ("y.md", ""),
                (
                    ".synapso.json",
                    r#"{"colors":{"x":2},"closed":["plan"],"recent":["y","x"]}"#,
                ),
            ],
        );
        let s = restore(&Index::load(&root));
        assert_eq!(s.tabs, ["x.md", "y.md"]);
        assert!(s.canvases.is_empty());
        assert_eq!(s.note, "y.md");
        let text = std::fs::read_to_string(root.join(".synapso.json")).unwrap();
        assert!(text.contains("closed_files") && !text.contains(r#""closed":"#));
        let prefs = Prefs::load(&root);
        assert_eq!(
            prefs.color("x", 0),
            synapso_core::vault::DocColor::Preset(2)
        );
        assert_eq!(prefs.recent(), ["y", "x"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Closing the board on screen hands the pane to the most recently
    /// opened board still open, never to one closed before.
    #[test]
    fn next_canvas_picks_only_open_boards() {
        let root = vault(
            "nextopen",
            &[("a.canvas", "{}"), ("b.canvas", "{}"), ("c.canvas", "{}")],
        );
        remember(&root, "c.canvas", true);
        remember(&root, "b.canvas", true);
        remember(&root, "b.canvas", false);
        let index = Index::load(&root);
        assert_eq!(next_canvas(&index, "a.canvas", |_| true), "c.canvas");
        remember(&root, "c.canvas", false);
        assert_eq!(next_canvas(&index, "a.canvas", |_| true), "");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Trashing the board on screen hands the pane to another board, never
    /// to a dead one, even while the index still lists the trashed files.
    #[test]
    fn next_canvas_skips_the_gone_and_the_trashed() {
        let root = vault(
            "next",
            &[
                ("a.canvas", "{}"),
                ("dir/b.canvas", "{}"),
                ("c.canvas", "{}"),
            ],
        );
        remember(&root, "c.canvas", true);
        remember(&root, "dir/b.canvas", true);
        let index = Index::load(&root);
        let live = |rel: &str| rel != "dir/b.canvas";
        // b is the latest but trashed with its folder; c is next in recents
        assert_eq!(next_canvas(&index, "a.canvas", live), "c.canvas");
        assert_eq!(next_canvas(&index, "c.canvas", live), "a.canvas");
        assert_eq!(next_canvas(&index, "c.canvas", |r| r == "c.canvas"), "");
        let _ = std::fs::remove_dir_all(&root);
    }
}
