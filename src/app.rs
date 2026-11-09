//! The app's shared state: one `App` of signal handles, provided at the root
//! and consumed by whatever pane needs it.
use std::{collections::HashSet, path::PathBuf};

use freya::prelude::*;

use crate::index::{DocKind, Index};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Notes,
    Split,
    Canvas,
    Graph,
}

impl View {
    /// The pane ⌘W acts on: the only one on screen, else the one `last`
    /// pressed or opened. The graph leaves it on the note.
    pub fn pane(self, last: DocKind) -> DocKind {
        match self {
            View::Canvas => DocKind::Canvas,
            View::Split => last,
            View::Notes | View::Graph => DocKind::Note,
        }
    }
}

/// What the status bar reads off the editor.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Caret {
    pub row: usize,
    pub col: usize,
    pub words: usize,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Overlay {
    None,
    /// ⌘P: documents, with a folder column scoping the list.
    Picker,
    /// ⌘K: commands and file names.
    Palette,
    /// ⌘⇧F: text across the vault.
    Search,
    /// `[[` in a note: pick what the mention points at.
    Link,
    /// What's in `.trash/`: restore one, or hold to empty it (design 1e).
    Trash,
}

/// Something that can be taken back from the toast (or ⌘Z after it).
#[derive(Clone, PartialEq, Debug)]
pub enum Undo {
    /// Files moved to the trash: (where they went, where they were).
    Trash(Vec<(PathBuf, PathBuf)>),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Toast {
    pub id: u32,
    pub text: String,
    pub detail: String,
    pub undo: Option<Undo>,
}

/// Where the colour popover opened, and for which folder or document.
#[derive(Clone, PartialEq, Debug)]
pub struct ColorPick {
    pub key: String,
    pub x: f32,
    pub y: f32,
    /// The rainbow strip is showing instead of the swatches.
    pub strip: bool,
    /// Where the pointer (or the arrows) sit on the strip: hue, lightness.
    pub pad: Option<(f32, f32)>,
}

impl ColorPick {
    pub fn at(key: String, x: f32, y: f32) -> Self {
        Self {
            key,
            x,
            y,
            strip: false,
            pad: None,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct GraphState {
    pub focus: String,
    pub history: Vec<String>,
    pub branch: bool,
    pub depth: usize,
    pub hidden: HashSet<String>,
    /// Weeks back from now the history scrubber sits at; 0 = today.
    pub week: usize,
}

#[derive(Clone, Copy, PartialEq)]
pub struct App {
    pub index: State<Index>,
    pub tabs: State<Vec<String>>,
    /// The boards left open: the board switcher's list, as `tabs` is the rail's.
    pub canvases: State<Vec<String>>,
    pub note: State<String>,
    pub canvas: State<String>,
    /// The pane last pressed, or the kind last opened (`View::pane`).
    pub pane: State<DocKind>,
    pub view: State<View>,
    /// Where ⌘G (or the status bar's graph button) goes back to.
    pub last_view: State<View>,
    pub split: State<f32>,
    pub tree: State<bool>,
    /// Docked beside the rail (5a) instead of floating over the note (5c).
    pub tree_pinned: State<bool>,
    pub dark: State<bool>,
    pub overlay: State<Overlay>,
    pub toast: State<Option<Toast>>,
    pub caret: State<Caret>,
    pub dirty: State<HashSet<String>>,
    pub color_pick: State<Option<ColorPick>>,
    pub graph: State<GraphState>,
    /// Bumped to ask the open note to flush to disk now.
    pub flush: State<u32>,
    /// A document freshly created whose name is being typed in place.
    pub naming: State<Option<String>>,
    /// The last undoable action, kept after its toast is gone (⌘Z).
    pub last_undo: State<Option<Undo>>,
    /// A document being dragged out of the tree, and where it hovers over
    /// the board (pane coordinates).
    pub drag_doc: State<Option<String>>,
    pub drop_at: State<Option<(f64, f64)>>,
    /// Bumped when the open board must reload from disk (a rename rewrote it).
    pub board_rev: State<u64>,
    /// Bumped on every write of a note. A section card on the board quotes
    /// a note's middle, which a save can change without moving anything the
    /// index keeps.
    pub note_rev: State<u64>,
    /// A card a `[[canvas: board#…]]` mention asked to see: the board's rel and
    /// the card as the mention spells it (`id:3`, or a caption). That board
    /// takes it once it is on screen with a size.
    pub reveal: State<Option<(String, String)>>,
    /// Where a `[[note#Heading]]` or `[[note#^id]]` mention (or a section
    /// card) asked the note to put its caret: the note's rel and the section
    /// as the link spells it. That note takes it once it is on screen with a
    /// size.
    pub land: State<Option<(String, String)>>,
    /// ⌘F in the open note: the query, and which hit is current.
    pub find: State<Option<String>>,
    pub find_at: State<usize>,
    /// Text the open note should insert at its caret (the `[[` picker's answer).
    pub insert: State<Option<String>>,
    /// The tree's right-click menu: on what, and where.
    pub tree_menu: State<Option<(String, f32, f32)>>,
    /// The open board's zoom, for the status bar.
    pub canvas_zoom: State<f64>,
    /// The note's right-click menu: where, on which note, with what quote.
    pub note_menu: State<Option<(f32, f32, String, String)>>,
}

thread_local! {
    static MODS: std::cell::Cell<(bool, bool)> = const { std::cell::Cell::new((false, false)) };
}

/// Ctrl/⌘ and Shift as the keyboard last left them: the board asks while a
/// wheel or a drag is running, which carry no modifiers of their own.
pub fn set_mods(e: &KeyboardEventData, pressed: bool) {
    MODS.with(|m| m.set(mods_after(e, pressed)));
}

/// ⚠️ winit on macOS sends a modifier key's own press and release before it
/// updates the modifier state, so a lone ⇧ press arrives saying Shift is up
/// and its release saying it is down. A key that IS a modifier is believed
/// over the state it carries.
fn mods_after(e: &KeyboardEventData, pressed: bool) -> (bool, bool) {
    let (mut ctrl, mut shift) = (crate::ctrl(e), e.modifiers.contains(Modifiers::SHIFT));
    match e.key {
        Key::Named(NamedKey::Shift) => shift = pressed,
        // ⌃ and ⌘ are one flag: letting go of one leaves the other's say
        Key::Named(NamedKey::Control) => ctrl = pressed || e.modifiers.contains(Modifiers::META),
        Key::Named(NamedKey::Meta) => ctrl = pressed || e.modifiers.contains(Modifiers::CONTROL),
        _ => {}
    }
    (ctrl, shift)
}

/// Shift as the keyboard last left it, for a handler with no `App` in reach.
pub fn shift_held() -> bool {
    MODS.with(|m| m.get().1)
}

impl App {
    pub fn ctrl_down(&self) -> bool {
        MODS.with(|m| m.get().0)
    }

    pub fn shift_down(&self) -> bool {
        shift_held()
    }

    pub fn root(&self) -> PathBuf {
        self.index.read().root.clone()
    }

    pub fn reindex(&self) {
        let root = self.root();
        let mut index = self.index;
        index.set(Index::load(&root));
    }

    /// Open a document: a note goes to the editor, a canvas to the board.
    /// The graph is a toggle over the other three views.
    pub fn toggle_graph(&self) {
        // the graph replaces both panes, and a pane's pending save dies with it
        crate::note::flush_note();
        crate::board::flush_board();
        let mut view = self.view;
        let now = *view.peek();
        if now == View::Graph {
            view.set(*self.last_view.peek());
        } else {
            let mut last = self.last_view;
            last.set(now);
            view.set(View::Graph);
        }
    }

    pub fn open(&self, rel: &str) {
        crate::note::flush_note();
        crate::board::flush_board();
        if !*self.tree_pinned.peek() {
            let mut t = self.tree;
            t.set(false);
        }
        let kind = self.index.read().doc(rel).map(|d| d.kind);
        // tabs are notes only: a board joins the board switcher's list
        let mut list = if kind == Some(DocKind::Canvas) {
            self.canvases
        } else {
            self.tabs
        };
        if !list.read().iter().any(|t| t == rel) {
            list.write().push(rel.to_string());
        }
        crate::session::remember(&self.root(), rel, true);
        let mut pane = self.pane;
        pane.set_if_modified(kind.unwrap_or(DocKind::Note));
        match kind {
            Some(DocKind::Canvas) => {
                let mut c = self.canvas;
                c.set(rel.to_string());
                let mut v = self.view;
                if *v.read() == View::Notes || *v.read() == View::Graph {
                    v.set(View::Split);
                }
            }
            _ => {
                let mut n = self.note;
                n.set(rel.to_string());
                let mut v = self.view;
                if *v.read() == View::Canvas || *v.read() == View::Graph {
                    v.set(View::Split);
                }
            }
        }
    }

    /// Open a note on a section of it (`land`). A note already open stays
    /// mounted, so the place it was read at is only moved, never lost; `#`
    /// with nothing after it names no section.
    pub fn open_at(&self, rel: &str, section: Option<&str>) {
        let mut land = self.land;
        land.set(
            section
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| (rel.to_string(), s.to_string())),
        );
        self.open(rel);
    }

    pub fn close_tab(&self, rel: &str) {
        // an empty pane has nothing to close, and "" must not reach `closed`
        if rel.is_empty() {
            return;
        }
        // the pane that goes takes its pending save with it: write it first,
        // and ⚠️ a refused write keeps it open, as the only copy of the edit
        // (ADR-001: a refusal stops what comes after)
        let note = crate::note::flush_note().filter(|_| *self.note.read() == rel);
        let board = crate::board::flush_board().filter(|_| *self.canvas.read() == rel);
        if let Some(why) = note.or(board) {
            self.toast(format!("{} not closed", crate::base_name(rel)), why, None);
            return;
        }
        let mut tabs = self.tabs;
        tabs.write().retain(|t| t != rel);
        let mut canvases = self.canvases;
        canvases.write().retain(|c| c != rel);
        crate::session::remember(&self.root(), rel, false);
        if *self.note.read() == rel {
            let next = tabs.read().first().cloned().unwrap_or_default();
            let mut n = self.note;
            n.set(next);
        }
        // a board gets here from ⌘W in the board, its × in the switcher, a trash;
        // ⚠️ the pane must land on another board, or it reads as a first launch
        if *self.canvas.read() == rel {
            let root = self.root();
            let next =
                crate::session::next_canvas(&self.index.read(), rel, |r| root.join(r).exists());
            let mut c = self.canvas;
            c.set(next);
        }
    }

    pub fn toast(&self, text: impl Into<String>, detail: impl Into<String>, undo: Option<Undo>) {
        // never reused: the host shows a toast whose id it hasn't seen
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut t = self.toast;
        if undo.is_some() {
            let mut last = self.last_undo;
            last.set(undo.clone());
        }
        t.set(Some(Toast {
            id,
            text: text.into(),
            detail: detail.into(),
            undo,
        }));
    }

    /// Delete = move to `.trash/`, at once, with undo on the toast.
    pub fn trash(&self, rels: &[String]) {
        // what the panes hold reaches disk first, so the trash (and undo) keep it
        crate::note::flush_note();
        crate::board::flush_board();
        let root = self.root();
        let mut moved = vec![];
        for rel in rels {
            let from = root.join(rel.trim_end_matches('/'));
            let moved_to = if rel.ends_with('/') {
                trash_folder(&root, &from)
            } else {
                synapso_core::vault::trash(&root, &from)
            };
            if let Ok(to) = moved_to {
                moved.push((to, from));
                // the boards are no tabs: the switcher's list is asked too
                let open: Vec<String> = self
                    .tabs
                    .read()
                    .iter()
                    .chain(self.canvases.read().iter())
                    .filter(|t| t.starts_with(rel.as_str()))
                    .cloned()
                    .collect();
                for t in open {
                    self.close_tab(&t);
                }
            }
        }
        if moved.is_empty() {
            return;
        }
        self.reindex();
        let text = if moved.len() == 1 {
            format!("{} moved to trash", crate::index::file_name(&moved[0].1))
        } else {
            format!("{} notes moved to trash", moved.len())
        };
        self.toast(text, "", Some(Undo::Trash(moved)));
    }

    pub fn undo(&self, undo: &Undo) {
        match undo {
            Undo::Trash(moved) => {
                for (trashed, original) in moved {
                    // a restore never overwrites: if the name came back, the
                    // file stays in the trash
                    if !original.exists() {
                        let _ = std::fs::rename(trashed, original);
                    }
                }
                self.reindex();
                // the first document comes back open; a folder stays in the tree
                let root = self.root();
                let first = moved
                    .iter()
                    .map(|(_, original)| crate::index::rel_of(&root, original))
                    .find(|rel| self.index.read().doc(rel).is_some());
                if let Some(rel) = first {
                    self.open(&rel);
                }
                let mut t = self.toast;
                t.set(None);
                let mut last = self.last_undo;
                last.set(None);
            }
        }
    }
}

/// A whole folder into `.trash/`, under a name that's free there.
fn trash_folder(root: &std::path::Path, from: &std::path::Path) -> std::io::Result<PathBuf> {
    let dir = root.join(crate::index::TRASH);
    std::fs::create_dir_all(&dir)?;
    let name = from
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut to = dir.join(&name);
    let mut n = 2;
    while to.exists() {
        to = dir.join(format!("{name}-{n}"));
        n += 1;
    }
    std::fs::rename(from, &to)?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⌘W closes what the user is in: a single-pane view says which, a
    /// split asks the pane last pressed (or opened).
    #[test]
    fn cmd_w_acts_on_the_pane_in_focus() {
        use DocKind::{Canvas, Note};
        assert_eq!(View::Split.pane(Canvas), Canvas);
        assert_eq!(View::Split.pane(Note), Note);
        assert_eq!(View::Canvas.pane(Note), Canvas);
        assert_eq!(View::Notes.pane(Canvas), Note);
        assert_eq!(View::Graph.pane(Canvas), Note);
    }

    /// ⚠️ the regression this guards: macOS hands a lone ⇧ press the state
    /// from before it and its release the state from before that, so a drag
    /// with ⇧ held never snapped and one after letting go did.
    #[test]
    fn a_modifier_key_says_what_it_did_whatever_state_it_carries() {
        let key = |k, code, mods| KeyboardEventData::new(Key::Named(k), code, mods);
        let shift = key(NamedKey::Shift, Code::ShiftLeft, Modifiers::empty());
        assert_eq!(mods_after(&shift, true), (false, true));
        let shift = key(NamedKey::Shift, Code::ShiftLeft, Modifiers::SHIFT);
        assert_eq!(mods_after(&shift, false), (false, false));
        let cmd = key(NamedKey::Meta, Code::MetaLeft, Modifiers::SHIFT);
        assert_eq!(mods_after(&cmd, true), (true, true));
        assert_eq!(mods_after(&cmd, false), (false, true));
        // any other key reports the state as it is
        let a = KeyboardEventData::new(Key::Character("a".into()), Code::KeyA, Modifiers::META);
        assert_eq!(mods_after(&a, true), (true, false));
    }

    #[test]
    fn letting_go_of_one_of_ctrl_and_cmd_keeps_the_other_held() {
        let key = |k, code, mods| KeyboardEventData::new(Key::Named(k), code, mods);
        // the release says both are down (macOS) or only the other one is: either
        // way the other one is still held
        let both = Modifiers::CONTROL | Modifiers::META;
        let cmd = key(NamedKey::Meta, Code::MetaLeft, both);
        assert_eq!(mods_after(&cmd, false), (true, false));
        let ctrl = key(NamedKey::Control, Code::ControlLeft, both);
        assert_eq!(mods_after(&ctrl, false), (true, false));
        let cmd = key(NamedKey::Meta, Code::MetaLeft, Modifiers::CONTROL);
        assert_eq!(mods_after(&cmd, false), (true, false));
        // and one held alone, let go, is let go
        let cmd = key(NamedKey::Meta, Code::MetaLeft, Modifiers::META);
        assert_eq!(mods_after(&cmd, false), (false, false));
    }
}
