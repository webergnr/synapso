//! The app's shared state: one `App` of signal handles, provided at the root
//! and consumed by whatever pane needs it.
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

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
    /// Tree folders opened or closed by hand; the rest follow `tree::rows`.
    pub tree_open: State<HashMap<String, bool>>,
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
        if !self.flush("graph") {
            return;
        }
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
        self.try_open(rel);
    }

    /// False when a refused flush kept the panes as they are.
    fn try_open(&self, rel: &str) -> bool {
        let kind = self.index.read().doc(rel).map(|d| d.kind);
        // what is already open stays mounted, and keeps its edit. ⚠️ by the
        // pane it goes to: a board gone from the index opens as a note
        let here = match kind {
            Some(DocKind::Canvas) => *self.canvas.read() == rel,
            _ => *self.note.read() == rel,
        };
        if !here && !self.flush("open") {
            return false;
        }
        if !*self.tree_pinned.peek() {
            let mut t = self.tree;
            t.set(false);
        }
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
        true
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
        if !self.try_open(rel) {
            land.set(None);
        }
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

    /// Write what both panes hold before something unmounts them. False, with
    /// a toast, when an edit is still on screen only ([`flush_panes`]): the
    /// caller must stop.
    pub fn flush(&self, action: &'static str) -> bool {
        match flush_panes(action) {
            Ok(()) => true,
            Err(why) => {
                self.toast("not saved: do it again to leave it unsaved", why, None);
                false
            }
        }
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

/// Write what the note and the board panes hold, before something unmounts
/// them. `Err(why)` when an edit is still on screen only (the write and its
/// `-unsaved` rescue both failed): whatever was about to unmount the pane must
/// not, or the edit dies with it (ADR-001: a refusal stops what comes after).
///
/// ⚠️ The same action refused again, pressed anew ([`FRESH`]) within
/// [`AGAIN`], goes through: that is the user choosing to leave the edit
/// unsaved, and a vault that stays unwritable (a full disk, an unplugged
/// drive) must not lock every tab, and the window, shut. Another action,
/// another reason, or a refusal from long ago stops again.
pub fn flush_panes(action: &'static str) -> Result<(), String> {
    let note = crate::note::flush_note();
    let board = crate::board::flush_board();
    let now = std::time::Instant::now();
    REFUSED.with(|last| insist(&mut last.borrow_mut(), note.or(board), action, now))
}

/// A refused flush, remembered so the user's repeat of it can go through.
struct Refusal {
    why: String,
    /// What it stopped: only the same action again is a repeat.
    action: &'static str,
    at: std::time::Instant,
    /// The repeat went through: what that action goes on to do (`open`, after
    /// ⌘N made the note) goes through too, until [`AGAIN`] runs out.
    released: bool,
}

thread_local! {
    /// The last refused flush (`insist`).
    static REFUSED: std::cell::RefCell<Option<Refusal>> = const { std::cell::RefCell::new(None) };
}

/// How soon the same refusal again is a repeat: while its toast is up (3 s),
/// and a little after.
const AGAIN: std::time::Duration = std::time::Duration::from_secs(5);

/// A repeat sooner than this is a held key's, not a second press: keys repeat
/// after about half a second, and each repeat starts the clock again.
const FRESH: std::time::Duration = std::time::Duration::from_secs(1);

fn insist(
    last: &mut Option<Refusal>,
    why: Option<String>,
    action: &'static str,
    now: std::time::Instant,
) -> Result<(), String> {
    let Some(why) = why else {
        *last = None;
        return Ok(());
    };
    if let Some(r) = last.as_mut().filter(|r| r.why == why) {
        let age = now.duration_since(r.at);
        let pass = if r.released {
            age < AGAIN
        } else {
            r.action == action && (FRESH..AGAIN).contains(&age)
        };
        if pass {
            r.released = true;
            return Ok(());
        }
    }
    *last = Some(Refusal {
        why: why.clone(),
        action,
        at: now,
        released: false,
    });
    Err(why)
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

    /// A refused flush stops what was about to unmount the pane. Only the same
    /// action again, pressed anew, goes through: that is the user choosing to
    /// leave the edit unsaved. A write that works clears the slate.
    #[test]
    fn a_refusal_stops_once_and_only_a_fresh_repeat_goes_through() {
        let full = || Some("disk full".to_string());
        let t0 = std::time::Instant::now();
        let at = |ms: u64| t0 + std::time::Duration::from_millis(ms);
        let stop = Err("disk full".to_string());
        let mut last = None;
        assert_eq!(insist(&mut last, None, "graph", at(0)), Ok(()));
        assert_eq!(insist(&mut last, full(), "graph", at(0)), stop);
        // a held key repeats in well under a second, and keeps the clock back
        assert_eq!(insist(&mut last, full(), "graph", at(500)), stop, "held");
        assert_eq!(
            insist(&mut last, full(), "graph", at(1000)),
            stop,
            "held on"
        );
        // another action is not a repeat: the close after a refused ⌘G stops
        assert_eq!(
            insist(&mut last, full(), "close", at(3000)),
            stop,
            "another action"
        );
        assert_eq!(
            insist(&mut last, full(), "close", at(4500)),
            Ok(()),
            "close again"
        );
        // and what it goes on to do goes through with it, for a while
        assert_eq!(
            insist(&mut last, full(), "open", at(4500)),
            Ok(()),
            "released"
        );
        assert_eq!(
            insist(&mut last, full(), "open", at(3000) + AGAIN),
            stop,
            "long after"
        );
        assert_eq!(
            insist(&mut last, Some("read-only".into()), "open", at(9000)),
            Err("read-only".into()),
            "another reason"
        );
        assert_eq!(insist(&mut last, None, "open", at(9500)), Ok(()));
        assert_eq!(
            insist(&mut last, Some("read-only".into()), "open", at(10_000)),
            Err("read-only".into()),
            "a write that worked in between"
        );
    }

    /// What stops a transition is an edit nothing on disk holds: a rescue to
    /// `-unsaved` counts as written.
    #[test]
    fn only_an_edit_left_on_screen_stops_a_transition() {
        use synapso_core::vault::{Saved, save_note, stamp};
        let dir = std::env::temp_dir().join("synapso-flush-refusal");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("plan.md");
        std::fs::write(&note, "loaded").unwrap();
        let loaded = stamp(&note);
        std::fs::write(&note, "someone else's, longer").unwrap();

        let rescued = save_note(&note, "mine", loaded, None);
        assert!(matches!(rescued, Saved::Rescued(_)), "{rescued:?}");
        // a rescue that will not write: a folder holding something is in its way
        let blocked = dir.join("blocked-unsaved.md");
        std::fs::create_dir_all(blocked.join("inside")).unwrap();
        let failed = save_note(&note, "mine", loaded, Some(&blocked));

        let mut last = None;
        let now = std::time::Instant::now();
        let why = |s: &Saved| s.unsaved().map(str::to_string);
        assert_eq!(insist(&mut last, why(&rescued), "open", now), Ok(()));
        assert!(insist(&mut last, why(&failed), "open", now).is_err());
    }

    /// Every flush whose answer is dropped is one where nothing unmounts the
    /// pane, so the edit is still there to save. Anywhere else a refusal has
    /// to stop what comes next (`flush_panes`).
    #[test]
    fn no_flush_that_unmounts_a_pane_drops_its_refusal() {
        // (file, function, why dropping the answer loses nothing)
        const KEPT: [(&str, &str, &str); 4] = [
            (
                "app.rs",
                "trash",
                "close_tab, right after, flushes again and refuses",
            ),
            (
                "graph.rs",
                "pin_to_canvas",
                "only in the graph, where no board is mounted",
            ),
            ("overlays.rs", "run", "⌘S: nothing unmounts"),
            (
                "status.rs",
                "render",
                "a view switch keeps both panes mounted",
            ),
        ];
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut dropped = vec![];
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let mut func = String::new();
            for (n, line) in text.lines().enumerate() {
                let t = line.trim();
                if let Some(rest) = ["fn ", "pub fn ", "pub(crate) fn "]
                    .iter()
                    .find_map(|p| t.strip_prefix(p))
                {
                    func = rest.split(['(', '<']).next().unwrap_or("").to_string();
                }
                let drops = (t.ends_with("flush_note();") || t.ends_with("flush_board();"))
                    && (!t.contains('=') || t.starts_with("let _"));
                if drops
                    && !KEPT
                        .iter()
                        .any(|(f, func_, _)| *f == file && *func_ == func)
                {
                    dropped.push(format!("src/{file}:{} in {func}", n + 1));
                }
            }
        }
        assert!(dropped.is_empty(), "a refusal is dropped at {dropped:#?}");
    }
}
