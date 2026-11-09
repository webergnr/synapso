//! Undo, by watching rather than by asking.
//!
//! Every mutating message could push its own snapshot, and every new one could
//! forget to. So nothing does: the history compares the note and the canvas
//! against the last mark on the autosave heartbeat, and keeps the *previous*
//! text when they differ. A burst of typing is one entry, which is the
//! granularity people expect from ⌘Z anyway.

use std::path::{Path, PathBuf};

use crate::vault;

/// A step back. Notes travel as markdown and canvases as JSON: both re-parse
/// into exactly what they were, and neither costs anything to hold onto.
#[derive(Debug, Clone)]
pub enum Step {
    Note {
        doc: PathBuf,
        markdown: String,
        /// The line the caret was on — the note is one buffer, so putting a
        /// step back means putting the caret back on its line.
        line: usize,
    },
    Canvas {
        path: PathBuf,
        json: String,
    },
}

/// Deep enough for a session, short enough to never think about the memory.
const DEPTH: usize = 200;

#[derive(Default)]
pub struct History {
    past: Vec<Step>,
    future: Vec<Step>,
    /// What the note and the canvas looked like at the last mark.
    note: Option<(PathBuf, String)>,
    canvas: Option<(PathBuf, String)>,
}

impl History {
    /// Note the current state, and record the previous one as a step back if
    /// it changed. Called on the heartbeat and before every undo, so a change
    /// still being typed is already on the stack when ⌘Z arrives.
    pub fn mark(
        &mut self,
        note: Option<(PathBuf, String, usize)>,
        canvas: Option<(PathBuf, String)>,
    ) {
        if let Some((path, markdown, line)) = note {
            match &self.note {
                // a different note: start watching it, with nothing to undo yet
                Some((was, _)) if *was != path => self.note = Some((path, markdown)),
                Some((was, before)) if *before != markdown => {
                    self.push(Step::Note {
                        doc: was.clone(),
                        markdown: before.clone(),
                        line,
                    });
                    self.note = Some((path, markdown));
                }
                Some(_) => {}
                None => self.note = Some((path, markdown)),
            }
        }
        if let Some((path, json)) = canvas {
            match &self.canvas {
                Some((was, _)) if *was != path => self.canvas = Some((path, json)),
                Some((was, before)) if *before != json => {
                    self.push(Step::Canvas {
                        path: was.clone(),
                        json: before.clone(),
                    });
                    self.canvas = Some((path, json));
                }
                Some(_) => {}
                None => self.canvas = Some((path, json)),
            }
        }
    }

    fn push(&mut self, step: Step) {
        self.past.push(step);
        if self.past.len() > DEPTH {
            self.past.remove(0);
        }
        // a new edit is a new timeline: whatever was undone is not coming back
        self.future.clear();
    }

    /// The step to apply, and where to put the state it replaces.
    ///
    /// ⚠️ `current` is handed the popped step and must answer for *that*
    /// step's document. One chronological stack holds both surfaces, so the
    /// thing ⌘Z moves is whatever was edited last — not whatever is on screen.
    /// Reading the visible surface instead paired redo with the wrong file:
    /// undo a note while looking at the canvas, and the note could never come
    /// back.
    pub fn undo(&mut self, current: impl FnOnce(&Step) -> Option<Step>) -> Option<Step> {
        let step = self.past.pop()?;
        if let Some(now) = current(&step) {
            self.future.push(now);
        }
        self.adopt(&step);
        Some(step)
    }

    pub fn redo(&mut self, current: impl FnOnce(&Step) -> Option<Step>) -> Option<Step> {
        let step = self.future.pop()?;
        if let Some(now) = current(&step) {
            self.past.push(now);
        }
        self.adopt(&step);
        Some(step)
    }

    /// After travelling, the mark *is* where we landed — otherwise the next
    /// heartbeat would read the jump as a fresh edit and push it right back.
    fn adopt(&mut self, step: &Step) {
        match step {
            Step::Note { doc, markdown, .. } => self.note = Some((doc.clone(), markdown.clone())),
            Step::Canvas { path, json } => self.canvas = Some((path.clone(), json.clone())),
        }
    }

    /// A rename moves the file; the steps recorded for it move too. Left
    /// behind, every one of them dies as "that note is gone", and the pending
    /// diff is dropped by `mark`'s path-change reset — a rename would sever
    /// the note's whole history.
    pub fn rename(&mut self, from: &Path, to: &Path) {
        for step in self.past.iter_mut().chain(self.future.iter_mut()) {
            let (Step::Note { doc: p, .. } | Step::Canvas { path: p, .. }) = step;
            if p.as_path() == from {
                *p = to.to_path_buf();
            }
        }
        if let Some((p, _)) = &mut self.note {
            if p.as_path() == from {
                *p = to.to_path_buf();
            }
        }
        if let Some((p, _)) = &mut self.canvas {
            if p.as_path() == from {
                *p = to.to_path_buf();
            }
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
}

/// The canvas as text, for comparing and for travelling back to.
pub fn canvas_json(canvas: &vault::Canvas) -> String {
    serde_json::to_string(canvas).unwrap_or_default()
}

pub fn parse_canvas(json: &str) -> Option<vault::Canvas> {
    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, text: &str) -> Option<(PathBuf, String, usize)> {
        Some((PathBuf::from(path), text.to_string(), 0))
    }

    /// ⚠️ a step back on the board does not restore the FILE, it restores the
    /// model and lets the board write itself out again. So the gate is not
    /// "does the json round-trip" — `canvas_json` is a compact comparison
    /// form and drops what `Canvas::save` puts back — but "does a board that
    /// travelled through a step come out as the same file it would have
    /// written anyway". Anything lost in there is data ⌘Z would delete in
    /// silence, and the migration's hard constraint is that the vault format
    /// does not change (TRD §5). One real board, one of everything.
    #[test]
    fn a_step_back_writes_the_same_file_the_board_would_have() {
        let dir = std::env::temp_dir().join(format!("synapso-step-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let source = include_str!("fixtures/one-of-everything.canvas");
        let canvas: vault::Canvas = serde_json::from_str(source).expect("the fixture parses");

        // what the board writes with no travelling at all
        let straight = dir.join("straight.canvas");
        canvas.save(&straight, vault::stamp(&straight)).unwrap();

        // and what it writes after going through a step and back
        let travelled = parse_canvas(&canvas_json(&canvas)).expect("a step parses back");
        let round = dir.join("round.canvas");
        travelled.save(&round, vault::stamp(&round)).unwrap();

        assert_eq!(
            std::fs::read_to_string(&straight).unwrap(),
            std::fs::read_to_string(&round).unwrap(),
            "a step back must hand the board the same file it had"
        );
        // and the step itself has to be stable, or repeated ⌘Z would drift
        let once = canvas_json(&canvas);
        assert_eq!(once, canvas_json(&parse_canvas(&once).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_burst_of_edits_is_one_step_back() {
        let mut history = History::default();
        history.mark(note("/a.md", "one"), None);
        // the first mark only starts watching — there is nothing behind it yet
        assert!(!history.can_undo());

        history.mark(note("/a.md", "one two"), None);
        history.mark(note("/a.md", "one two three"), None);
        assert!(history.can_undo());

        let back = history.undo(|_| {
            Some(Step::Note {
                doc: PathBuf::from("/a.md"),
                markdown: "one two three".into(),
                line: 0,
            })
        });
        assert!(matches!(back, Some(Step::Note { markdown, .. }) if markdown == "one two"));

        // and forward again
        let forward = history.redo(|_| {
            Some(Step::Note {
                doc: PathBuf::from("/a.md"),
                markdown: "one two".into(),
                line: 0,
            })
        });
        assert!(
            matches!(forward, Some(Step::Note { markdown, .. }) if markdown == "one two three")
        );
    }

    #[test]
    fn travelling_does_not_record_itself() {
        let mut history = History::default();
        history.mark(note("/a.md", "one"), None);
        history.mark(note("/a.md", "two"), None);
        history.undo(|_| None);
        // the mark now says "one", so the heartbeat after the undo is quiet
        history.mark(note("/a.md", "one"), None);
        assert!(!history.can_undo());
    }

    /// ⚠️ The regression this signature exists for. One stack holds both
    /// surfaces, so ⌘Z can move a document that is not the one on screen —
    /// and redo has to bring *that* document forward, not whatever was
    /// visible. Reading the visible surface put a note's state into the
    /// canvas's redo slot, and the note could never be recovered.
    #[test]
    fn undo_of_a_note_is_redone_even_when_the_canvas_is_the_open_surface() {
        let mut history = History::default();
        history.mark(note("/a.md", "the whole paragraph"), None);
        history.mark(note("/a.md", ""), None); // ⌘A, then delete

        // the canvas is what is on screen — `current` is still asked about the
        // note, because the note is what the popped step names
        let back = history.undo(|step| match step {
            Step::Note { doc, .. } => Some(Step::Note {
                doc: doc.clone(),
                markdown: String::new(),
                line: 0,
            }),
            Step::Canvas { .. } => unreachable!("the last edit was a note"),
        });
        assert!(
            matches!(back, Some(Step::Note { markdown, .. }) if markdown == "the whole paragraph")
        );

        let forward = history.redo(|step| match step {
            Step::Note { doc, .. } => Some(Step::Note {
                doc: doc.clone(),
                markdown: "the whole paragraph".into(),
                line: 0,
            }),
            Step::Canvas { .. } => unreachable!(),
        });
        assert!(
            matches!(forward, Some(Step::Note { ref markdown, .. }) if markdown.is_empty()),
            "the deletion has to be redoable: {forward:?}"
        );
    }

    #[test]
    fn a_step_for_a_document_that_is_gone_travels_without_a_redo_partner() {
        let mut history = History::default();
        history.mark(note("/a.md", "one"), None);
        history.mark(note("/a.md", "two"), None);
        // the note was deleted between the edit and the ⌘Z: nothing to pair
        assert!(history.undo(|_| None).is_some());
        assert!(history.redo(|_| None).is_none());
    }

    #[test]
    fn a_renamed_note_keeps_its_steps() {
        let mut history = History::default();
        history.mark(note("/x.md", "one"), None);
        history.mark(note("/x.md", "two"), None);
        history.rename(Path::new("/x.md"), Path::new("/y.md"));

        let back = history.undo(|_| None);
        assert!(
            matches!(back, Some(Step::Note { doc, .. }) if doc == Path::new("/y.md")),
            "the recorded step follows the file"
        );
    }

    #[test]
    fn a_rename_carries_the_pending_baseline_too() {
        let mut history = History::default();
        history.mark(note("/x.md", "one"), None);
        history.mark(note("/x.md", "two"), None);
        history.rename(Path::new("/x.md"), Path::new("/y.md"));

        // the mark under the new name is a continuation, not a path change —
        // an unmapped baseline would reset here and eat the "two" step
        history.mark(note("/y.md", "three"), None);
        let back = history.undo(|_| None);
        assert!(matches!(back, Some(Step::Note { markdown, .. }) if markdown == "two"));
    }

    #[test]
    fn switching_notes_does_not_look_like_an_edit() {
        let mut history = History::default();
        history.mark(note("/a.md", "one"), None);
        history.mark(note("/b.md", "other"), None);
        assert!(!history.can_undo());
        history.mark(note("/b.md", "other!"), None);
        assert!(history.can_undo());
    }
}
