//! The board's scene: what is on it, what is selected, and what a gesture
//! does to both.
//!
//! Phase 2, unit 3 of `docs/floem-migration-trd.md`. This is state and
//! semantics, not drawing — the shell hands it pointer events in world
//! coordinates and gets back an [`Outcome`] saying what to persist, open or
//! focus. Both shells drive the same one, so a drag means the same thing in
//! each.
//!
//! Two things stayed behind on purpose:
//!
//! - **the inline editor's buffer.** A `Scene` keeps the *text* being edited;
//!   the cursor, selection and undo of the widget showing it belong to the
//!   shell that draws it.
//! - **the toolbar.** The scene takes a [`Tool`], not the toolbar's icon: what
//!   a click does is the board's business, what it looks like is not.

use std::path::PathBuf;

use kurbo::{Point, Rect, Size, Vec2};

use super::{
    anchor_point, caption_size, corner, distance_to_path, edge_middle, grip_point, handle_at,
    label_anchor, normalized, resize, resize_locked, route_edge, spanning, EdgeLine, Viewport,
    CLICK_SLOP,
};
use crate::card::{card_columns, card_height, wrap};
use crate::theme::TEXT_BASE;
use crate::vault::{self, Shape};

/// How far apart two points are.
fn distance(a: Point, b: Point) -> f64 {
    (b - a).hypot()
}

/// What the pointer is being used *for* — the board's half of the toolbar.
/// The shell maps its own icons onto this; the scene never sees an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Pen,
    Rect,
    Ellipse,
    Triangle,
    Text,
    Frame,
    Arrow,
    Line,
}

/// What a pointer gesture is currently doing.
#[derive(Debug, Clone)]
enum Drag {
    Create {
        shape: Shape,
        origin: Point,
        cur: Point,
    },
    Move {
        /// Where the pointer was pressed, in world units. Every frame puts
        /// the nodes at where they began plus how far the pointer has come,
        /// so a snap that pulled them a few px never accumulates.
        start: Point,
        /// What is moving, and where each one began.
        ids: Vec<(String, Rect)>,
    },
    Rotate {
        id: String,
        /// World centre the node turns about — fixed for the whole gesture.
        centre: Point,
    },
    Resize {
        /// Text size when the gesture began — scaling from `node` would compound.
        size: f32,
        id: String,
        handle: (i8, i8),
        orig: Rect,
    },
    Connect {
        from: String,
        cur: Point,
        arrow: bool,
    },
    /// The selected edge's middle, dragged: the line bends through it.
    Bend {
        edge: String,
    },
    Pan {
        last: Point,
    },
    /// The pen: world points as they come in.
    Free {
        points: Vec<Point>,
    },
    /// Rubber band over empty canvas, in world units.
    Marquee {
        origin: Point,
        cur: Point,
    },
    /// Dragging one end of the selected edge: onto a face to pin it there,
    /// onto the middle of a node to let go, onto another node to rewire.
    Reconnect {
        edge: String,
        end: End,
        cur: Point,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    From,
    To,
}

/// What the select tool would take hold of at a point. The press asks it and
/// so does the pointer passing by, so what the cursor promises is what the
/// click does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Under {
    #[default]
    Nothing,
    /// An end of the selected edge — drag it onto another node, or loose.
    End(String, End),
    /// The middle of the selected edge — drag it to bend the line.
    Bend(String),
    /// The grip that turns the one selected node.
    Grip,
    /// A resize handle of the one selected node.
    Handle((i8, i8)),
    Node(String),
    Edge(String),
}

/// What is selected right now. Nodes and edges share one list: every "is
/// anything selected?" is one question, and the marquee fills it in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Node(String),
    Edge(String),
}

impl Selection {
    pub fn node(&self) -> Option<&str> {
        match self {
            Self::Node(id) => Some(id),
            Self::Edge(_) => None,
        }
    }
}

/// What a tool's next node or edge will wear: a node or an edge nobody can
/// see, which the style bar edits while that tool is in hand and nothing is
/// selected, and whose style a new one is born with.
#[derive(Debug, Clone)]
pub enum Template {
    Node(vault::Node),
    Edge(vault::Edge),
}

/// The id a template wears while the style bar works on it. Never in a file:
/// it is in the canvas only for the length of one verb (`Scene::with_next`).
const TEMPLATE_ID: &str = "\u{0}next";

/// The narrowest a label written from nothing gets: room for the caret.
const MIN_LABEL_WIDTH: f32 = 12.0;

/// The tool that draws a shape: the text tool's is a label.
fn shape_tool(shape: Shape) -> Option<Tool> {
    Some(match shape {
        Shape::Rect => Tool::Rect,
        Shape::Ellipse => Tool::Ellipse,
        Shape::Triangle => Tool::Triangle,
        Shape::Label => Tool::Text,
        _ => return None,
    })
}

/// Which groups of the style bar the current selection understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Styleable {
    /// Something is selected: colour and thickness.
    pub any: bool,
    /// Every one is a text node: shape and alignment.
    pub text: bool,
    /// Every one is bare text — no box to reshape, no border to thicken.
    pub label: bool,
    /// Every one is an edge: route, dash and the two arrowheads.
    pub edges: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Pressed(Point, u8),
    /// Middle button, or left button with space held — pan whatever the tool is.
    PanPressed(Point),
    /// Right button: open the context menu on whatever is under the cursor.
    Menu(Point),
    /// The pointer, and whether ⇧ is down — it snaps rotation to 15° and
    /// locks a resize to the node's aspect ratio.
    Dragged(Point, bool),
    Released(Point),
    /// The pointer left the pane: nothing is under it any more.
    Left,
    Scrolled {
        delta: Vec2,
        zoom: bool,
        at: Point,
    },
    /// A trackpad pinch: the magnification since the last one, positive as
    /// the fingers spread, about the point between them.
    Pinched {
        delta: f64,
        at: Point,
    },
    DeleteSelected,
    EditSelected,
    /// Fit every node in the viewport (`shift 1`).
    ZoomFit,
    SetZoom(f64),
}

/// What the app must do after a scene event.
pub enum Outcome {
    None,
    /// Nothing on screen moved — the pointer passed over the same thing it
    /// was already over. The one outcome that does not earn a repaint.
    Quiet,
    /// Persist the canvas.
    Changed,
    /// Persist, then move focus into the inline node editor.
    EditNode,
    /// A shape/connector was drawn: persist, then fall back to the select tool
    /// so the next click selects instead of drawing another one.
    Created {
        edit: bool,
    },
    /// Open what a card points at — the file, and the `#Heading` / `#^id`
    /// inside it when the card names one.
    OpenDoc(String, Option<String>),
    Menu {
        at: Point,
        target: Option<Selection>,
    },
}

pub struct Scene {
    pub path: PathBuf,
    pub canvas: vault::Canvas,
    pub selected: Vec<Selection>,
    pub view: Viewport,
    /// What the inline editor is on: a node's caption, or an edge's label.
    pub editing: Option<Selection>,
    /// The inline editor's buffer. A `text_editor`, not a field: a canvas text
    /// node holds paragraphs, so Enter has to be a line break.
    /// What the inline editor is showing. The buffer itself — cursor,
    /// undo, selection — belongs to whichever shell is drawing it; this is
    /// the text that buffer was opened on and the text a commit reads back.
    pub edit_text: String,
    /// What `begin_edit` PUT in `edit_text`. ⚠️ not the label: the buffer is
    /// written back trimmed, so an untouched editor over a caption with a
    /// space on the end wrote a different caption to the file — with nobody
    /// having typed. Comparing against what the editor opened with is the only
    /// way to know whether anything was typed at all.
    edit_from: String,
    /// The node's box when its editor opened: typing grows a label
    /// (`fit_label`), and Esc puts it back with the text.
    edit_box: Option<Rect>,
    drag: Option<Drag>,
    /// What the pointer is over while no gesture is running — the hover
    /// outline and the cursor read it. ⚠️ `Nothing` for the whole of a drag:
    /// a rubber band crossing a node is not the pointer resting on it.
    pub under: Under,
    /// Pane size, stamped by whichever shell draws it so `reveal` can centre
    /// on the real viewport.
    pub size: std::cell::Cell<Size>,
    /// Why the file on disk could not be read. A board that failed to load is
    /// blank on screen, and blank is exactly what must never be saved over it.
    pub broken: Option<String>,
    /// The file as we last saw it, so a save can refuse to clobber someone else.
    stamp: vault::Stamp,
    /// Where this board's refused save was written, once it has been — the
    /// canvas twin of `Doc::rescued`: unique once, stable for the session, so
    /// repeated refusals update one file instead of starting a trail.
    pub rescued: Option<PathBuf>,
    /// Ctrl is held: a move snaps to the edges and centres of the nodes
    /// around it (`snap_move`). The shell sets it as it hands over a drag.
    pub align: bool,
    /// The alignments the move snapped to, world units: what to draw as
    /// guides while it lasts.
    guides: Vec<(Point, Point)>,
    /// The pen's thickness in px — what the next stroke is drawn at. Not the
    /// canvas's: a pen setting is the hand's, and nothing on disk changes when
    /// it does.
    pub pen: f32,
    /// The pen's colour, as a node's `color` spells it, and `None` for the
    /// theme's own. The hand's too, like `pen`: only the strokes carry it.
    pub pen_color: Option<String>,
    /// What each tool's next node or edge wears, once the style bar has been
    /// used with that tool in hand. The hand's, like `pen`: never saved.
    next: std::collections::HashMap<Tool, Template>,
    /// The board as it was last agreed with disk. `Doc` keeps a `dirty` flag
    /// because a note has one funnel that changes it (`Doc::touched`); a board
    /// is changed from twenty places, so this asks the question instead of
    /// trusting twenty callers to have answered it.
    saved: String,
    /// Quote cards whose text is no longer in the note they took it from.
    ///
    /// ⚠️ Never persisted, and never a field on the node. "This quote went
    /// stale" is not data — it is an observation *about* two pieces of data,
    /// and writing it down would give it a chance to be wrong on disk.
    stale: std::collections::HashSet<String>,
    /// Sum of the revisions of every note when `stale` was last worked out.
    /// Notes only bump theirs on a real change, so an idle app costs one
    /// addition per heartbeat and nothing else.
    ///
    /// ⚠️ `None`, not `0`. A board opened with stale quotes already on it has
    /// seen no edits, so the sum is zero — and against a zeroed counter the
    /// check would decide nothing had changed and never run at all.
    checked: Option<u64>,
}

impl Scene {
    pub fn load(path: PathBuf) -> Self {
        // ⚠️ the stamp BEFORE the read: an external write landing in between
        // then makes the stamp stale, which errs toward a refusal (and its
        // rescue) — stamped after, the same race silently clobbers that write,
        // the one thing `write_checked` exists to prevent
        let stamp = vault::stamp(&path);
        let (canvas, broken) = match vault::Canvas::load(&path) {
            Ok(canvas) => (canvas, None),
            Err(e) => (vault::Canvas::default(), Some(e.to_string())),
        };
        let saved = crate::history::canvas_json(&canvas);
        Self {
            canvas,
            saved,
            broken,
            stamp,
            path,
            selected: Vec::new(),
            view: Viewport::default(),
            editing: None,
            edit_text: String::new(),
            edit_from: String::new(),
            edit_box: None,
            drag: None,
            under: Under::Nothing,
            size: std::cell::Cell::new(Size::new(840.0, 640.0)),
            stale: std::collections::HashSet::new(),
            checked: None,
            rescued: None,
            pen: vault::STROKES[0],
            pen_color: None,
            next: std::collections::HashMap::new(),
            align: false,
            guides: Vec::new(),
        }
    }

    /// Whether the board on screen differs from the file it came from.
    ///
    /// ⚠️ `&mut`, and deliberately: the inline editor's buffer is not part of
    /// the canvas until `flush_edit` puts it there, so asking without flushing
    /// would answer "clean" for a caption somebody is halfway through typing.
    pub fn dirty(&mut self) -> bool {
        // a board that did not load is blank on screen, and blank is exactly
        // what must never count as an edit worth keeping
        if self.broken.is_some() {
            return false;
        }
        self.flush_edit();
        crate::history::canvas_json(&self.canvas) != self.saved
    }

    /// Which quote cards no longer say what their note says.
    ///
    /// A quote card is a **photo**: the slice of the note as it was when you
    /// sent it to the board (ADR-003). A photo you cannot tell is wrong is
    /// worse than one that is right, so the card says when the words it holds
    /// have left the note.
    ///
    /// ⚠️ Called from the autosave heartbeat, never from `draw`. Drawing
    /// happens sixty times a second, and this board has already been down that
    /// road once — searching a note per card per frame is the same mistake in
    /// new clothes. `revisions` only moves when a note really changed, so an
    /// idle app pays one comparison.
    ///
    /// one substring search per quote card whenever any note changes.
    /// With a wall of quote cards over a large note, while typing, that is the
    /// ceiling — index by note, or check only what is on screen, if it ever
    /// grates.
    ///
    /// Returns whether the verdict CHANGED, which is what the caller repaints
    /// on. ⚠️ the set, not its size: one card going stale while another comes
    /// back in the same tick nets to zero, and the board would keep drawing
    /// both of them wrong.
    pub fn recheck_quotes(
        &mut self,
        revisions: u64,
        mut note_text: impl FnMut(&str) -> Option<String>,
    ) -> bool {
        if self.checked == Some(revisions) {
            return false;
        }
        self.checked = Some(revisions);
        let before = std::mem::take(&mut self.stale);
        for node in &self.canvas.nodes {
            let (Some(quote), Some(file)) = (node.excerpt.as_deref(), node.file.as_deref()) else {
                continue;
            };
            // a note this build cannot find is a different complaint, already
            // made by the card itself — not a stale quote
            if let Some(text) = note_text(file) {
                if !text.contains(quote.trim()) {
                    self.stale.insert(node.id.clone());
                }
            }
        }
        self.stale != before
    }

    /// Write what is sitting in the inline editor into the model, **without
    /// closing it**.
    ///
    /// ⚠️ The editor is a third state: widget buffer → `Canvas` → disk. Until
    /// this existed only an explicit commit (Enter, Esc, clicking away) moved
    /// the first into the second, so anything that persisted in between wrote
    /// the old value — and said it had saved.
    ///
    /// Not `commit_edit`: that one closes the editor, and a save can happen
    /// while you are still typing (autosave, a drag elsewhere, losing focus to
    /// another window). The field must survive its own flush.
    ///
    /// A `[[mention]]` is left as literal text here and only becomes a doc card
    /// on a real commit. Nothing is lost either way, and turning a node into a
    /// card underneath a save nobody asked for would be worse than the bug.
    pub fn flush_edit(&mut self) {
        // ⚠️ the whole point: opening the editor is not an edit. `EditSelected`
        // falls through to `settle` → `save` → here, so double-clicking a node
        // wrote the canvas — and a caption with whitespace on it (anything
        // Obsidian or another editor made) came back trimmed, changed on disk,
        // with nobody having pressed a key.
        if !self.has_pending_edit() {
            return;
        }
        let Some(what) = self.editing.clone() else {
            return;
        };
        let value = self.edit_text.clone().trim().to_string();
        match what {
            Selection::Node(id) => {
                if let Some(node) = self.canvas.node_mut(&id) {
                    node.set_edit_label(value);
                }
            }
            Selection::Edge(id) => {
                if let Some(edge) = self.canvas.edge_mut(&id) {
                    // an empty label is no label: the key leaves the file
                    edge.label = (!value.is_empty()).then_some(value);
                }
            }
        }
    }

    pub fn quote_is_stale(&self, id: &str) -> bool {
        self.stale.contains(id)
    }

    /// Throw away the verdict so the next check redoes it — for when the
    /// *cards* changed rather than the notes, which no note revision reports.
    pub fn forget_quote_check(&mut self) {
        self.checked = None;
    }

    /// Whether the inline editor is holding text the model has not seen.
    ///
    /// ⚠️ NOT "the editor is open". The heartbeat serialises the whole canvas
    /// when this is true, so an editor sitting open with nothing typed into it
    /// was a whole-board write every 600 ms, for as long as it stayed open.
    ///
    /// The comparison is against what `begin_edit` PUT in the buffer — for a
    /// node that is `edit_label`, `name#heading` and all, not the caption.
    /// ⚠️ Comparing against the caption calls a section card pending the
    /// moment it opens, and the flush that follows writes the `#heading` into
    /// the file name. `set_edit_label` is the other half of this pair.
    pub fn has_pending_edit(&self) -> bool {
        let Some(what) = self.editing.as_ref() else {
            return false;
        };
        // ⚠️ FIRST: nothing was typed, so there is nothing to write however
        // the model and the trimmed buffer compare. Without this an editor
        // opened over a caption with a trailing space answered "pending" for
        // as long as it stayed open, and the heartbeat serialised the whole
        // canvas every 600 ms to write a trim nobody asked for.
        if self.edit_text == self.edit_from {
            return false;
        }
        let text = self.edit_text.clone();
        let value = text.trim();
        match what {
            Selection::Node(id) => self
                .canvas
                .node(id)
                .is_some_and(|node| node.edit_label() != value),
            Selection::Edge(id) => self
                .canvas
                .edge(id)
                .is_some_and(|edge| edge.label.as_deref().unwrap_or_default() != value),
        }
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        // ⚠️ before the `broken` check and before anything else: this is the
        // one place every canvas write passes through, which is why the flush
        // lives here instead of in the twenty callers of `save_scene`
        self.flush_edit();
        if let Some(why) = &self.broken {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} did not load ({why}) — not saving over it", self.name()),
            ));
        }
        match self.canvas.save(&self.path, self.stamp) {
            Ok(stamp) => {
                self.stamp = stamp;
                // what disk now holds — the one place that may say the board is clean
                self.saved = crate::history::canvas_json(&self.canvas);
                Ok(())
            }
            // the board on screen is the only copy of the edit: beside the
            // file it lost the race for, as the note does (`-unsaved.md`)
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let rescue = self.rescued.clone().unwrap_or_else(|| {
                    let dir = self.path.parent().unwrap_or(std::path::Path::new("."));
                    vault::unique_path(dir, &format!("{}-unsaved", self.name()), "canvas")
                });
                // the rescue is ours alone, so its own stamp is the one to expect
                let why = match self.canvas.save(&rescue, vault::stamp(&rescue)) {
                    Ok(_) => {
                        self.rescued = Some(rescue.clone());
                        format!("changed on disk — copy kept in {}", vault::name(&rescue))
                    }
                    Err(e2) => format!("changed on disk, and the rescue failed too ({e2})"),
                };
                Err(std::io::Error::new(
                    e.kind(),
                    format!("{} {why}", self.name()),
                ))
            }
            Err(e) => Err(e),
        }
    }

    pub fn name(&self) -> String {
        vault::name(&self.path)
    }

    /// The one node selected, when exactly one thing is and it is a node.
    /// Handles, the rotate grip and the inline editor all mean *this* node,
    /// and none of them has an answer for two.
    pub fn only_node(&self) -> Option<&str> {
        match self.selected.as_slice() {
            [Selection::Node(id)] => Some(id),
            _ => None,
        }
    }

    fn only_selected(&self) -> Option<Selection> {
        match self.selected.as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        }
    }

    fn only_edge(&self) -> Option<&str> {
        match self.selected.as_slice() {
            [Selection::Edge(id)] => Some(id),
            _ => None,
        }
    }

    /// ADR-004 §D4 — what the style bar may offer for what is selected right
    /// now. A control shows only when it applies to **everything** selected:
    /// a shape and an arrow together leave colour and thickness, and nothing
    /// that would either lie (do nothing) or surprise (do something to half
    /// the selection).
    pub fn styleable(&self) -> Styleable {
        let text_node = |what: &Selection| {
            what.node()
                .and_then(|id| self.canvas.node(id))
                // a card, a frame and a drawing have no shape of their own to
                // change and no caption to move around inside a box
                .is_some_and(|n| n.kind == "text" && n.draw.is_none())
        };
        let bare = |what: &Selection| {
            what.node()
                .and_then(|id| self.canvas.node(id))
                .is_some_and(|n| n.shape() == Shape::Label)
        };
        Styleable {
            any: !self.selected.is_empty(),
            text: !self.selected.is_empty() && self.selected.iter().all(text_node),
            label: !self.selected.is_empty() && self.selected.iter().all(bare),
            edges: !self.selected.is_empty()
                && self
                    .selected
                    .iter()
                    .all(|w| matches!(w, Selection::Edge(_))),
        }
    }

    /// The template a tool's next node or edge is born from: what the style
    /// bar set with that tool in hand, or a plain one. `None` for a tool that
    /// draws nothing styleable this way (select, pen, frame).
    pub fn next(&self, tool: Tool) -> Option<Template> {
        if let Some(t) = self.next.get(&tool) {
            return Some(t.clone());
        }
        let rect = Rect::new(0.0, 0.0, 1.0, 1.0);
        match tool {
            Tool::Arrow | Tool::Line => Some(Template::Edge(vault::Edge {
                id: TEMPLATE_ID.to_string(),
                from_node: TEMPLATE_ID.to_string(),
                to_node: TEMPLATE_ID.to_string(),
                to_end: (tool == Tool::Line).then(|| "none".to_string()),
                ..vault::Edge::default()
            })),
            Tool::Frame | Tool::Pen | Tool::Select => None,
            _ => tool_shape(tool).map(|shape| {
                Template::Node(vault::Node::new(TEMPLATE_ID.to_string(), shape, rect, ""))
            }),
        }
    }

    /// What the style bar may offer for a tool's next node or edge: the
    /// selection's rules, applied to the template.
    pub fn next_styleable(&self, tool: Tool) -> Styleable {
        match self.next(tool) {
            Some(Template::Node(n)) => Styleable {
                any: true,
                text: n.kind == "text" && n.draw.is_none(),
                label: n.shape() == Shape::Label,
                edges: false,
            },
            Some(Template::Edge(_)) => Styleable {
                any: true,
                edges: true,
                ..Styleable::default()
            },
            None => Styleable::default(),
        }
    }

    /// Run a style verb on a tool's template instead of the selection: the
    /// template stands in the canvas, alone and selected, for the length of
    /// `f`, so every verb of the bar works on it unchanged. The canvas and
    /// the selection come back exactly as they were.
    pub fn with_next<R>(&mut self, tool: Tool, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        let template = self.next(tool)?;
        let selected = std::mem::take(&mut self.selected);
        match template {
            Template::Node(n) => {
                self.canvas.nodes.push(n);
                self.selected.push(Selection::Node(TEMPLATE_ID.to_string()));
            }
            Template::Edge(e) => {
                self.canvas.edges.push(e);
                self.selected.push(Selection::Edge(TEMPLATE_ID.to_string()));
            }
        }
        let out = f(self);
        let node = self.canvas.nodes.iter().position(|n| n.id == TEMPLATE_ID);
        let edge = self.canvas.edges.iter().position(|e| e.id == TEMPLATE_ID);
        let template = match (node, edge) {
            (Some(i), _) => Template::Node(self.canvas.nodes.remove(i)),
            (None, Some(i)) => Template::Edge(self.canvas.edges.remove(i)),
            (None, None) => unreachable!("a style verb never removes what it styles"),
        };
        self.next.insert(tool, template);
        self.selected = selected;
        Some(out)
    }

    /// A new node takes what its tool's template wears: colour, border,
    /// text size, weight and alignment — never its shape or its size.
    fn dress_node(&self, node: &mut vault::Node) {
        let Some(Template::Node(t)) =
            shape_tool(node.shape()).and_then(|tool| self.next.get(&tool))
        else {
            return;
        };
        node.color = t.color.clone();
        node.stroke_width = t.stroke_width;
        node.font_size = t.font_size;
        node.font_weight = t.font_weight.clone();
        node.align = t.align.clone();
        node.font_family = t.font_family.clone();
    }

    /// A new edge takes what its tool's template wears: colour, width,
    /// route, dash and both heads.
    fn dress_edge(&self, edge: &mut vault::Edge, arrow: bool) {
        let tool = if arrow { Tool::Arrow } else { Tool::Line };
        let Some(Template::Edge(t)) = self.next.get(&tool) else {
            return;
        };
        edge.color = t.color.clone();
        edge.stroke_width = t.stroke_width;
        edge.route = t.route.clone();
        edge.dash = t.dash;
        edge.to_end = t.to_end.clone();
        edge.from_end = t.from_end.clone();
        edge.font_family = t.font_family.clone();
    }

    /// The text being typed into a label changed its size: the shell measured
    /// it (`size`, world units). The box fits the lines' height and holds the
    /// widest line, so the words never run out of it. Text being written from
    /// nothing (the text tool's click) is as wide as its words, as in
    /// Excalidraw; text that was already there only grows, so a label made
    /// wide stays wide.
    pub fn fit_label(&mut self, id: &str, size: Size) {
        let fresh = self.edit_from.is_empty();
        let Some(node) = self
            .canvas
            .node_mut(id)
            .filter(|n| n.shape() == Shape::Label)
        else {
            return;
        };
        node.width = if fresh {
            (size.width as f32).max(MIN_LABEL_WIDTH)
        } else {
            node.width.max(size.width as f32)
        };
        node.height = size.height as f32;
    }

    /// A label left with no words is no label: the text tool's click that was
    /// never typed into leaves nothing behind. Only bare text — an empty box
    /// is still a box.
    fn drop_if_blank(&mut self, id: &str) {
        let blank = self
            .canvas
            .node(id)
            .is_some_and(|n| n.shape() == Shape::Label && n.text.trim().is_empty());
        // ⚠️ as a delete does: the edges that ended on it go too, or an arrow
        // to a cleared label is left pointing at an id that isn't there
        if blank {
            self.canvas.remove_node(id);
            self.selected.retain(|s| s.node() != Some(id));
        }
    }

    fn select_one(&mut self, what: Selection) {
        self.selected.clear();
        self.selected.push(what);
    }

    /// ⚠️ by id, not by building a `Selection` — the draw loop asks this once
    /// per edge per frame, and a `String` per ask is an allocation per frame.
    pub fn holds_node(&self, id: &str) -> bool {
        self.selected
            .iter()
            .any(|s| matches!(s, Selection::Node(n) if n == id))
    }

    pub fn holds_edge(&self, id: &str) -> bool {
        self.selected
            .iter()
            .any(|s| matches!(s, Selection::Edge(e) if e == id))
    }

    /// The edge under `world`, within a tolerance that is constant on screen —
    /// a 6px reach at any zoom, because that is what the pointer is.
    fn hit_edge(&self, world: Point) -> Option<String> {
        let reach = 6.0 / self.view.zoom;
        self.canvas
            .edges
            .iter()
            .rev()
            .find(|edge| {
                self.edge_points(edge)
                    .is_some_and(|line| distance_to_path(&line, world) <= reach)
            })
            .map(|edge| edge.id.clone())
    }

    /// The edge as a polyline in world coordinates — the same geometry the
    /// draw uses, so what you can click is what you can see.
    ///
    /// ⚠️ [`reach`] is capped in *screen* pixels, so the distance goes in
    /// scaled by the zoom and the answer comes back out of it.
    fn edge_line_of(&self, edge: &vault::Edge) -> Option<EdgeLine> {
        let from = self.canvas.node(&edge.from_node)?;
        let to = self.canvas.node(&edge.to_node)?;
        Some(route_edge(edge, from, to))
    }

    fn edge_points(&self, edge: &vault::Edge) -> Option<Vec<Point>> {
        Some(self.edge_line_of(edge)?.points)
    }

    /// Where the selected edge's bend handle is, on screen: the middle of
    /// its line (`edge_middle`).
    pub fn bend_handle(&self) -> Option<Point> {
        let edge = self.canvas.edge(self.only_edge()?)?;
        // an elbow that turns once (one side across, one along) has no leg
        // in its middle to move: no handle, rather than one that does nothing
        if edge.route() == vault::Route::Elbow {
            let from = self.canvas.node(&edge.from_node)?;
            let to = self.canvas.node(&edge.to_node)?;
            let (_, a, _, b) = vault::edge_ends(edge, from, to);
            if a.horizontal() != b.horizontal() {
                return None;
            }
        }
        let line = self.edge_line_of(edge)?;
        Some(self.view.to_screen(edge_middle(&line, edge)?))
    }

    /// A move of `ids` by `by`, pulled onto the nearest alignment with the
    /// nodes that stay put — tldraw's snapping, held on Ctrl. Each axis on
    /// its own: the moving box's left, centre or right onto another node's
    /// left, centre or right, within 8px on screen; top, middle or bottom
    /// likewise. Returns the move and the guides to draw: one line per
    /// alignment, through every node that shares it.
    fn snap_move(&self, ids: &[(String, Rect)], by: Vec2) -> (Vec2, Vec<(Point, Point)>) {
        let Some(moving) = ids.iter().map(|(_, r)| *r).reduce(|a, b| a.union(b)) else {
            return (by, Vec::new());
        };
        let box_now = moving + by;
        let others: Vec<Rect> = self
            .canvas
            .nodes
            .iter()
            .filter(|n| n.shape() != Shape::Point && !ids.iter().any(|(id, _)| *id == n.id))
            .map(|n| n.rect())
            .collect();
        let reach = 8.0 / self.view.zoom;
        let xs = |r: Rect| [r.x0, r.center().x, r.x1];
        let ys = |r: Rect| [r.y0, r.center().y, r.y1];
        // the smallest pull on one axis that lines some line of the box up
        // with some line of another node
        let pull = |mine: [f64; 3], theirs: &dyn Fn(Rect) -> [f64; 3]| {
            let mut best: Option<f64> = None;
            for r in &others {
                for t in theirs(*r) {
                    for m in mine {
                        let d = t - m;
                        if d.abs() <= reach && best.is_none_or(|b| d.abs() < b.abs()) {
                            best = Some(d);
                        }
                    }
                }
            }
            best
        };
        let dx = pull(xs(box_now), &xs);
        let dy = pull(ys(box_now), &ys);
        let by = by + Vec2::new(dx.unwrap_or(0.0), dy.unwrap_or(0.0));
        let snapped = moving + by;

        let mut guides = Vec::new();
        let near = |a: f64, b: f64| (a - b).abs() < 0.5;
        if dx.is_some() {
            for x in xs(snapped) {
                let lined: Vec<&Rect> = others
                    .iter()
                    .filter(|r| xs(**r).iter().any(|t| near(*t, x)))
                    .collect();
                if !lined.is_empty() {
                    let top = lined.iter().map(|r| r.y0).fold(snapped.y0, f64::min);
                    let bottom = lined.iter().map(|r| r.y1).fold(snapped.y1, f64::max);
                    guides.push((Point::new(x, top), Point::new(x, bottom)));
                }
            }
        }
        if dy.is_some() {
            for y in ys(snapped) {
                let lined: Vec<&Rect> = others
                    .iter()
                    .filter(|r| ys(**r).iter().any(|t| near(*t, y)))
                    .collect();
                if !lined.is_empty() {
                    let left = lined.iter().map(|r| r.x0).fold(snapped.x0, f64::min);
                    let right = lined.iter().map(|r| r.x1).fold(snapped.x1, f64::max);
                    guides.push((Point::new(left, y), Point::new(right, y)));
                }
            }
        }
        (by, guides)
    }

    /// The alignment guides of the move running now, world units.
    pub fn guides(&self) -> &[(Point, Point)] {
        &self.guides
    }

    /// A pan is running (space or the middle button held): the shell keeps
    /// the pointer's trail to let the board glide on as it's let go.
    pub fn is_panning(&self) -> bool {
        matches!(self.drag, Some(Drag::Pan { .. }))
    }

    /// Point nodes no edge ends at any more: a loose end whose edge went, or
    /// was reconnected elsewhere, leaves nothing behind.
    fn drop_loose_points(&mut self) {
        let used: std::collections::HashSet<String> = self
            .canvas
            .edges
            .iter()
            .flat_map(|e| [e.from_node.clone(), e.to_node.clone()])
            .collect();
        let gone: Vec<String> = self
            .canvas
            .nodes
            .iter()
            .filter(|n| n.shape() == Shape::Point && !used.contains(&n.id))
            .map(|n| n.id.clone())
            .collect();
        for id in gone {
            self.canvas.nodes.retain(|n| n.id != id);
            self.selected.retain(|s| s.node() != Some(id.as_str()));
        }
    }

    /// A loose end at `at`: a new point node, returned by id.
    fn new_point(&mut self, at: Point) -> String {
        let id = self.canvas.next_id();
        self.canvas.nodes.push(vault::Node::point(id.clone(), at));
        id
    }

    /// Centre the viewport on a node and select it — used by `[[canvas: …]]`.
    /// ⚠️ Zooms to the node, not just centres on it. Landing on a match at
    /// whatever zoom happened to be set is how ⌘F took you to a card you still
    /// could not read — or filled the pane with one corner of it.
    ///
    /// The zoom is clamped and never magnifies past 1:1: a small node in the
    /// middle of a legible board should not throw the board away to fill the
    /// pane with itself.
    pub fn reveal(&mut self, id: &str) {
        if let Some(node) = self.canvas.node(id) {
            let (r, pane) = (node.rect(), self.size.get());
            // a wide margin on purpose: the node is the answer, but the board
            // around it is the context that makes the answer mean something
            const MARGIN: f64 = 260.0;
            self.view.zoom = ((pane.width - MARGIN) / r.width().max(1.0))
                .min((pane.height - MARGIN) / r.height().max(1.0))
                // the same ceiling `zoom_to_fit` uses — one node is not a
                // reason to magnify the board past twice life size
                .clamp(0.25, 2.0);
            self.view.offset = Vec2::new(
                pane.width / 2.0 - r.center().x * self.view.zoom,
                pane.height / 2.0 - r.center().y * self.view.zoom,
            );
            self.select_one(Selection::Node(id.to_string()));
        }
    }

    /// Fit every node in view, with a margin. No nodes: back to 100%.
    pub fn zoom_to_fit(&mut self) {
        let pane = self.size.get();
        let Some(mut bounds) = self.canvas.nodes.first().map(|n| n.rect()) else {
            self.view = Viewport::default();
            return;
        };
        for node in &self.canvas.nodes {
            bounds = bounds.union(node.rect());
        }
        let bounds = bounds;
        let margin = 60.0;
        let zoom = ((pane.width - margin) / bounds.width().max(1.0))
            .min((pane.height - margin) / bounds.height().max(1.0))
            .clamp(0.25, 2.0);
        self.view.zoom = zoom;
        self.view.offset = Vec2::new(
            pane.width / 2.0 - bounds.center().x * zoom,
            pane.height / 2.0 - bounds.center().y * zoom,
        );
    }

    pub fn handle(&mut self, event: Event, tool: Tool) -> Outcome {
        match event {
            Event::Pressed(screen, clicks) => self.press(self.view.to_world(screen), clicks, tool),
            Event::PanPressed(screen) => {
                self.drag = Some(Drag::Pan { last: screen });
                Outcome::None
            }
            Event::Menu(screen) => {
                let world = self.view.to_world(screen);
                let target = self
                    .canvas
                    .hit(world)
                    .map(|n| Selection::Node(n.id.clone()))
                    .or_else(|| self.hit_edge(world).map(Selection::Edge));
                if let Some(target) = &target {
                    // right-clicking something already in a group selection
                    // keeps the group — the menu is about all of it
                    if !self.selected.contains(target) {
                        self.select_one(target.clone());
                    }
                }
                Outcome::Menu { at: screen, target }
            }
            Event::Dragged(screen, snap) if self.drag.is_some() => {
                self.drag_to(self.view.to_world(screen), screen, snap)
            }
            // no gesture: the pointer is just passing over the board
            Event::Dragged(screen, _) => {
                // only the select tool grabs things; the others draw where
                // they are pressed, so the outline would promise nothing
                let under = match (tool, &self.editing) {
                    (Tool::Select, None) => self.under(self.view.to_world(screen)),
                    _ => Under::Nothing,
                };
                if under == self.under {
                    return Outcome::Quiet;
                }
                self.under = under;
                Outcome::None
            }
            Event::Left => {
                self.under = Under::Nothing;
                Outcome::None
            }
            Event::Released(screen) => self.release(self.view.to_world(screen)),
            Event::Scrolled { delta, zoom, at } => {
                if zoom {
                    // exponential in pixels: one mouse notch (Freya hands it
                    // on as 53px) is ~20%, and a trackpad's many small deltas
                    // add up smoothly
                    self.view.zoom_at(at, (delta.y * 0.0035).exp());
                } else {
                    self.view.offset += delta;
                }
                Outcome::None
            }
            // exp, not `1 + delta`: the same for the small steps a pinch
            // comes in, and never zero or below on a big one
            Event::Pinched { delta, at } => {
                self.view.zoom_at(at, delta.exp());
                Outcome::None
            }
            Event::ZoomFit => {
                self.zoom_to_fit();
                Outcome::None
            }
            Event::SetZoom(zoom) => {
                let pane = self.size.get();
                self.view.zoom_at(
                    Point::new(pane.width / 2.0, pane.height / 2.0),
                    zoom / self.view.zoom,
                );
                Outcome::None
            }
            // ⚠️ nothing is taken while the editor is open: ⌫ there is a
            // keystroke, and clearing the selection on the way past would
            // leave the node it is editing unselected behind the field
            Event::DeleteSelected if self.editing.is_some() || self.selected.is_empty() => {
                Outcome::None
            }
            Event::DeleteSelected => {
                for what in std::mem::take(&mut self.selected) {
                    match what {
                        Selection::Node(id) => self.canvas.remove_node(&id),
                        Selection::Edge(id) => self.canvas.remove_edge(&id),
                    }
                }
                self.drop_loose_points();
                Outcome::Changed
            }
            // exactly one thing: the editor is a field over one caption, and
            // has no answer for two
            Event::EditSelected => match (self.only_selected(), &self.editing) {
                (Some(what), None) => {
                    self.begin_edit(what);
                    // begin_edit declines on cards that own no label
                    if self.editing.is_some() {
                        Outcome::EditNode
                    } else {
                        Outcome::None
                    }
                }
                _ => Outcome::None,
            },
        }
    }

    fn press(&mut self, world: Point, clicks: u8, tool: Tool) -> Outcome {
        if self.editing.is_some() {
            self.commit_edit();
        }
        if let Some(shape) = tool_shape(tool) {
            self.drag = Some(Drag::Create {
                shape,
                origin: world,
                cur: world,
            });
            return Outcome::None;
        }
        if tool == Tool::Pen {
            self.drag = Some(Drag::Free {
                points: vec![world],
            });
            return Outcome::None;
        }
        if let Some(arrow) = connector_arrow(tool) {
            if let Some(node) = self.canvas.hit(world) {
                self.drag = Some(Drag::Connect {
                    from: node.id.clone(),
                    cur: world,
                    arrow,
                });
            }
            return Outcome::None;
        }

        // select tool. ⚠️ a press starts a gesture, and the hover outline is
        // for a pointer at rest: whatever it was over, it is not any more
        self.under = Under::Nothing;
        match self.under(world) {
            Under::End(edge, end) => {
                self.drag = Some(Drag::Reconnect {
                    edge,
                    end,
                    cur: world,
                });
                Outcome::None
            }
            Under::Bend(edge) => {
                self.drag = Some(Drag::Bend { edge });
                Outcome::None
            }
            Under::Grip => {
                if let Some(node) = self.only_node().and_then(|id| self.canvas.node(id)) {
                    self.drag = Some(Drag::Rotate {
                        id: node.id.clone(),
                        centre: node.rect().center(),
                    });
                }
                Outcome::None
            }
            Under::Handle(handle) => {
                if let Some(node) = self.only_node().and_then(|id| self.canvas.node(id)) {
                    self.drag = Some(Drag::Resize {
                        size: node.text_size(caption_size(node.shape())),
                        id: node.id.clone(),
                        handle,
                        orig: node.rect(),
                    });
                }
                Outcome::None
            }
            Under::Node(id) => {
                let Some(node) = self.canvas.node(&id) else {
                    return Outcome::None;
                };
                let shape = node.shape();
                let file = node.file.clone();
                // the card's `#Heading` / `#^id` goes along: a card that opens
                // its note at the top is a link that forgot where it pointed
                let section = node.section().map(str::to_string);
                // grabbing something already in the group moves the group;
                // grabbing anything else starts a fresh selection
                if !self.holds_node(&id) {
                    self.select_one(Selection::Node(id.clone()));
                }
                if clicks >= 2 {
                    self.select_one(Selection::Node(id.clone()));
                    if shape == Shape::File {
                        return Outcome::OpenDoc(file.unwrap_or_default(), section);
                    }
                    self.begin_edit(Selection::Node(id.clone()));
                    return Outcome::EditNode;
                }
                let ids = self
                    .moving_nodes()
                    .into_iter()
                    .filter_map(|id| {
                        let r = self.canvas.node(&id)?.rect();
                        Some((id, r))
                    })
                    .collect();
                self.drag = Some(Drag::Move { start: world, ids });
                Outcome::None
            }
            // an edge has no position of its own — selecting it is the
            // whole gesture, and dragging from here would mean nothing
            Under::Edge(id) => {
                self.select_one(Selection::Edge(id.clone()));
                if clicks >= 2 {
                    self.begin_edit(Selection::Edge(id));
                    return Outcome::EditNode;
                }
                Outcome::None
            }
            // ⚠️ the empty-space drag is the marquee now, not the pan.
            // Pan keeps the two gestures it already had of its own —
            // space+drag and the middle button (`Event::PanPressed`) —
            // and this is the gesture Figma, Excalidraw and Miro spend it on
            Under::Nothing => {
                self.selected.clear();
                self.drag = Some(Drag::Marquee {
                    origin: world,
                    cur: world,
                });
                Outcome::None
            }
        }
    }

    /// What the select tool would take hold of at `world`, in the order the
    /// press tries them. ⚠️ the selected edge's end handles come first: they
    /// sit exactly on a node's border, and the node would swallow the press.
    /// Then the one selected node's grip and handles, which hang outside it.
    pub fn under(&self, world: Point) -> Under {
        let ends = self.only_edge().map(str::to_string).and_then(|id| {
            let line = self.canvas.edge(&id).and_then(|e| self.edge_points(e))?;
            Some((id, *line.first()?, *line.last()?))
        });
        if let Some((id, a, b)) = ends {
            let tolerance = 7.0 / self.view.zoom;
            if distance(a, world) <= tolerance {
                return Under::End(id, End::From);
            }
            if distance(b, world) <= tolerance {
                return Under::End(id, End::To);
            }
            let middle = self.bend_handle().map(|p| self.view.to_world(p));
            if middle.is_some_and(|m| distance(m, world) <= tolerance) {
                return Under::Bend(id);
            }
        }
        // a loose end has no handles: it is only ever moved
        let boxed = self
            .only_node()
            .and_then(|id| self.canvas.node(id))
            .filter(|n| n.shape() != Shape::Point);
        if let Some(node) = boxed {
            let tolerance = 6.0 / self.view.zoom;
            let grip = grip_point(node, self.view.zoom);
            let reach = tolerance + 2.0 / self.view.zoom;
            if (grip.x - world.x).abs() <= reach && (grip.y - world.y).abs() <= reach {
                return Under::Grip;
            }
            // handles live in the node's upright frame, so the pointer
            // has to be brought there first
            if let Some(handle) = handle_at(node.rect(), node.local(world), tolerance) {
                return Under::Handle(handle);
            }
        }
        if let Some(node) = self.canvas.hit(world) {
            return Under::Node(node.id.clone());
        }
        match self.hit_edge(world) {
            Some(id) => Under::Edge(id),
            None => Under::Nothing,
        }
    }

    /// The node the pointer is resting on, for the hover outline.
    pub fn hovered(&self) -> Option<&str> {
        match &self.under {
            Under::Node(id) => Some(id),
            _ => None,
        }
    }

    /// What a rubber band caught. A node comes along when the band touches it;
    /// an edge only when **both** its ends are inside — an edge has no position
    /// of its own, so half a link in the box means nothing to move.
    fn enclosed(&self, area: Rect) -> Vec<Selection> {
        let nodes = self
            .canvas
            .nodes
            .iter()
            .filter(|n| n.rect().overlaps(area))
            .map(|n| Selection::Node(n.id.clone()));
        let edges = self
            .canvas
            .edges
            .iter()
            .filter(|e| {
                self.edge_points(e).is_some_and(|line| {
                    line.first().is_some_and(|p| area.contains(*p))
                        && line.last().is_some_and(|p| area.contains(*p))
                })
            })
            .map(|e| Selection::Edge(e.id.clone()));
        nodes.chain(edges).collect()
    }

    /// Which nodes a move gesture carries: everything selected that is a node.
    fn moving_nodes(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter_map(|s| s.node().map(str::to_string))
            .collect()
    }

    /// Every selected edge — restyling applies to the whole selection, so a
    /// marquee over a diagram can make all of it dashed in one go.
    pub fn selected_edges(&self) -> Vec<String> {
        self.selected
            .iter()
            .filter_map(|s| match s {
                Selection::Edge(id) => Some(id.clone()),
                Selection::Node(_) => None,
            })
            .collect()
    }

    /// ⚠️ `&mut self.drag`, never `self.drag.clone()`. `Drag::Free` carries
    /// every point in the squiggle so far, and cloning the whole drag on every
    /// pointer move made one freehand stroke O(n²) in its own length — for
    /// five of these arms, which do nothing but rewrite `cur`. The node edits
    /// still reach `self.canvas`: they are a different field, so the borrows
    /// do not overlap.
    fn drag_to(&mut self, world: Point, screen: Point, snap: bool) -> Outcome {
        if let Some(Drag::Bend { edge }) = &self.drag {
            let id = edge.clone();
            self.bend(&id, world);
            return Outcome::None;
        }
        match &mut self.drag {
            Some(Drag::Create { cur, .. }) => {
                *cur = world;
                Outcome::None
            }
            Some(Drag::Move { start, ids }) => {
                let ids = ids.clone();
                let mut by = world - *start;
                self.guides.clear();
                if self.align {
                    let (snapped, guides) = self.snap_move(&ids, by);
                    by = snapped;
                    self.guides = guides;
                }
                for (id, from) in &ids {
                    if let Some(node) = self.canvas.node_mut(id) {
                        node.set_rect(from.with_origin((from.x0 + by.x, from.y0 + by.y)));
                    }
                }
                Outcome::None
            }
            Some(Drag::Rotate { id, centre }) => {
                let (id, centre) = (id.clone(), *centre);
                if let Some(node) = self.canvas.node_mut(&id) {
                    // the grip sits above the node, so straight up is zero
                    let degrees =
                        (world.y - centre.y).atan2(world.x - centre.x).to_degrees() + 90.0;
                    // design: free by default, ⇧ locks to 15°
                    node.set_angle(if snap {
                        ((degrees / 15.0).round() * 15.0) as f32
                    } else {
                        degrees as f32
                    });
                }
                Outcome::None
            }
            Some(Drag::Resize {
                size,
                id,
                handle,
                orig,
            }) => {
                let (size, id, handle, orig) = (*size, id.clone(), *handle, *orig);
                if let Some(node) = self.canvas.node_mut(&id) {
                    let angle = node.radians();
                    let local = vault::turn(world, orig.center(), -angle);
                    // ⇧: resize keeps the ratio — a stretch becomes a scale
                    let mut next = if snap {
                        resize_locked(orig, handle, local)
                    } else {
                        resize(orig, handle, local)
                    };
                    // resizing moves the centre, and the node turns about its
                    // centre — so the edge the drag was NOT holding would swing
                    // away. Put it back.
                    let anchor = anchor_point(orig, handle);
                    let held = vault::turn(anchor, orig.center(), angle);
                    let drifted = vault::turn(anchor, next.center(), angle);
                    next = next + (held - drifted);
                    // a corner scales the text with the box, so the thing keeps
                    // its proportions; a side only changes the box, which is
                    // what lets a paragraph re-wrap — unless ⇧ is holding the
                    // ratio, when the side resize is a scaling too
                    if (snap || (handle.0 != 0 && handle.1 != 0)) && orig.height() > 1.0 {
                        node.set_text_size(size * (next.height() / orig.height()) as f32);
                    }
                    node.set_rect(next);
                }
                Outcome::None
            }
            Some(Drag::Connect { cur, .. }) => {
                *cur = world;
                Outcome::None
            }
            Some(Drag::Pan { last }) => {
                let by = Vec2::new(screen.x - last.x, screen.y - last.y);
                *last = screen;
                self.view.offset += by;
                Outcome::None
            }
            Some(Drag::Free { points }) => {
                // one point every couple of world units, not every
                // pointer event — a slow hand would otherwise write thousands
                // of them into the file for one squiggle
                if points
                    .last()
                    .is_none_or(|last| distance(*last, world) >= 2.0)
                {
                    points.push(world);
                }
                Outcome::None
            }
            Some(Drag::Marquee { cur, .. }) => {
                *cur = world;
                Outcome::None
            }
            Some(Drag::Reconnect { cur, .. }) => {
                *cur = world;
                Outcome::None
            }
            // handled above, before the borrow
            Some(Drag::Bend { .. }) | None => Outcome::None,
        }
    }

    fn release(&mut self, world: Point) -> Outcome {
        self.guides.clear();
        match self.drag.take() {
            Some(Drag::Create { shape, origin, cur }) => {
                let rect = if (cur.x - origin.x).abs() < CLICK_SLOP
                    && (cur.y - origin.y).abs() < CLICK_SLOP
                {
                    let (w, h) = shape.default_size();
                    Rect::from_origin_size(origin, Size::new(f64::from(w), f64::from(h)))
                } else {
                    normalized(origin, cur)
                };
                let id = self.canvas.next_id();
                let mut node = vault::Node::new(id.clone(), shape, rect, "");
                self.dress_node(&mut node);
                // ⚠️ a frame is drawn around things, so it is born at the BACK.
                // Draw order is z-order here, and pushing it last put its fill
                // over everything it was drawn around — and iced paints a
                // frame's text above all fills, so the captions underneath
                // stayed visible on top of the box that covered them. The
                // result was a frame with other people's words floating in it.
                if shape == Shape::Frame {
                    self.canvas.nodes.insert(0, node);
                } else {
                    self.canvas.nodes.push(node);
                }
                self.select_one(Selection::Node(id.clone()));
                // a fresh shape wants its label typed straight away
                let edit = shape != Shape::File;
                if edit {
                    self.begin_edit(Selection::Node(id.clone()));
                }
                Outcome::Created { edit }
            }
            Some(Drag::Connect { from, arrow, .. }) => {
                match self.canvas.hit(world).map(|n| n.id.clone()) {
                    // ⚠️ no `fromSide`/`toSide`: written at creation they are
                    // never recomputed, so the arrow keeps leaving by the face
                    // the nodes happened to be on when it was drawn. Absent
                    // means "the side that faces the other one, now".
                    Some(to) if to != from && self.canvas.node(&from).is_some() => {
                        let id = self.canvas.next_id();
                        let mut edge = vault::Edge {
                            id: id.clone(),
                            from_node: from,
                            to_node: to,
                            to_end: (!arrow).then(|| "none".to_string()),
                            ..vault::Edge::default()
                        };
                        self.dress_edge(&mut edge, arrow);
                        self.canvas.edges.push(edge);
                        self.select_one(Selection::Edge(id));
                        Outcome::Created { edit: false }
                    }
                    // let go over nothing: the arrow ends there, loose —
                    // unless it never left the node it started on
                    None if self.canvas.node(&from).is_some_and(|n| {
                        !n.rect().inflate(CLICK_SLOP, CLICK_SLOP).contains(world)
                    }) =>
                    {
                        let to = self.new_point(world);
                        let id = self.canvas.next_id();
                        let mut edge = vault::Edge {
                            id: id.clone(),
                            from_node: from,
                            to_node: to,
                            to_end: (!arrow).then(|| "none".to_string()),
                            ..vault::Edge::default()
                        };
                        self.dress_edge(&mut edge, arrow);
                        self.canvas.edges.push(edge);
                        self.select_one(Selection::Edge(id));
                        Outcome::Created { edit: false }
                    }
                    _ => Outcome::None,
                }
            }
            Some(Drag::Move { .. } | Drag::Resize { .. } | Drag::Rotate { .. }) => Outcome::Changed,
            Some(Drag::Free { points }) => self.commit_drawing(&points),
            Some(Drag::Reconnect { edge, end, .. }) => self.reconnect(&edge, end, world),
            Some(Drag::Bend { .. }) => Outcome::Changed,
            // selecting is not a change to the file, so nothing is saved
            Some(Drag::Marquee { origin, cur }) => {
                self.selected = self.enclosed(spanning(origin, cur));
                Outcome::None
            }
            Some(Drag::Pan { .. }) | None => Outcome::None,
        }
    }

    /// A finished pen stroke becomes a node the size of what was drawn, with
    /// the points stored in `0..=1` of that box — so moving and resizing it
    /// afterwards are the code that already exists.
    fn commit_drawing(&mut self, points: &[Point]) -> Outcome {
        if points.len() < 2 {
            return Outcome::None;
        }
        let (mut lo, mut hi) = (points[0], points[0]);
        for p in points {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        // a perfectly straight stroke has no extent across, and dividing by
        // that is how every point lands on one edge of the box
        const FLOOR: f64 = 4.0;
        let (w, h) = ((hi.x - lo.x).max(FLOOR), (hi.y - lo.y).max(FLOOR));
        let rect = Rect::from_origin_size(
            Point::new((lo.x + hi.x - w) / 2.0, (lo.y + hi.y - h) / 2.0),
            Size::new(w, h),
        );
        // three decimals: enough for a stroke, and the file stays readable
        let round = |v: f64| ((v * 1000.0).round() / 1000.0) as f32;
        let normalised = points
            .iter()
            .map(|p| {
                [
                    round((p.x - rect.x0) / rect.width()),
                    round((p.y - rect.y0) / rect.height()),
                ]
            })
            .collect();
        let mut node = vault::Node::drawing(self.canvas.next_id(), rect, normalised);
        node.set_stroke(self.pen);
        node.color = self.pen_color.clone();
        self.canvas.nodes.push(node);
        // ⚠️ nothing selected afterwards, where a shape leaves itself selected:
        // the pen stays in hand for the next stroke, and a selection outline
        // round the last one is in the way of drawing beside it
        self.selected.clear();
        Outcome::Created { edit: false }
    }

    /// Where an edge's end landed. Dropping it on a **face** pins it there;
    /// dropping it on the **middle** of a node lets go and hands the side back
    /// to the automatic one — without that, brushing a face once would cost the
    /// auto-routing forever with nothing on screen to explain it.
    ///
    /// Dropping it on a *different* node rewires the edge, which until now was
    /// not something you could do at all.
    fn reconnect(&mut self, edge: &str, end: End, world: Point) -> Outcome {
        let Some(node) = self.canvas.hit(world) else {
            // over nothing: the end comes loose there (a point node)
            let at = self.new_point(world);
            if let Some(edge) = self.canvas.edge_mut(edge) {
                match end {
                    End::From => (edge.from_node, edge.from_side, edge.from_at) = (at, None, None),
                    End::To => (edge.to_node, edge.to_side, edge.to_at) = (at, None, None),
                }
            }
            self.drop_loose_points();
            return Outcome::Changed;
        };
        let (id, rect, local) = (node.id.clone(), node.rect(), node.local(world));
        let point = node.shape() == Shape::Point;
        let core = (rect.width().min(rect.height()) * 0.25).clamp(6.0, 40.0);
        // near the border: pinned to that side, at that place along it — the
        // middle when that's where it was dropped. A loose end has no sides
        let side = (!point && !rect.inflate(-core, -core).contains(local))
            .then(|| vault::Side::nearest(rect, local));
        let along = side
            .map(|s| (s.fraction(rect, local) * 1000.0).round() / 1000.0)
            .filter(|t| (t - 0.5).abs() > 0.08);
        let pin = side.map(|s| s.as_str().to_string());

        let Some(edge) = self.canvas.edge_mut(edge) else {
            return Outcome::None;
        };
        // an edge from a node to itself has nowhere to go
        let other = match end {
            End::From => &edge.to_node,
            End::To => &edge.from_node,
        };
        if *other == id {
            return Outcome::None;
        }
        match end {
            End::From => (edge.from_node, edge.from_side, edge.from_at) = (id, pin, along),
            End::To => (edge.to_node, edge.to_side, edge.to_at) = (id, pin, along),
        }
        self.drop_loose_points();
        Outcome::Changed
    }

    /// The selected edge's middle dragged to `world`: it bends through it.
    /// Dragged back onto the straight middle, it is straight again — the key
    /// leaves the file.
    fn bend(&mut self, id: &str, world: Point) {
        let ends = self.canvas.edge(id).and_then(|e| {
            let from = self.canvas.node(&e.from_node)?;
            let to = self.canvas.node(&e.to_node)?;
            let (a, _, b, _) = vault::edge_ends(e, from, to);
            Some(a.midpoint(b))
        });
        let snap = 6.0 / self.view.zoom;
        if let (Some(mid), Some(edge)) = (ends, self.canvas.edge_mut(id)) {
            let by = world - mid;
            // a tenth of a px is finer than anyone drags, and the file stays readable
            let tenth = |v: f64| ((v * 10.0).round() / 10.0) as f32;
            edge.bend = (by.hypot() > snap).then_some([tenth(by.x), tenth(by.y)]);
        }
    }

    pub fn pane_size(&self) -> Size {
        self.size.get()
    }

    /// A home for a new node near `at` that does not land on top of an
    /// existing one — overlapping nodes are legal, they just make an ugly
    /// first impression (and iced's canvas cannot hide the caption beneath).
    fn free_spot(&self, at: Point, size: Size) -> Point {
        let mut origin = Point::new(at.x - size.width / 2.0, at.y - size.height / 2.0);
        for _ in 0..24 {
            let rect = Rect::from_origin_size(origin, size);
            let clash = self
                .canvas
                .nodes
                .iter()
                .any(|n| n.rect().overlaps(rect.inflate(-4.0, -4.0)));
            if !clash {
                break;
            }
            origin.x += 28.0;
            origin.y += 24.0;
        }
        origin
    }

    // --- the style bar's verbs (ADR-004) ---------------------------------
    //
    // Each one applies to the whole selection. ⚠️ Every TOGGLE follows the
    // FIRST of what is selected, never each item's own state — otherwise a
    // mixed selection just flips each one against its neighbours and the key
    // does nothing you can see.

    /// Every selected node's outline. ⚠️ Never across the line between a box
    /// and bare text, either way: a shape is not flattened into words and
    /// words are not given a box. The bar hides the group for a label, but a
    /// box and a label selected together still show it.
    pub fn set_shape(&mut self, shape: Shape) {
        let bare = shape == Shape::Label;
        for what in self.selected.clone() {
            let node = what.node().and_then(|id| self.canvas.node_mut(id));
            if let Some(node) = node.filter(|n| (n.shape() == Shape::Label) == bare) {
                node.set_shape(shape);
            }
        }
    }

    pub fn set_align(&mut self, align: vault::Align) {
        for what in self.selected.clone() {
            if let Some(node) = what.node().and_then(|id| self.canvas.node_mut(id)) {
                node.set_align(align);
            }
        }
    }

    /// The face of what is written: the board's hand or the app's mono, on
    /// every selected node with words of its own (a caption, a frame's name)
    /// and every edge (its label). A drawing and a card have no words to set.
    pub fn set_mono(&mut self, mono: bool) {
        for what in self.selected.clone() {
            match what {
                Selection::Node(id) => {
                    if let Some(node) = self
                        .canvas
                        .node_mut(&id)
                        .filter(|n| n.draw.is_none() && n.file.is_none())
                    {
                        node.set_mono(mono);
                    }
                }
                Selection::Edge(id) => {
                    if let Some(edge) = self.canvas.edge_mut(&id) {
                        edge.set_mono(mono);
                    }
                }
            }
        }
    }

    /// Bold, on the nodes that have words of their own — a drawing and a card
    /// are not text nodes, whatever else is selected with them.
    pub fn toggle_bold(&mut self) {
        let nodes: Vec<String> = self
            .selected
            .iter()
            .filter_map(Selection::node)
            .filter_map(|id| self.canvas.node(id))
            .filter(|node| node.kind == "text" && node.draw.is_none())
            .map(|node| node.id.clone())
            .collect();
        let on = nodes
            .first()
            .and_then(|id| self.canvas.node(id))
            .is_some_and(|node| !node.bold());
        for id in nodes {
            if let Some(node) = self.canvas.node_mut(&id) {
                node.set_bold(on);
            }
        }
    }

    /// One key, one meaning: the same thickness is a drawing's stroke, any
    /// shape's border and a connector's line.
    pub fn set_stroke(&mut self, index: usize) {
        let px = vault::STROKES[index.min(vault::STROKES.len() - 1)];
        for id in self.selected_edges() {
            if let Some(edge) = self.canvas.edge_mut(&id) {
                edge.set_width(px);
            }
        }
        for what in self.selected.clone() {
            if let Some(node) = what.node().and_then(|id| self.canvas.node_mut(id)) {
                node.set_stroke(px);
            }
        }
    }

    /// The thickness the pen draws its next stroke at — the same three steps
    /// as [`Scene::set_stroke`], and nothing already drawn changes.
    pub fn set_pen(&mut self, index: usize) {
        self.pen = vault::STROKES[index.min(vault::STROKES.len() - 1)];
    }

    pub fn set_route(&mut self, route: vault::Route) {
        for id in self.selected_edges() {
            if let Some(edge) = self.canvas.edge_mut(&id) {
                edge.set_route(route);
            }
        }
    }

    pub fn toggle_dash(&mut self) {
        let edges = self.selected_edges();
        let on = edges
            .first()
            .and_then(|id| self.canvas.edge(id))
            .is_some_and(|edge| !edge.dashed());
        for id in edges {
            if let Some(edge) = self.canvas.edge_mut(&id) {
                edge.set_dashed(on);
            }
        }
    }

    /// The head at one end of every selected edge. [`End::To`] is the
    /// destination — the one the spec has on by default.
    pub fn toggle_arrow(&mut self, end: End) {
        let edges = self.selected_edges();
        let on = edges
            .first()
            .and_then(|id| self.canvas.edge(id))
            .is_some_and(|edge| match end {
                End::To => !edge.has_arrow(),
                End::From => !edge.back_arrow(),
            });
        for id in edges {
            let Some(edge) = self.canvas.edge_mut(&id) else {
                continue;
            };
            match end {
                End::To => edge.set_arrow(on),
                End::From => edge.set_back_arrow(on),
            }
        }
    }

    /// A preset name or a `#RRGGBB`, and `None` for "the theme's own" — which
    /// takes the key out of the file rather than writing a default into it.
    pub fn set_color(&mut self, color: Option<String>) {
        for what in self.selected.clone() {
            match what {
                Selection::Node(id) => {
                    if let Some(node) = self.canvas.node_mut(&id) {
                        node.color = color.clone();
                    }
                }
                Selection::Edge(id) => {
                    if let Some(edge) = self.canvas.edge_mut(&id) {
                        edge.color = color.clone();
                    }
                }
            }
        }
    }

    /// Draw order *is* z-order, so restacking is a move within the vec.
    pub fn restack(&mut self, id: &str, to_front: bool) {
        let Some(i) = self.canvas.nodes.iter().position(|n| n.id == id) else {
            return;
        };
        let node = self.canvas.nodes.remove(i);
        if to_front {
            self.canvas.nodes.push(node);
        } else {
            self.canvas.nodes.insert(0, node);
        }
    }

    /// What ⌘C takes off the board: one `[[canvas: …]]` per selected node,
    /// which is a **live reference** when it lands in a note — the syntax
    /// `Message::Follow` already knows how to walk back here. A picture card is
    /// the exception: it copies as `![](file)`, so it lands as the picture.
    ///
    /// A node with no caption has nothing to be referred to by, so it is left
    /// out of the text; the nodes themselves still travel in [`copied_nodes`].
    pub fn copy_text(&self) -> Option<String> {
        let lines: Vec<String> = self
            .selected
            .iter()
            .filter_map(|what| match what {
                Selection::Node(id) => {
                    // a picture copies as the picture — the same vault file, so
                    // a note draws it instead of linking to the card holding it
                    let node = self.canvas.node(id)?;
                    if let Some(file) = node.file.as_deref() {
                        if node.shape() == Shape::File && vault::is_image(file) {
                            return Some(format!("![]({file})"));
                        }
                    }
                    // ⚠️ folded to one line. Canvas text is deliberately
                    // multi-line and a mention cannot be: `inline` reads them a
                    // line at a time, so a newline between the brackets leaves
                    // `[[` on one line and `]]` on the next, and the link is
                    // dead the moment the note is reopened. PR #16 adds
                    // `vault::one_line` for exactly this — fold it inline here
                    // so the two branches do not collide; dedupe on merge.
                    let caption = node.caption();
                    let caption = caption.split_whitespace().collect::<Vec<_>>().join(" ");
                    (!caption.is_empty()).then(|| {
                        // Carry the board as identity: a caption-only mention
                        // changes meaning as soon as another canvas is open.
                        // The alias keeps what the note shows as just the card's
                        // caption rather than exposing the routing syntax.
                        format!("[[canvas: {}#id:{id}|{caption}]]", self.name())
                    })
                }
                // an edge has no reference of its own — its name is all there is
                Selection::Edge(id) => {
                    let label = self.canvas.edge(id)?.label.as_deref()?.trim();
                    (!label.is_empty()).then(|| label.to_string())
                }
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// The selected nodes themselves. Text on the clipboard is what other apps
    /// can read; this is what makes a paste back here keep the colour, the
    /// size and the shape instead of a name.
    pub fn copied_nodes(&self) -> Vec<vault::Node> {
        self.selected
            .iter()
            .filter_map(|what| self.canvas.node(what.node()?).cloned())
            .collect()
    }

    /// Copies of `nodes`, landing with their top-left at `at` and keeping the
    /// shape they had relative to one another.
    pub fn paste_nodes(&mut self, at: Point, nodes: &[vault::Node]) {
        let Some(origin) = corner(nodes) else {
            return;
        };
        self.selected.clear();
        // a loose arrow end is only ever its arrow's, and arrows aren't
        // copied: pasted alone it would be an invisible node nothing uses
        for node in nodes.iter().filter(|n| n.shape() != Shape::Point) {
            let mut copy = node.clone();
            copy.id = self.canvas.next_id();
            copy.x += (at.x - origin.x) as f32;
            copy.y += (at.y - origin.y) as f32;
            self.selected.push(Selection::Node(copy.id.clone()));
            self.canvas.nodes.push(copy);
        }
        // ⚠️ the copies carry the original's `excerpt`, and the stale verdict is
        // keyed by node id — so a copy of a stale quote starts life looking
        // fresh. Only a note edit would have corrected it, and copying a card
        // edits no note. `duplicate_selection` comes through here too.
        self.forget_quote_check();
    }

    /// ⌘D. Returns whether there was anything to copy.
    pub fn duplicate_selection(&mut self) -> bool {
        let nodes = self.copied_nodes();
        let Some(origin) = corner(&nodes) else {
            return false;
        };
        self.paste_nodes(Point::new(origin.x + 16.0, origin.y + 16.0), &nodes);
        true
    }

    /// Pasted text that refers to nothing: a label where it landed.
    pub fn add_label(&mut self, at: Point, text: &str) {
        let (w, h) = Shape::Label.default_size();
        let lines = text.lines().count().max(1) as f32;
        let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
        let size = Size::new(
            f64::from(w.max(widest as f32 * TEXT_BASE * 0.6 + 8.0)),
            f64::from(h.max(lines * TEXT_BASE * 1.4)),
        );
        let rect = Rect::from_origin_size(self.free_spot(at, size), size);
        let id = self.canvas.next_id();
        self.canvas
            .nodes
            .push(vault::Node::new(id.clone(), Shape::Label, rect, text));
        self.select_one(Selection::Node(id));
    }

    /// "Send to canvas": a doc card that quotes the selected slice of the note.
    /// It is a reference, not a copy — the text stays editable in the note.
    pub fn add_excerpt(&mut self, at: Point, file: &str, excerpt: &str) {
        let (w, h) = Shape::File.default_size();
        // sized to the wrapped quote, so the card never shows a half-empty
        // body — long quotes stop at MAX_QUOTE_LINES with an ellipsis
        const MAX_QUOTE_LINES: usize = 24;
        let lines = wrap(excerpt.trim(), card_columns(w))
            .len()
            .min(MAX_QUOTE_LINES);
        let size = Size::new(f64::from(w), f64::from(card_height(lines).max(h)));
        let rect = Rect::from_origin_size(self.free_spot(at, size), size);
        let id = self.canvas.next_id();
        let mut node = vault::Node::new(id.clone(), Shape::File, rect, "");
        node.file = Some(format!("{}.md", file.trim().trim_end_matches(".md")));
        node.excerpt = Some(excerpt.trim().to_string());
        self.canvas.nodes.push(node);
        self.select_one(Selection::Node(id));
    }

    /// A picture pasted from the clipboard: a file node at its own size, in
    /// the middle of what you are looking at.
    pub fn add_image(&mut self, at: Point, file: &str, size: Size) {
        let rect = Rect::from_origin_size(self.free_spot(at, size), size);
        let id = self.canvas.next_id();
        let mut node = vault::Node::new(id.clone(), Shape::File, rect, "");
        node.file = Some(file.to_string());
        self.canvas.nodes.push(node);
        self.select_one(Selection::Node(id));
    }

    /// Mid-gesture: a drag is a single change, however long the mouse is down.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn is_rotating(&self) -> bool {
        matches!(self.drag, Some(Drag::Rotate { .. }))
    }

    /// Back out of the inline editor, leaving the node as it was.
    pub fn cancel_edit(&mut self) {
        if let Some(Selection::Node(id)) = self.editing.take() {
            // the box as it was too: what typing grew, Esc takes back
            if let (Some(r), Some(node)) = (self.edit_box.take(), self.canvas.node_mut(&id)) {
                node.set_rect(r);
            }
            self.drop_if_blank(&id);
        }
        self.edit_text.clear();
    }

    /// Replace the buffer wholesale.
    pub fn set_edit_text(&mut self, value: &str) {
        self.edit_text = value.to_string();
    }

    fn begin_edit(&mut self, what: Selection) {
        let start = match &what {
            Selection::Node(id) => {
                let Some(node) = self.canvas.node(id) else {
                    return;
                };
                // a quote card has no label of its own — the text lives in the
                // note, and that is where it stays editable. A drawing's text
                // is the placeholder that keeps it a legal `text` node, and
                // emptying it would make the node illegal
                if node.excerpt.is_some() || matches!(node.shape(), Shape::Draw | Shape::Point) {
                    return;
                }
                node.edit_label()
            }
            Selection::Edge(id) => match self.canvas.edge(id) {
                Some(edge) => edge.label.clone().unwrap_or_default(),
                None => return,
            },
        };
        self.edit_box = what
            .node()
            .and_then(|id| self.canvas.node(id))
            .map(vault::Node::rect);
        self.edit_text = start.to_string();
        self.edit_from = self.edit_text.clone();
        self.editing = Some(what);
    }

    /// Commit the inline editor. `[[name]]` turns the node into a doc card —
    /// the canvas half of the mention system.
    pub fn commit_edit(&mut self) {
        let editing = self.editing.take();
        let value = self.edit_text.clone().trim().to_string();
        let id = match editing {
            Some(Selection::Node(id)) => id,
            Some(Selection::Edge(id)) => {
                if let Some(edge) = self.canvas.edge_mut(&id) {
                    // an empty label is no label: the key leaves the file
                    edge.label = (!value.is_empty()).then_some(value);
                }
                self.edit_text.clear();
                return;
            }
            None => return,
        };
        let mention = value
            .strip_prefix("[[")
            .and_then(|rest| rest.strip_suffix("]]"))
            .map(|name| name.trim().to_string());
        match mention {
            Some(name) => {
                let (file, section) = match name.split_once('#') {
                    Some((file, heading)) => (file.trim(), Some(heading.trim().to_string())),
                    None => (name.as_str(), None),
                };
                self.attach_ref(Some(id), Point::ORIGIN, file, section);
            }
            None => {
                if let Some(node) = self.canvas.node_mut(&id) {
                    node.set_edit_label(value);
                }
                self.drop_if_blank(&id);
            }
        }
        self.edit_text.clear();
    }

    /// Turn a node into a doc-reference card — or drop a new card at `at`
    /// (world coordinates) when there is no node yet. The canvas half of `[[ ]]`.
    pub fn attach_ref(
        &mut self,
        node: Option<String>,
        at: Point,
        file: &str,
        section: Option<String>,
    ) {
        // ⚠️ An extension the caller supplied is a DECISION, not a hint. The
        // palette already knows whether you picked the note or the board, and
        // re-deriving it from the filesystem handed you `roadmap.canvas` when
        // you chose the note `roadmap` — silently, with a blank preview. Only a
        // bare name is guessed, and that is the `[[nome]]` typed inside a card,
        // where nobody has decided yet.
        let file = file.trim();
        let file = if file.ends_with(".md") || file.ends_with(".canvas") {
            file.to_string()
        } else {
            let is_board = self
                .path
                .parent()
                .is_some_and(|dir| dir.join(format!("{file}.canvas")).exists());
            format!("{file}.{}", if is_board { "canvas" } else { "md" })
        };
        let (w, h) = Shape::File.default_size();
        let id = match node {
            Some(id) => id,
            None => {
                let id = self.canvas.next_id();
                let size = Size::new(f64::from(w), f64::from(h));
                self.canvas.nodes.push(vault::Node::new(
                    id.clone(),
                    Shape::File,
                    Rect::from_origin_size(self.free_spot(at, size), size),
                    &file,
                ));
                id
            }
        };
        if let Some(node) = self.canvas.node_mut(&id) {
            node.kind = "file".into();
            node.shape = None;
            node.text = String::new();
            node.file = Some(file);
            node.subpath = section.map(|s| format!("#{s}"));
            node.width = node.width.max(w);
            node.height = node.height.max(h);
        }
        self.select_one(Selection::Node(id));
    }

    /// The shape being rubber-banded right now, if any.
    pub fn preview(&self) -> Option<(Shape, Rect)> {
        match &self.drag {
            Some(Drag::Create { shape, origin, cur }) => Some((*shape, normalized(*origin, *cur))),
            _ => None,
        }
    }

    pub fn drawing(&self) -> Option<&[Point]> {
        match &self.drag {
            Some(Drag::Free { points }) => Some(points),
            _ => None,
        }
    }

    pub fn marquee(&self) -> Option<Rect> {
        match &self.drag {
            Some(Drag::Marquee { origin, cur }) => Some(spanning(*origin, *cur)),
            _ => None,
        }
    }

    pub fn connecting(&self) -> Option<(Point, Point)> {
        match &self.drag {
            Some(Drag::Connect { from, cur, .. }) => {
                self.canvas.node(from).map(|n| (n.rect().center(), *cur))
            }
            // the end that is NOT moving stays put while the other follows
            Some(Drag::Reconnect { edge, end, cur }) => {
                let line = self.edge_points(self.canvas.edge(edge)?)?;
                let anchor = match end {
                    End::From => line.last()?,
                    End::To => line.first()?,
                };
                Some((*anchor, *cur))
            }
            _ => None,
        }
    }

    /// Where the inline editor goes, on screen: the node's own box, or a
    /// field's worth of room at the label's anchor on the edge.
    pub fn edit_rect(&self) -> Option<Rect> {
        match self.editing.as_ref()? {
            Selection::Node(id) => Some(self.view.screen_rect(self.canvas.node(id)?.rect())),
            Selection::Edge(id) => {
                let edge = self.canvas.edge(id)?;
                let at = self
                    .view
                    .to_screen(label_anchor(&self.edge_points(edge)?, edge.route()));
                Some(Rect::from_origin_size(
                    Point::new(at.x - 60.0, at.y - 11.0),
                    Size::new(120.0, 22.0),
                ))
            }
        }
    }

    /// The two ends of the selected edge, on screen, and whether each is
    /// pinned — the handles, and the only place the pin is visible.
    pub fn edge_handles(&self) -> Option<(Point, bool, Point, bool)> {
        let edge = self.canvas.edge(self.only_edge()?)?;
        let line = self.edge_points(edge)?;
        Some((
            self.view.to_screen(*line.first()?),
            edge.face_from().is_some(),
            self.view.to_screen(*line.last()?),
            edge.face_to().is_some(),
        ))
    }
}

fn tool_shape(icon: Tool) -> Option<Shape> {
    Some(match icon {
        Tool::Rect => Shape::Rect,
        Tool::Ellipse => Shape::Ellipse,
        Tool::Triangle => Shape::Triangle,
        Tool::Text => Shape::Label,
        Tool::Frame => Shape::Frame,
        _ => return None,
    })
}

/// Arrow and line both make edges; only the head differs.
fn connector_arrow(icon: Tool) -> Option<bool> {
    match icon {
        Tool::Arrow => Some(true),
        Tool::Line => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Canvas;
    use kurbo::Size;

    /// ⚠️ no tool on the rail may drag out a `Shape::File`. A file card is a
    /// card that POINTS at something, and a drag has nothing to point at:
    /// `release` builds every shape as `Node::new(id, shape, rect, "")`, with
    /// no `file`. The `Image` tool used to map here anyway and the result was
    /// a `"type":"file"` node aimed at nothing — double-clicking it asked the
    /// shell to open the empty path.
    ///
    /// The three paths that make a real file card all carry the file with
    /// them: `add_image`, `add_excerpt` and `attach_ref`.
    #[test]
    fn no_tool_drags_out_a_card_that_points_at_nothing() {
        for tool in [
            Tool::Select,
            Tool::Pen,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Triangle,
            Tool::Text,
            Tool::Frame,
            Tool::Arrow,
            Tool::Line,
        ] {
            assert_ne!(
                tool_shape(tool),
                Some(Shape::File),
                "{tool:?} drags out a card with nowhere to go"
            );
        }
    }

    fn scene_with(nodes: Vec<vault::Node>) -> Scene {
        let mut scene = Scene::load(PathBuf::from("/tmp/synapso-test-not-written.canvas"));
        scene.canvas = Canvas {
            nodes,
            edges: vec![],
            ..Default::default()
        };
        scene
    }

    fn node(id: &str, shape: Shape, x: f64, y: f64, w: f64, h: f64) -> vault::Node {
        vault::Node::new(
            id.into(),
            shape,
            Rect::from_origin_size(Point::new(x, y), Size::new(w, h)),
            "",
        )
    }

    /// ⚠️ The rule every toggle on the style bar follows: the group takes the
    /// opposite of the FIRST one selected. Flipping each against its own state
    /// leaves a mixed selection exactly as mixed as it was, and the key looks
    /// broken.
    #[test]
    fn a_toggle_over_a_group_follows_the_first_one() {
        let mut scene = scene_with(vec![
            node("a", Shape::Rect, 0.0, 0.0, 40.0, 40.0),
            node("b", Shape::Rect, 80.0, 0.0, 40.0, 40.0),
        ]);
        let mut dashed = vault::Edge {
            id: "dashed".into(),
            from_node: "a".into(),
            to_node: "b".into(),
            ..Default::default()
        };
        dashed.set_dashed(true);
        let solid = vault::Edge {
            id: "solid".into(),
            from_node: "b".into(),
            to_node: "a".into(),
            ..Default::default()
        };
        scene.canvas.edges = vec![dashed, solid];
        scene.selected = vec![
            Selection::Edge("dashed".into()),
            Selection::Edge("solid".into()),
        ];

        // the first is dashed, so the group goes solid — both of them
        scene.toggle_dash();
        assert!(!scene.canvas.edge("dashed").unwrap().dashed());
        assert!(!scene.canvas.edge("solid").unwrap().dashed());
        // and back, together
        scene.toggle_dash();
        assert!(scene.canvas.edge("dashed").unwrap().dashed());
        assert!(scene.canvas.edge("solid").unwrap().dashed());
    }

    /// One key, one meaning: the same thickness is a shape's border and a
    /// connector's line, and a mixed selection gets both.
    #[test]
    fn the_thickness_key_reaches_a_border_and_a_line_at_once() {
        let mut scene = scene_with(vec![
            node("a", Shape::Rect, 0.0, 0.0, 40.0, 40.0),
            node("b", Shape::Rect, 80.0, 0.0, 40.0, 40.0),
        ]);
        scene.canvas.edges = vec![vault::Edge {
            id: "e".into(),
            from_node: "a".into(),
            to_node: "b".into(),
            ..Default::default()
        }];
        scene.selected = vec![Selection::Node("a".into()), Selection::Edge("e".into())];
        scene.set_stroke(2);
        assert_eq!(scene.canvas.node("a").unwrap().stroke(), vault::STROKES[2]);
        assert_eq!(scene.canvas.edge("e").unwrap().width(), vault::STROKES[2]);
        // and nothing was said about the node nobody selected
        assert_eq!(scene.canvas.node("b").unwrap().stroke(), vault::STROKES[0]);
    }

    /// Bold is about words. A drawing has none, whatever is selected with it.
    #[test]
    fn bold_skips_what_has_no_words_of_its_own() {
        let mut drawing = node("pen", Shape::Draw, 0.0, 0.0, 40.0, 40.0);
        drawing.draw = Some(vec![[0.0, 0.0], [1.0, 1.0]]);
        let mut scene = scene_with(vec![node("a", Shape::Rect, 0.0, 0.0, 40.0, 40.0), drawing]);
        scene.selected = vec![Selection::Node("a".into()), Selection::Node("pen".into())];
        scene.toggle_bold();
        assert!(scene.canvas.node("a").unwrap().bold());
        assert!(!scene.canvas.node("pen").unwrap().bold());
    }

    /// "No colour of its own" takes the key out of the file rather than
    /// writing a default into it — the same promise every other setter keeps.
    #[test]
    fn clearing_a_colour_takes_the_key_out_of_the_file() {
        let mut scene = scene_with(vec![node("a", Shape::Rect, 0.0, 0.0, 40.0, 40.0)]);
        scene.selected = vec![Selection::Node("a".into())];
        scene.set_color(Some("#A78BFA".into()));
        let written = serde_json::to_string(scene.canvas.node("a").unwrap()).unwrap();
        assert!(written.contains("#A78BFA"));
        scene.set_color(None);
        let written = serde_json::to_string(scene.canvas.node("a").unwrap()).unwrap();
        assert!(!written.contains("color"), "{written}");
    }

    /// ADR-004 §D4 — a control that is visible has to work, so it is only
    /// visible when it works on *everything* selected. Shown with a shape and
    /// an arrow together, an "elbow" key would either lie or surprise.
    #[test]
    fn fitting_an_empty_board_puts_the_viewport_back_to_default() {
        let mut scene = scene_with(vec![]);
        scene.view.zoom = 3.0;
        scene.zoom_to_fit();
        assert_eq!(scene.view, Viewport::default());
    }

    #[test]
    fn fitting_puts_the_middle_of_the_board_in_the_middle_of_the_pane() {
        let mut scene = scene_with(vec![
            node("a", Shape::Rect, 0.0, 0.0, 100.0, 100.0),
            node("b", Shape::Rect, 900.0, 500.0, 100.0, 100.0),
        ]);
        let pane = Size::new(800.0, 600.0);
        scene.size.set(pane);
        scene.zoom_to_fit();
        let centre = scene.view.to_screen(Point::new(500.0, 300.0));
        assert!((centre.x - 400.0).abs() < 1e-9 && (centre.y - 300.0).abs() < 1e-9);
        // and everything the board holds is inside the pane
        for n in &scene.canvas.nodes {
            let on_screen = scene.view.screen_rect(n.rect());
            assert!(
                on_screen.x0 >= 0.0 && on_screen.x1 <= pane.width,
                "{on_screen:?} hangs out of the pane"
            );
        }
    }

    #[test]
    fn the_bar_offers_only_what_everything_selected_understands() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Ellipse, 200.0, 0.0, 100.0, 40.0),
            node("card", Shape::File, 0.0, 100.0, 100.0, 40.0),
            vault::Node::drawing(
                "ink".into(),
                kurbo::Rect::from_origin_size(Point::ORIGIN, Size::new(10.0, 10.0)),
                vec![[0.0, 0.0], [1.0, 1.0]],
            ),
        ]);
        scene.canvas.edges.push(vault::Edge {
            id: "e".into(),
            from_node: "1".into(),
            to_node: "2".into(),
            ..Default::default()
        });
        let styleable = |scene: &Scene| scene.styleable();

        // nothing selected: no bar at all
        assert_eq!(styleable(&scene), Styleable::default());

        scene.selected = vec![Selection::Node("1".into()), Selection::Node("2".into())];
        let both = styleable(&scene);
        assert!(both.any && both.text && !both.edges, "two shapes: {both:?}");

        scene.selected = vec![Selection::Edge("e".into())];
        let arrow = styleable(&scene);
        assert!(
            arrow.any && arrow.edges && !arrow.text,
            "an edge: {arrow:?}"
        );

        // the mixed selection is the whole point of the rule
        scene.selected = vec![Selection::Node("1".into()), Selection::Edge("e".into())];
        let mixed = styleable(&scene);
        assert!(
            mixed.any && !mixed.text && !mixed.edges,
            "colour and thickness only: {mixed:?}"
        );

        // a card has no shape of its own and a drawing has no caption to move
        for id in ["card", "ink"] {
            scene.selected = vec![Selection::Node(id.into())];
            let it = styleable(&scene);
            assert!(it.any && !it.text && !it.edges, "{id}: {it:?}");
        }

        // bare text has a caption to align and nothing else: no box to
        // reshape, no border to thicken — the two groups that are about a box
        scene
            .canvas
            .nodes
            .push(node("t", Shape::Label, 0.0, 200.0, 120.0, 22.0));
        scene.selected = vec![Selection::Node("t".into())];
        let bare = styleable(&scene);
        assert!(bare.any && bare.text && bare.label, "a label: {bare:?}");

        // put one box in the selection and the box controls are back
        scene.selected = vec![Selection::Node("t".into()), Selection::Node("1".into())];
        assert!(
            !styleable(&scene).label,
            "a label next to a rect is not a bare selection"
        );
    }

    /// ADR-001: a failed load must never become an eligible save. The board
    /// on screen is a `Canvas::default()` standing in for a file nobody could
    /// read, and writing it out would replace that file with an empty one.
    ///
    /// ⚠️ this used to carry the doc comment of a ⏎/⇧⏎ caption test that moved
    /// to `card.rs` — the comment stayed behind and described the wrong test.
    #[test]
    fn a_canvas_that_failed_to_load_is_never_saved_over() {
        let path = std::env::temp_dir().join("synapso-broken-scene.canvas");
        std::fs::write(&path, "not json at all {{{").unwrap();

        let mut scene = Scene::load(path.clone());
        assert!(scene.broken.is_some());
        assert!(scene.canvas.nodes.is_empty(), "it draws as empty");

        scene
            .canvas
            .nodes
            .push(node("1", Shape::Rect, 0.0, 0.0, 10.0, 10.0));
        assert!(scene.save().is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "not json at all {{{"
        );
    }

    /// `drag_to` used to `self.drag.clone()` on every pointer move, and
    /// `Drag::Free` carries every point in the squiggle so far — one freehand
    /// stroke was O(n²) in its own length. Borrowing instead has to leave
    /// every arm doing exactly what it did.
    #[test]
    fn every_drag_arm_still_does_what_it_did_without_the_clone() {
        // freehand: points accumulate, and only one every couple of units
        let mut scene = scene_with(vec![]);
        scene.drag = Some(Drag::Free {
            points: vec![Point::ZERO],
        });
        for i in 1..=50 {
            scene.drag_to(Point::new(f64::from(i) * 3.0, 0.0), Point::ZERO, false);
        }
        scene.drag_to(Point::new(150.5, 0.0), Point::ZERO, false);
        let Some(Drag::Free { points }) = &scene.drag else {
            panic!("the pen let go of its stroke")
        };
        assert_eq!(
            points.len(),
            51,
            "the half-unit move should have been dropped"
        );

        // move: every node follows, and `last` advances so the NEXT move is a
        // delta and not an offset from the origin
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 10.0, 10.0)]);
        scene.drag = Some(Drag::Move {
            start: Point::ZERO,
            ids: vec![("1".into(), Rect::new(0.0, 0.0, 10.0, 10.0))],
        });
        scene.drag_to(Point::new(5.0, 5.0), Point::ZERO, false);
        scene.drag_to(Point::new(8.0, 5.0), Point::ZERO, false);
        assert_eq!(
            scene.canvas.node("1").unwrap().rect().origin(),
            Point::new(8.0, 5.0),
            "the second move was measured from the wrong place"
        );

        // pan moves the view by the SCREEN delta, and advances its own `last`
        let mut scene = scene_with(vec![]);
        let was = scene.view.offset;
        scene.drag = Some(Drag::Pan { last: Point::ZERO });
        scene.drag_to(Point::ZERO, Point::new(10.0, 4.0), false);
        scene.drag_to(Point::ZERO, Point::new(13.0, 4.0), false);
        assert_eq!(scene.view.offset, was + Vec2::new(13.0, 4.0));

        // the four that only carry a `cur`
        let mut scene = scene_with(vec![]);
        scene.drag = Some(Drag::Marquee {
            origin: Point::ZERO,
            cur: Point::ZERO,
        });
        scene.drag_to(Point::new(9.0, 9.0), Point::ZERO, false);
        assert!(matches!(
            scene.drag,
            Some(Drag::Marquee { origin, cur })
                if origin == Point::ZERO && cur == Point::new(9.0, 9.0)
        ));
        scene.drag = Some(Drag::Create {
            shape: Shape::Rect,
            origin: Point::ZERO,
            cur: Point::ZERO,
        });
        scene.drag_to(Point::new(4.0, 7.0), Point::ZERO, false);
        assert!(
            matches!(scene.drag, Some(Drag::Create { cur, .. }) if cur == Point::new(4.0, 7.0))
        );

        // and nothing in flight is still nothing
        scene.drag = None;
        assert!(matches!(
            scene.drag_to(Point::new(1.0, 1.0), Point::ZERO, false),
            Outcome::None
        ));
    }

    fn scratch_board(name: &str, json: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("synapso-rescue-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("board.canvas");
        std::fs::write(&path, json).unwrap();
        path
    }

    const OTHER_WRITER: &str = r#"{"nodes":[],"edges":[]}"#;

    /// Someone else writes the file underneath us — the stamp is now stale.
    fn moved_under_us(path: &PathBuf) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(path, OTHER_WRITER).unwrap();
    }

    /// README: "a refused canvas save writes a `-unsaved.canvas` file on the
    /// refusal itself". The board on screen is the only copy of the edit, and
    /// a refusal that keeps it in memory alone loses it to the next ⌘Q.
    #[test]
    fn a_refused_canvas_save_leaves_the_board_beside_it() {
        let path = scratch_board("refused", "{\"nodes\":[],\"edges\":[]}");
        let mut scene = Scene::load(path.clone());
        scene
            .canvas
            .nodes
            .push(node("1", Shape::Rect, 0.0, 0.0, 10.0, 10.0));
        moved_under_us(&path);

        let refused = scene.save();
        // the board on disk is still the other writer's — refusing was right
        assert_eq!(std::fs::read_to_string(&path).unwrap(), OTHER_WRITER);
        // …and the in-memory board is beside it rather than only in this process
        let rescue = path.with_file_name("board-unsaved.canvas");
        assert!(rescue.exists(), "the refusal rescued nothing");
        assert!(std::fs::read_to_string(&rescue).unwrap().contains("\"1\""));
        assert_eq!(scene.rescued.as_ref(), Some(&rescue));
        // the refusal says where the copy went
        let why = refused.unwrap_err().to_string();
        assert!(why.contains("board-unsaved"), "{why}");

        // a second refusal updates the same file instead of starting a trail
        scene
            .canvas
            .nodes
            .push(node("2", Shape::Rect, 20.0, 0.0, 10.0, 10.0));
        assert!(scene.save().is_err());
        assert!(std::fs::read_to_string(&rescue).unwrap().contains("\"2\""));
        assert!(
            !path.with_file_name("board-unsaved-2.canvas").exists(),
            "a refusal per save would litter the vault"
        );
    }

    /// A rescue left by an earlier session may hold a different edit: this
    /// session's goes beside it, never over it.
    #[test]
    fn a_rescue_never_overwrites_one_left_by_an_earlier_session() {
        let path = scratch_board("earlier", "{\"nodes\":[],\"edges\":[]}");
        let earlier = path.with_file_name("board-unsaved.canvas");
        std::fs::write(&earlier, "from last night").unwrap();
        let mut scene = Scene::load(path.clone());
        scene
            .canvas
            .nodes
            .push(node("1", Shape::Rect, 0.0, 0.0, 10.0, 10.0));
        moved_under_us(&path);

        assert!(scene.save().is_err());
        assert_eq!(
            std::fs::read_to_string(&earlier).unwrap(),
            "from last night"
        );
        let mine = path.with_file_name("board-unsaved-2.canvas");
        assert!(std::fs::read_to_string(&mine).unwrap().contains("\"1\""));
        assert_eq!(scene.rescued.as_ref(), Some(&mine));
    }

    /// The rescue can fail too (its folder is gone). The refusal still stands,
    /// the file on disk is still not clobbered, and the error says both.
    #[test]
    fn a_rescue_that_cannot_be_written_is_still_a_refusal() {
        let path = scratch_board("norescue", "{\"nodes\":[],\"edges\":[]}");
        let mut scene = Scene::load(path.clone());
        scene.rescued = Some(path.with_file_name("gone/board-unsaved.canvas"));
        scene
            .canvas
            .nodes
            .push(node("1", Shape::Rect, 0.0, 0.0, 10.0, 10.0));
        moved_under_us(&path);

        let why = scene.save().unwrap_err().to_string();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), OTHER_WRITER);
        assert!(why.contains("rescue"), "{why}");
        assert!(scene.dirty(), "the edit is still only in memory");
    }

    /// Double-clicking a node opens the inline editor, and `EditSelected`
    /// falls through to `settle` → `save` → `flush_edit`. The buffer is
    /// written back TRIMMED, so a caption with whitespace on it — anything
    /// Obsidian or another editor wrote — came back changed on disk with
    /// nobody having pressed a key.
    #[test]
    fn opening_the_editor_writes_nothing_until_something_is_typed() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 10.0, 10.0)]);
        scene
            .canvas
            .node_mut("1")
            .unwrap()
            .set_caption("  spaced  ".into());
        let before = crate::history::canvas_json(&scene.canvas);

        scene.selected = vec![Selection::Node("1".into())];
        scene.handle(Event::EditSelected, Tool::Select);
        assert!(scene.editing.is_some(), "the editor is open");
        assert!(
            !scene.has_pending_edit(),
            "nothing was typed — and the heartbeat serialises the whole canvas \
             every 600 ms while this says otherwise"
        );

        scene.flush_edit();
        assert_eq!(
            crate::history::canvas_json(&scene.canvas),
            before,
            "opening an editor is not an edit"
        );

        // …and a real edit still lands, still trimmed
        scene.set_edit_text("  typed  ");
        assert!(scene.has_pending_edit());
        scene.flush_edit();
        assert_eq!(scene.canvas.node("1").unwrap().caption(), "typed");
        assert!(
            !scene.has_pending_edit(),
            "written — nothing left for the heartbeat to do"
        );
    }

    #[test]
    fn dragging_with_a_shape_tool_creates_that_node() {
        let mut scene = scene_with(vec![]);
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Ellipse);
        scene.handle(
            Event::Dragged(Point::new(110.0, 60.0), false),
            Tool::Ellipse,
        );
        scene.handle(Event::Released(Point::new(110.0, 60.0)), Tool::Ellipse);

        assert_eq!(scene.canvas.nodes.len(), 1);
        let created = &scene.canvas.nodes[0];
        assert_eq!(created.shape(), Shape::Ellipse);
        assert_eq!(
            created.rect(),
            kurbo::Rect::from_origin_size(Point::new(10.0, 10.0), Size::new(100.0, 50.0))
        );
        assert_eq!(scene.editing, Some(Selection::Node(created.id.clone())));
    }

    /// ⚠️ A frame is drawn *around* things, so it is born at the back. Draw
    /// order is z-order here: pushed last, its fill covered everything it was
    /// drawn around — and since iced paints a frame's text above all fills, the
    /// captions underneath stayed visible on top of the box covering them.
    /// ⚠️ Two reference cards overlapping read as garbled text: iced paints a
    /// frame's text above every fill, so the card underneath drew its words
    /// straight through the card on top. Occlusion tested only where the
    /// string *started*, so a card covered from the right — or from below —
    /// was never hidden at all.
    /// ⚠️ The renderer wraps at `max_width`, so a long caption is wider than
    /// one line's worth of characters in HEIGHT, not in width. Counting only
    /// the `\n`s returned a box one line tall for text that draws three, and
    /// the two lines below it were never considered covered.
    #[test]
    fn revealing_a_node_fits_it_instead_of_keeping_the_zoom() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 150.0, 56.0),
            node("2", Shape::Rect, 4000.0, 3000.0, 900.0, 700.0),
        ]);
        let pane = scene.pane_size();

        // zoomed far out, a small node comes closer
        scene.view.zoom = 0.25;
        scene.reveal("1");
        assert!(scene.view.zoom > 0.25, "zoom {}", scene.view.zoom);
        assert!(
            scene.view.zoom <= 2.0,
            "and it does not go past the ceiling"
        );
        // …and it is in the middle of the pane
        let centre = scene
            .view
            .to_screen(scene.canvas.node("1").unwrap().rect().center());
        assert!((centre.x - pane.width / 2.0).abs() < 0.5);
        assert!((centre.y - pane.height / 2.0).abs() < 0.5);

        // zoomed right in, a big node has to give way
        scene.view.zoom = 4.0;
        scene.reveal("2");
        assert!(scene.view.zoom < 1.0, "zoom {}", scene.view.zoom);
        let r = scene.canvas.node("2").unwrap().rect();
        assert!(
            r.width() * scene.view.zoom < pane.width && r.height() * scene.view.zoom < pane.height,
            "the whole node fits in the area"
        );
    }

    /// A quote card is a photo (ADR-003). A photo you cannot tell is wrong is
    /// worse than one that is right, so the card knows when its words have
    /// left the note.
    #[test]
    fn a_quote_knows_when_it_left_the_note() {
        let mut scene = scene_with(vec![]);
        let mut card = node("1", Shape::File, 0.0, 0.0, 240.0, 108.0);
        card.file = Some("note.md".into());
        card.excerpt = Some("a whole sentence".into());
        scene.canvas.nodes.push(card);

        let note = |text: &'static str| move |f: &str| (f == "note.md").then(|| text.to_string());

        // ⚠️ the very first check happens at revision ZERO — a board opened
        // with stale quotes already on it has seen no edits at all. Gated
        // against a zeroed counter this never runs, and the card lies from the
        // moment it appears. The assertion has to be the POSITIVE one, or
        // skipping the check passes it.
        scene.recheck_quotes(0, note("the note was rewritten"));
        assert!(scene.quote_is_stale("1"), "stale from the very first draw");

        scene.recheck_quotes(1, note("blah blah a whole sentence blah"));
        assert!(!scene.quote_is_stale("1"), "the slice is there");

        // and it comes back when the words do
        // and it goes again when the note changes once more
        scene.recheck_quotes(2, note("rewritten again"));
        assert!(scene.quote_is_stale("1"));

        // a note this build cannot find is a different complaint — the card
        // already says "missing note" itself, and a quote of nothing is not stale
        scene.recheck_quotes(3, |_: &str| None);
        assert!(!scene.quote_is_stale("1"));
    }

    /// The check is gated on the revisions so an idle app pays nothing — but a
    /// gate that never opens is the same as no check.
    #[test]
    fn the_quote_check_is_skipped_only_when_nothing_moved() {
        let mut scene = scene_with(vec![]);
        let mut card = node("1", Shape::File, 0.0, 0.0, 240.0, 108.0);
        card.file = Some("note.md".into());
        card.excerpt = Some("gone".into());
        scene.canvas.nodes.push(card);

        let mut calls = 0;
        let mut check = |scene: &mut Scene, rev: u64| {
            scene.recheck_quotes(rev, |_| {
                calls += 1;
                Some("something else".to_string())
            });
        };
        check(&mut scene, 7);
        check(&mut scene, 7);
        check(&mut scene, 8);
        assert_eq!(calls, 2, "the same revision costs nothing");

        // …and forgetting the verdict makes it run again on the same revision,
        // which is what a card that just changed needs
        scene.forget_quote_check();
        let mut again = 0;
        scene.recheck_quotes(8, |_| {
            again += 1;
            Some("something else".to_string())
        });
        assert_eq!(again, 1);
    }

    /// ⚠️ The heartbeat serialises the whole canvas whenever this is true, so
    /// "the editor is open" is the wrong question: an editor left open with
    /// nothing typed into it was a full board write every 600 ms.
    #[test]
    fn an_open_editor_with_nothing_typed_is_not_pending() {
        let mut scene = scene_with(vec![vault::Node::new(
            "1".into(),
            Shape::Rect,
            kurbo::Rect::from_origin_size(Point::ORIGIN, Size::new(100.0, 40.0)),
            "old",
        )]);
        scene.selected = vec![Selection::Node("1".into())];
        scene.handle(Event::EditSelected, Tool::Select);
        assert!(scene.editing.is_some(), "the editor did not open");

        assert!(
            !scene.has_pending_edit(),
            "opening the editor is not an edit"
        );

        scene.set_edit_text("new");
        assert!(scene.has_pending_edit(), "and typing is");

        // a flush settles it: what is in the buffer is what the model holds
        let _ = scene.save();
        assert!(!scene.has_pending_edit(), "the flush left it pending");
    }

    /// ⚠️ A section card is edited as `note.md#Heading` — one string over two
    /// fields. Handing that back through `set_caption` put the whole thing in
    /// `file`, so the card pointed at a note that exists nowhere while
    /// `subpath` still said `#Heading` too; and since an open editor used to
    /// count as pending, the heartbeat did it to you 600 ms after opening one,
    /// without a keystroke.
    #[test]
    fn editing_a_section_card_keeps_the_heading_out_of_the_file_name() {
        let mut card = node("1", Shape::File, 0.0, 0.0, 240.0, 108.0);
        card.file = Some("note.md".into());
        card.subpath = Some("#Heading".into());
        let mut scene = scene_with(vec![card]);
        scene.selected = vec![Selection::Node("1".into())];
        scene.handle(Event::EditSelected, Tool::Select);

        assert_eq!(scene.edit_text.trim(), "note.md#Heading");
        assert!(!scene.has_pending_edit(), "opening one is not editing it");

        // …and even when it IS edited, the two halves land in their own fields
        scene.set_edit_text("note.md#Other");
        assert!(scene.has_pending_edit());
        let _ = scene.save();
        let card = scene.canvas.node("1").unwrap();
        assert_eq!(card.file.as_deref(), Some("note.md"), "the file name");
        assert_eq!(card.section(), Some("Other"), "the heading");
        assert!(!scene.has_pending_edit(), "and it settles");
    }

    /// ⚠️ a copy of a card is not a note edit, so `paste_nodes` throws the
    /// verdict away by hand — see there.
    #[test]
    fn a_copy_of_a_stale_quote_is_stale_too() {
        let mut scene = scene_with(vec![]);
        let mut card = node("1", Shape::File, 0.0, 0.0, 240.0, 108.0);
        card.file = Some("note.md".into());
        card.excerpt = Some("gone".into());
        scene.canvas.nodes.push(card);
        let notes = |_: &str| Some("the note was rewritten".to_string());

        scene.recheck_quotes(1, notes);
        assert!(scene.quote_is_stale("1"));

        scene.selected = vec![Selection::Node("1".into())];
        assert!(scene.duplicate_selection());
        let copy = scene.canvas.nodes.last().unwrap().id.clone();

        // ⚠️ the SAME revision: no note moved, only the cards did
        scene.recheck_quotes(1, notes);
        assert!(
            scene.quote_is_stale(&copy),
            "the copy quotes the same words the note no longer has"
        );
    }

    /// The line under a quote card, and the one state that had no branch.
    #[test]
    fn two_cards_trading_places_is_still_a_change() {
        let mut scene = scene_with(vec![]);
        for (id, quote) in [("1", "alpha"), ("2", "beta")] {
            let mut card = node(id, Shape::File, 0.0, 0.0, 240.0, 108.0);
            card.file = Some(format!("{id}.md"));
            card.excerpt = Some(quote.to_string());
            scene.canvas.nodes.push(card);
        }
        let notes = |one: &'static str, two: &'static str| {
            move |f: &str| Some(if f == "1.md" { one } else { two }.to_string())
        };

        assert!(scene.recheck_quotes(1, notes("rewritten", "beta is here")));
        assert!(scene.quote_is_stale("1") && !scene.quote_is_stale("2"));

        assert!(
            scene.recheck_quotes(2, notes("alpha is back", "rewritten")),
            "the count is the same, the set is not"
        );
        assert!(!scene.quote_is_stale("1") && scene.quote_is_stale("2"));

        // and a closed gate is never a change
        assert!(!scene.recheck_quotes(2, notes("", "")));
    }

    #[test]
    fn a_frame_is_born_behind_everything_else() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 50.0, 50.0, 100.0, 40.0)]);
        scene.handle(Event::Pressed(Point::new(0.0, 0.0), 1), Tool::Frame);
        scene.handle(Event::Dragged(Point::new(400.0, 300.0), false), Tool::Frame);
        scene.handle(Event::Released(Point::new(400.0, 300.0)), Tool::Frame);

        assert_eq!(scene.canvas.nodes.len(), 2);
        assert_eq!(
            scene.canvas.nodes[0].shape(),
            Shape::Frame,
            "primeiro = atrás"
        );
        assert_eq!(scene.canvas.nodes[1].id, "1", "a caixa continua na frente");

        // an ordinary shape still lands on top
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Rect);
        scene.handle(Event::Released(Point::new(60.0, 40.0)), Tool::Rect);
        assert_eq!(scene.canvas.nodes.last().unwrap().shape(), Shape::Rect);
    }

    #[test]
    fn a_click_without_a_drag_uses_the_default_size() {
        let mut scene = scene_with(vec![]);
        scene.handle(Event::Pressed(Point::new(40.0, 40.0), 1), Tool::Rect);
        scene.handle(Event::Released(Point::new(42.0, 41.0)), Tool::Rect);

        let (w, h) = Shape::Rect.default_size();
        assert_eq!(
            scene.canvas.nodes[0].rect().size(),
            kurbo::Size::new(w as f64, h as f64)
        );
    }

    #[test]
    fn select_drags_the_node_under_the_cursor() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(70.0, 45.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(70.0, 45.0)), Tool::Select);

        assert_eq!(
            scene.canvas.nodes[0].rect().origin(),
            kurbo::Point::new(50.0, 25.0)
        );
        assert_eq!(scene.only_node(), Some("1"));
    }

    #[test]
    fn a_handle_grab_resizes_instead_of_moving() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        scene.selected = vec![Selection::Node("1".into())];
        // bottom-right handle
        scene.handle(Event::Pressed(Point::new(100.0, 40.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(160.0, 90.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(160.0, 90.0)), Tool::Select);

        assert_eq!(
            scene.canvas.nodes[0].rect().size(),
            kurbo::Size::new(160.0, 90.0)
        );
        assert_eq!(scene.canvas.nodes[0].rect().origin(), kurbo::Point::ORIGIN);
    }

    #[test]
    fn shift_resize_from_the_corner_scales_instead_of_stretching() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 50.0)]);
        scene.selected = vec![Selection::Node("1".into())];
        // bottom-right handle, ⇧ held: 100×50 → 200×100, top-left pinned
        scene.handle(Event::Pressed(Point::new(100.0, 50.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(200.0, 100.0), true), Tool::Select);
        scene.handle(Event::Released(Point::new(200.0, 100.0)), Tool::Select);
        let r = scene.canvas.nodes[0].rect();
        assert_eq!(r.origin(), kurbo::Point::ORIGIN);
        assert_eq!(r.size(), kurbo::Size::new(200.0, 100.0));
        assert_eq!(r.width() / r.height(), 2.0);
    }

    #[test]
    fn the_arrow_tool_connects_two_nodes_or_ends_loose() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        // a click that never left its node: nothing
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(30.0, 25.0)), Tool::Arrow);
        assert!(scene.canvas.edges.is_empty());

        // into empty space: a loose end there, a point node
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(600.0, 600.0)), Tool::Arrow);
        assert_eq!(scene.canvas.edges.len(), 1);
        let loose = scene.canvas.edges[0].to_node.clone();
        let point = scene.canvas.node(&loose).unwrap();
        assert_eq!(point.shape(), Shape::Point);
        assert_eq!(point.rect().center(), Point::new(600.0, 600.0));
        // deleting the arrow takes its loose end along
        scene.handle(Event::DeleteSelected, Tool::Select);
        assert!(scene.canvas.edges.is_empty());
        assert!(scene.canvas.node(&loose).is_none());

        // node to node: an edge, leaving by the sides that face each other —
        // and with no side written down, so moving a node still recomputes it
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Arrow);
        assert_eq!(scene.canvas.edges.len(), 1);
        let edge = &scene.canvas.edges[0];
        assert_eq!(edge.from_side, None);
        assert_eq!(edge.to_side, None);
        assert!(edge.has_arrow());

        let ends = |scene: &Scene| {
            let e = &scene.canvas.edges[0];
            let (_, from, _, to) = vault::edge_ends(
                e,
                scene.canvas.node("1").unwrap(),
                scene.canvas.node("2").unwrap(),
            );
            (from, to)
        };
        assert_eq!(ends(&scene), (vault::Side::Right, vault::Side::Left));

        // ⚠️ the fix the elbow would have made unbearable: drag the target
        // above the source and the arrow leaves by the top, not the right
        scene.canvas.node_mut("2").unwrap().y = -400.0;
        scene.canvas.node_mut("2").unwrap().x = 0.0;
        assert_eq!(ends(&scene), (vault::Side::Top, vault::Side::Bottom));
    }

    #[test]
    fn an_arrow_can_be_selected_and_deleted_without_its_nodes() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Arrow);
        let id = scene.canvas.edges[0].id.clone();

        // empty space between the two boxes, on the line: the arrow
        scene.selected.clear();
        scene.handle(Event::Pressed(Point::new(200.0, 20.0), 1), Tool::Select);
        assert!(scene.holds_edge(&id), "clicking the line selects the edge");

        // …and a hand's breadth off it is empty space again
        scene.handle(Event::Pressed(Point::new(200.0, 60.0), 1), Tool::Select);
        assert!(scene.selected.is_empty());

        scene.handle(Event::Pressed(Point::new(200.0, 20.0), 1), Tool::Select);
        scene.handle(Event::DeleteSelected, Tool::Select);
        assert!(scene.canvas.edges.is_empty(), "the arrow is gone");
        assert_eq!(scene.canvas.nodes.len(), 2, "and both boxes stayed");
    }

    #[test]
    fn the_line_tool_writes_an_edge_with_no_head() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Line);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Line);
        assert!(!scene.canvas.edges[0].has_arrow());
    }

    /// ⚠️ The bug the culling had: a curve's controls push it past the box of
    /// its two ends, so the old bounds would cut an arrow that crosses the pane.
    #[test]
    fn an_elbow_is_clicked_where_it_is_drawn() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 300.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 320.0)), Tool::Arrow);
        let id = scene.canvas.edges[0].id.clone();
        scene.canvas.edges[0].set_route(vault::Route::Elbow);

        let line = scene.edge_points(&scene.canvas.edges[0]).unwrap();
        let corner = line[2];
        scene.selected.clear();
        scene.handle(Event::Pressed(corner, 1), Tool::Select);
        assert!(scene.holds_edge(&id), "the corner is on the line");
    }

    /// Drag an end onto a face and it stays there; drag it onto the middle of
    /// the node and it goes back to following; drag it onto another node and
    /// the edge is rewired.
    /// ⚠️ The bug thickness detonated: the stroke was a constant in screen
    /// space with no `* z`, so zooming out shrank the diagram and left the
    /// arrow the same width — and the head was cravado at 8/4, following
    /// neither the zoom nor the line.
    /// ⚠️ A drawing is a legal JSON Canvas node or it is not a drawing: the
    /// spec enumerates node types, so it is a `text` node with a `draw` key —
    /// and the text has to be non-empty, or `skip_serializing_if` drops the
    /// one field the spec requires.
    #[test]
    fn the_pen_writes_a_legal_node_that_carries_its_stroke() {
        let mut scene = scene_with(vec![]);
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Pen);
        for x in [30.0, 60.0, 90.0] {
            scene.handle(Event::Dragged(Point::new(x, 50.0), false), Tool::Pen);
        }
        let out = scene.handle(Event::Released(Point::new(90.0, 50.0)), Tool::Pen);
        assert!(matches!(out, Outcome::Created { edit: false }));

        let node = &scene.canvas.nodes[0];
        assert_eq!(node.shape(), Shape::Draw);
        assert_eq!(node.kind, "text");
        assert!(
            !node.text.is_empty(),
            "an empty text drops the required key"
        );
        assert_eq!(
            node.rect(),
            kurbo::Rect::from_origin_size(Point::new(10.0, 10.0), Size::new(80.0, 40.0))
        );

        let json = serde_json::to_string(&scene.canvas).unwrap();
        assert!(json.contains("\"type\":\"text\""), "{json}");
        assert!(json.contains("\"text\":"), "{json}");
        assert!(json.contains("\"draw\":[["), "{json}");

        // the points ride in the box's own units, so moving the node carries
        // the stroke without touching a single one of them
        let before = scene.canvas.nodes[0].draw.clone();
        scene.selected = vec![Selection::Node(node.id.clone())];
        scene.handle(Event::Pressed(Point::new(50.0, 30.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(150.0, 30.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(150.0, 30.0)), Tool::Select);
        assert_eq!(scene.canvas.nodes[0].draw, before);
        assert_eq!(scene.canvas.nodes[0].x, 110.0);
    }

    /// The pen's thickness and colour are the hand's: picked once, every
    /// stroke after them is drawn with them — and a finished stroke is left
    /// unselected, whatever was selected before it, so the next one starts
    /// on a clean board.
    #[test]
    fn the_pen_keeps_its_thickness_and_colour_and_leaves_nothing_selected() {
        let mut scene = scene_with(vec![node("a", Shape::Rect, 300.0, 300.0, 40.0, 40.0)]);
        scene.selected = vec![Selection::Node("a".into())];
        scene.set_pen(2);
        scene.pen_color = Some("1".into());
        let draws: Vec<_> = scene.canvas.nodes.iter().map(|n| n.id.clone()).collect();
        for y in [20.0, 60.0] {
            scene.handle(Event::Pressed(Point::new(10.0, y), 1), Tool::Pen);
            scene.handle(Event::Dragged(Point::new(80.0, y + 10.0), false), Tool::Pen);
            scene.handle(Event::Released(Point::new(80.0, y + 10.0)), Tool::Pen);
            assert!(
                scene.selected.is_empty(),
                "the stroke at {y} was left selected"
            );
        }
        let strokes: Vec<_> = scene
            .canvas
            .nodes
            .iter()
            .filter(|n| !draws.contains(&n.id))
            .collect();
        assert_eq!(strokes.len(), 2);
        for stroke in strokes {
            assert_eq!(stroke.stroke(), vault::STROKES[2]);
            assert_eq!(stroke.color.as_deref(), Some("1"));
        }
        // setting the pen is not an edit to anything already on the board
        assert_eq!(scene.canvas.node("a").unwrap().stroke(), vault::STROKES[0]);
        assert_eq!(scene.canvas.node("a").unwrap().color, None);
    }

    /// A stroke drawn dead straight has no extent across, and dividing by that
    /// is how every point ends up on one edge of the box.
    #[test]
    fn a_flat_stroke_still_gets_a_box_it_fits_in() {
        let mut scene = scene_with(vec![]);
        scene.handle(Event::Pressed(Point::new(0.0, 100.0), 1), Tool::Pen);
        scene.handle(Event::Dragged(Point::new(200.0, 100.0), false), Tool::Pen);
        scene.handle(Event::Released(Point::new(200.0, 100.0)), Tool::Pen);

        let node = &scene.canvas.nodes[0];
        assert!(node.height >= 4.0);
        for point in node.draw.as_ref().unwrap() {
            assert!(point[1].is_finite(), "{point:?}");
            assert!((point[1] - 0.5).abs() < 0.01, "centred, not on the rim");
        }
    }

    #[test]
    fn an_edge_can_be_named_and_an_empty_name_leaves_the_file() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Arrow);
        let id = scene.canvas.edges[0].id.clone();

        // double-clicking the line opens the editor on the label
        scene.selected.clear();
        let out = scene.handle(Event::Pressed(Point::new(200.0, 20.0), 2), Tool::Select);
        assert!(matches!(out, Outcome::EditNode));
        assert_eq!(scene.editing, Some(Selection::Edge(id.clone())));

        scene.set_edit_text("depende de");
        scene.commit_edit();
        assert_eq!(
            scene.canvas.edge(&id).unwrap().label.as_deref(),
            Some("depende de")
        );

        // cleared, the key goes rather than sitting there empty
        scene.handle(Event::EditSelected, Tool::Select);
        scene.set_edit_text("   ");
        scene.commit_edit();
        assert_eq!(scene.canvas.edge(&id).unwrap().label, None);
    }

    /// "The middle" is not the middle of the chord once the route bends: on a
    /// curve it is halfway along the line, and on an elbow it is the middle of
    /// the longest leg, which is the only place with room for a word.
    #[test]
    fn an_edge_end_pins_to_a_face_unpins_in_the_middle_and_rewires() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
            node("3", Shape::Rect, 300.0, 300.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Arrow);
        let id = scene.canvas.edges[0].id.clone();
        assert!(scene.holds_edge(&id), "a fresh edge comes out selected");

        // grab the "from" end — it sits on node 1's right face — and drop it
        // on the same node's bottom edge
        scene.handle(Event::Pressed(Point::new(100.0, 20.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(50.0, 39.0), false), Tool::Select);
        let out = scene.handle(Event::Released(Point::new(50.0, 39.0)), Tool::Select);
        assert!(matches!(out, Outcome::Changed));
        assert_eq!(
            scene.canvas.edge(&id).unwrap().from_side.as_deref(),
            Some("bottom")
        );

        // the middle of the same node hands the side back to the automatic one
        scene.handle(Event::Pressed(Point::new(50.0, 40.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(50.0, 20.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(50.0, 20.0)), Tool::Select);
        assert_eq!(scene.canvas.edge(&id).unwrap().from_side, None);

        // and the "to" end dropped on a third node moves the edge onto it
        scene.handle(Event::Pressed(Point::new(300.0, 20.0), 1), Tool::Select);
        scene.handle(
            Event::Dragged(Point::new(350.0, 320.0), false),
            Tool::Select,
        );
        scene.handle(Event::Released(Point::new(350.0, 320.0)), Tool::Select);
        assert_eq!(scene.canvas.edge(&id).unwrap().to_node, "3");
    }

    #[test]
    fn a_rubber_band_over_empty_space_takes_what_it_encloses() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 150.0, 0.0, 100.0, 40.0),
            node("3", Shape::Rect, 600.0, 600.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(170.0, 20.0)), Tool::Arrow);
        let edge = scene.canvas.edges[0].id.clone();

        // an edge running node-to-node far away, to stay outside the band
        scene.canvas.edges.push(vault::Edge {
            id: "out".into(),
            from_node: "2".into(),
            to_node: "3".into(),
            ..vault::Edge::default()
        });

        scene.selected.clear();
        scene.handle(Event::Pressed(Point::new(-40.0, -40.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(300.0, 80.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(300.0, 80.0)), Tool::Select);

        assert!(scene.holds_node("1") && scene.holds_node("2"));
        assert!(!scene.holds_node("3"), "outside the band");
        assert!(scene.holds_edge(&edge), "both ends inside");
        assert!(!scene.holds_edge("out"), "half a link is not a link");

        // and the whole group travels on one drag
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(70.0, 45.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(70.0, 45.0)), Tool::Select);
        assert_eq!(
            scene.canvas.node("1").unwrap().rect().origin(),
            kurbo::Point::new(50.0, 25.0)
        );
        assert_eq!(
            scene.canvas.node("2").unwrap().rect().origin(),
            kurbo::Point::new(200.0, 25.0)
        );
        assert_eq!(
            scene.canvas.node("3").unwrap().rect().origin(),
            kurbo::Point::new(600.0, 600.0)
        );
    }

    /// A click that never moved is still a click: it clears the selection and
    /// takes nothing, however small the box would have been.
    #[test]
    fn an_empty_click_selects_nothing() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        scene.selected = vec![Selection::Node("1".into())];
        scene.handle(Event::Pressed(Point::new(400.0, 400.0), 1), Tool::Select);
        scene.handle(Event::Released(Point::new(400.0, 400.0)), Tool::Select);
        assert!(scene.selected.is_empty());
    }

    #[test]
    fn committing_a_bracket_mention_turns_the_node_into_a_doc_card() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        scene.selected = vec![Selection::Node("1".into())];
        scene.handle(Event::EditSelected, Tool::Select);
        scene.set_edit_text("[[auth-flow]]");
        scene.commit_edit();

        let node = &scene.canvas.nodes[0];
        assert_eq!(node.shape(), Shape::File);
        assert_eq!(node.file.as_deref(), Some("auth-flow.md"));
        assert!(node.width >= Shape::File.default_size().0);
    }

    #[test]
    fn double_clicking_a_doc_card_asks_the_app_to_open_the_note() {
        let mut scene = scene_with(vec![node("1", Shape::File, 0.0, 0.0, 240.0, 108.0)]);
        scene.canvas.nodes[0].file = Some("roadmap.md".into());
        let outcome = scene.handle(Event::Pressed(Point::new(20.0, 20.0), 2), Tool::Select);
        assert!(matches!(outcome, Outcome::OpenDoc(name, None) if name == "roadmap.md"));
    }

    /// A section card opens the note AT the section — `#Heading` and `#^id`
    /// alike, the way `[[note#…]]` lands.
    #[test]
    fn double_clicking_a_section_card_carries_the_section_along() {
        let mut scene = scene_with(vec![node("1", Shape::File, 0.0, 0.0, 240.0, 108.0)]);
        scene.canvas.nodes[0].file = Some("roadmap.md".into());
        scene.canvas.nodes[0].subpath = Some("#^gate-1".into());
        let outcome = scene.handle(Event::Pressed(Point::new(20.0, 20.0), 2), Tool::Select);
        assert!(
            matches!(outcome, Outcome::OpenDoc(name, Some(section)) if name == "roadmap.md" && section == "^gate-1")
        );
    }

    #[test]
    fn delete_removes_the_selection_but_not_while_typing() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        scene.selected = vec![Selection::Node("1".into())];
        scene.editing = Some(Selection::Node("1".into()));
        scene.handle(Event::DeleteSelected, Tool::Select);
        assert_eq!(scene.canvas.nodes.len(), 1);
        // ⌫ inside the editor is a keystroke — it must not quietly drop the
        // selection on its way past, or the field ends up over nothing
        assert_eq!(scene.only_node(), Some("1"));

        scene.editing = None;
        scene.handle(Event::DeleteSelected, Tool::Select);
        assert!(scene.canvas.nodes.is_empty());
    }

    /// The view's two halves have to be each other's inverse: a point put
    /// through `to_world` and back through `to_screen` is the point it was,
    /// at any pan and any zoom. Everything that hit-tests depends on it.
    ///
    /// ⚠️ this used to carry the doc comment of an n14 audit test about
    /// `String::truncate` on a doc card, which moved to `card.rs`.
    #[test]
    fn panning_and_zooming_keep_screen_and_world_in_step() {
        let mut scene = scene_with(vec![]);
        scene.handle(
            Event::Scrolled {
                delta: Vec2::new(40.0, 25.0),
                zoom: false,
                at: Point::ORIGIN,
            },
            Tool::Select,
        );
        assert_eq!(scene.view.to_world(Point::new(40.0, 25.0)), Point::ORIGIN);

        let anchor = Point::new(200.0, 150.0);
        let before = scene.view.to_world(anchor);
        scene.handle(
            Event::Scrolled {
                delta: Vec2::new(0.0, 2.0),
                zoom: true,
                at: anchor,
            },
            Tool::Select,
        );
        let after = scene.view.to_world(anchor);
        assert!(scene.view.zoom > 1.0);
        assert!((before.x - after.x).abs() < 0.01 && (before.y - after.y).abs() < 0.01);
    }

    /// The pointer resting on a node hovers it; leaving clears it; and a press
    /// — the start of a gesture — clears it too, so a rubber band crossing a
    /// node never dresses it as hovered.
    #[test]
    fn a_pointer_at_rest_hovers_the_node_under_it_and_a_gesture_does_not() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        let over = Point::new(20.0, 20.0);
        let off = Point::new(300.0, 300.0);

        scene.handle(Event::Dragged(over, false), Tool::Select);
        assert_eq!(scene.hovered(), Some("1"));
        // the same spot again is not worth a repaint
        assert!(matches!(
            scene.handle(Event::Dragged(over, false), Tool::Select),
            Outcome::Quiet
        ));
        scene.handle(Event::Dragged(off, false), Tool::Select);
        assert_eq!(scene.hovered(), None);
        // a drawing tool grabs nothing, so it promises nothing
        scene.handle(Event::Dragged(over, false), Tool::Rect);
        assert_eq!(scene.hovered(), None);

        scene.handle(Event::Dragged(over, false), Tool::Select);
        scene.handle(Event::Pressed(over, 1), Tool::Select);
        assert_eq!(scene.hovered(), None, "a press starts a gesture");
        scene.handle(Event::Dragged(Point::new(30.0, 25.0), false), Tool::Select);
        assert_eq!(scene.hovered(), None, "and the gesture keeps it clear");
        scene.handle(Event::Released(Point::new(30.0, 25.0)), Tool::Select);

        scene.handle(Event::Dragged(Point::new(40.0, 30.0), false), Tool::Select);
        assert_eq!(scene.hovered(), Some("1"));
        scene.handle(Event::Left, Tool::Select);
        assert_eq!(scene.hovered(), None);
    }

    /// `under` is the one answer the press and the cursor share: a corner is
    /// a handle, the dot above the top edge is the grip, the inside is the
    /// node — and none of the first two exist until the node is selected.
    #[test]
    fn under_tells_a_corner_from_the_grip_from_the_node() {
        let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
        let corner = Point::new(0.0, 0.0);
        let grip = grip_point(&scene.canvas.nodes[0], 1.0);
        assert_eq!(
            scene.under(corner),
            Under::Node("1".into()),
            "not selected: a corner is the node"
        );

        scene.handle(Event::Pressed(Point::new(50.0, 20.0), 1), Tool::Select);
        scene.handle(Event::Released(Point::new(50.0, 20.0)), Tool::Select);
        assert_eq!(scene.under(corner), Under::Handle((-1, -1)));
        assert_eq!(scene.under(Point::new(100.0, 20.0)), Under::Handle((1, 0)));
        assert_eq!(scene.under(grip), Under::Grip);
        assert_eq!(scene.under(Point::new(50.0, 20.0)), Under::Node("1".into()));
        assert_eq!(scene.under(Point::new(300.0, 300.0)), Under::Nothing);
    }

    /// A shape stays a shape and bare text stays text — whatever the bar is
    /// asked, whatever is typed into the box, and across a save. A box changes
    /// its outline; it never loses it, and words never grow one.
    #[test]
    fn a_shape_never_becomes_text_and_text_never_becomes_a_shape() {
        let path = scratch_board("shape-kind", r#"{"nodes":[],"edges":[]}"#);
        let mut scene = Scene::load(path.clone());
        scene.canvas.nodes = vec![
            node("box", Shape::Rect, 0.0, 0.0, 150.0, 56.0),
            node("words", Shape::Label, 0.0, 100.0, 120.0, 22.0),
        ];
        let shape = |scene: &Scene, id: &str| scene.canvas.node(id).unwrap().shape();

        scene.selected = vec![Selection::Node("box".into())];
        scene.set_shape(Shape::Label);
        assert_eq!(shape(&scene, "box"), Shape::Rect, "a shape went flat");
        scene.set_shape(Shape::Ellipse);
        assert_eq!(
            shape(&scene, "box"),
            Shape::Ellipse,
            "the outline still changes"
        );

        scene.selected = vec![Selection::Node("words".into())];
        scene.set_shape(Shape::Rect);
        assert_eq!(shape(&scene, "words"), Shape::Label, "text grew a box");

        // ⚠️ the bar shows the shape group for a box and a label together
        scene.selected = vec![
            Selection::Node("box".into()),
            Selection::Node("words".into()),
        ];
        scene.set_shape(Shape::Triangle);
        assert_eq!(shape(&scene, "box"), Shape::Triangle);
        assert_eq!(shape(&scene, "words"), Shape::Label, "text grew a box");
        scene.set_shape(Shape::Label);
        assert_eq!(shape(&scene, "box"), Shape::Triangle, "a shape went flat");

        // typing into the box keeps the box
        scene.selected.clear();
        scene.handle(Event::Pressed(Point::new(75.0, 28.0), 2), Tool::Select);
        assert!(scene.editing.is_some());
        scene.set_edit_text("a square");
        scene.commit_edit();
        assert_eq!(shape(&scene, "box"), Shape::Triangle);

        scene.save().unwrap();
        let back = Scene::load(path);
        assert_eq!(shape(&back, "box"), Shape::Triangle);
        assert_eq!(back.canvas.node("box").unwrap().caption(), "a square");
        assert_eq!(shape(&back, "words"), Shape::Label);
    }

    /// A pinch zooms about the point between the fingers: what is under them
    /// stays under them, spreading zooms in and closing zooms out.
    #[test]
    fn a_pinch_zooms_about_the_point_between_the_fingers() {
        let mut scene = scene_with(vec![]);
        scene.view = Viewport {
            offset: Vec2::new(30.0, -20.0),
            zoom: 1.0,
        };
        let at = Point::new(200.0, 150.0);
        let under = scene.view.to_world(at);

        scene.handle(Event::Pinched { delta: 0.2, at }, Tool::Select);
        assert!(scene.view.zoom > 1.0, "spreading zooms in");
        assert!((scene.view.to_screen(under) - at).hypot() < 1e-9);

        scene.handle(Event::Pinched { delta: -0.5, at }, Tool::Select);
        assert!(scene.view.zoom < 1.0, "closing zooms out");
        assert!((scene.view.to_screen(under) - at).hypot() < 1e-9);
    }

    /// ⌘C on a picture card copies the picture: `![](file)`, the same vault
    /// file, so ⌘V in a note draws it instead of linking to the card. Any
    /// other card is still a `[[canvas: …]]` mention of itself.
    #[test]
    fn a_copied_picture_card_is_the_picture_and_any_other_card_a_mention() {
        let mut shot = node("7", Shape::File, 0.0, 0.0, 80.0, 60.0);
        shot.file = Some("assets/pasted-3.png".into());
        let mut doc = node("8", Shape::File, 100.0, 0.0, 80.0, 60.0);
        doc.file = Some("notes/plan.md".into());
        let mut scene = scene_with(vec![shot, doc]);

        scene.selected = vec![Selection::Node("7".into())];
        assert_eq!(
            scene.copy_text().as_deref(),
            Some("![](assets/pasted-3.png)")
        );

        scene.selected.push(Selection::Node("8".into()));
        let name = scene.name();
        assert_eq!(
            scene.copy_text(),
            Some(format!(
                "![](assets/pasted-3.png)\n[[canvas: {name}#id:8|notes/plan.md]]"
            ))
        );
    }

    /// ⚠️ the regression this guards: the grip is drawn `GRIP_GAP` screen
    /// pixels over the node, and the press looked for it `GRIP_GAP` world
    /// units over — the same spot only at 100%. A press where it is drawn
    /// turns the node at any zoom.
    #[test]
    fn the_grip_is_pressed_where_it_is_drawn_at_any_zoom() {
        for zoom in [0.5, 1.0, 2.0, 3.5] {
            let mut scene = scene_with(vec![node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0)]);
            scene.view.zoom = zoom;
            scene.view.offset = Vec2::new(30.0, 70.0);
            let r = scene.view.screen_rect(scene.canvas.nodes[0].rect());
            scene.handle(Event::Pressed(r.center(), 1), Tool::Select);
            scene.handle(Event::Released(r.center()), Tool::Select);

            // the board's own drawing: the dot `GRIP_GAP` px over the top edge
            let drawn = Point::new(r.center().x, r.y0 - super::super::GRIP_GAP);
            scene.handle(Event::Pressed(drawn, 1), Tool::Select);
            assert!(scene.is_rotating(), "zoom {zoom}");
        }
    }

    #[test]
    fn a_tools_template_dresses_what_it_draws_and_leaves_the_board_alone() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        scene.selected = vec![Selection::Node("1".into())];
        let before = crate::history::canvas_json(&scene.canvas);

        // the bar, with the text tool in hand: colour, bold, alignment
        scene.with_next(Tool::Text, |s| {
            s.set_color(Some("4".into()));
            s.toggle_bold();
            s.set_align(vault::Align::Center);
        });
        // the arrow's: dashed, elbow, no head
        scene.with_next(Tool::Arrow, |s| {
            s.toggle_dash();
            s.set_route(vault::Route::Elbow);
            s.toggle_arrow(End::To);
        });
        assert_eq!(
            crate::history::canvas_json(&scene.canvas),
            before,
            "the template never stays"
        );
        assert_eq!(scene.selected, vec![Selection::Node("1".into())]);
        assert!(scene.next_styleable(Tool::Text).label);
        assert!(scene.next_styleable(Tool::Line).edges);
        assert_eq!(scene.next_styleable(Tool::Pen), Styleable::default());

        // a label from the text tool wears it
        scene.handle(Event::Pressed(Point::new(500.0, 500.0), 1), Tool::Text);
        scene.handle(Event::Released(Point::new(500.0, 500.0)), Tool::Text);
        let id = scene
            .editing
            .clone()
            .and_then(|e| e.node().map(str::to_string))
            .unwrap();
        let label = scene.canvas.node(&id).unwrap();
        assert_eq!(label.color.as_deref(), Some("4"));
        assert!(label.bold());
        assert_eq!(label.align(), Some(vault::Align::Center));

        // a box from the rect tool does not: that's the text tool's
        scene.handle(Event::Pressed(Point::new(700.0, 700.0), 1), Tool::Rect);
        scene.handle(Event::Released(Point::new(760.0, 740.0)), Tool::Rect);
        assert_eq!(scene.canvas.nodes.last().unwrap().color, None);

        // an arrow wears the arrow's, a line keeps its own
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Arrow);
        let edge = scene.canvas.edges.last().unwrap();
        assert!(edge.dashed() && !edge.has_arrow());
        assert_eq!(edge.route(), vault::Route::Elbow);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Line);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Line);
        let line = scene.canvas.edges.last().unwrap();
        assert!(!line.dashed() && !line.has_arrow());
    }

    #[test]
    fn text_never_typed_leaves_nothing_but_an_empty_box_stays() {
        let mut scene = scene_with(vec![]);
        // a text-tool click, then a press elsewhere commits nothing
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Text);
        scene.handle(Event::Released(Point::new(10.0, 10.0)), Tool::Text);
        scene.set_edit_text("   ");
        scene.commit_edit();
        assert!(scene.canvas.nodes.is_empty());
        assert!(scene.selected.is_empty());
        // ...and Esc on one does the same
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Text);
        scene.handle(Event::Released(Point::new(10.0, 10.0)), Tool::Text);
        scene.cancel_edit();
        assert!(scene.canvas.nodes.is_empty());
        // a box with no caption is still a box
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Rect);
        scene.handle(Event::Released(Point::new(80.0, 60.0)), Tool::Rect);
        scene.commit_edit();
        assert_eq!(scene.canvas.nodes.len(), 1);
    }

    #[test]
    fn a_label_grows_to_its_words_and_never_narrows() {
        let mut scene = scene_with(vec![node("t", Shape::Label, 0.0, 0.0, 120.0, 22.0)]);
        scene.canvas.node_mut("t").unwrap().text = "words".into();
        scene.begin_edit(Selection::Node("t".into()));
        scene.fit_label("t", Size::new(300.0, 44.0));
        let r = scene.canvas.node("t").unwrap().rect();
        assert_eq!((r.width(), r.height()), (300.0, 44.0));
        scene.fit_label("t", Size::new(40.0, 22.0));
        let r = scene.canvas.node("t").unwrap().rect();
        assert_eq!((r.width(), r.height()), (300.0, 22.0), "wide stays wide");
        // a box keeps the size it was drawn at
        scene
            .canvas
            .nodes
            .push(node("b", Shape::Rect, 0.0, 100.0, 150.0, 56.0));
        scene.fit_label("b", Size::new(400.0, 90.0));
        assert_eq!(scene.canvas.node("b").unwrap().rect().width(), 150.0);

        // written from nothing: as wide as the words, either way
        scene.handle(Event::Pressed(Point::new(500.0, 500.0), 1), Tool::Text);
        scene.handle(Event::Released(Point::new(500.0, 500.0)), Tool::Text);
        let id = scene
            .editing
            .clone()
            .and_then(|e| e.node().map(str::to_string))
            .unwrap();
        scene.fit_label(&id, Size::new(60.0, 22.0));
        assert_eq!(scene.canvas.node(&id).unwrap().rect().width(), 60.0);
        scene.fit_label(&id, Size::new(30.0, 22.0));
        assert_eq!(scene.canvas.node(&id).unwrap().rect().width(), 30.0);
    }

    #[test]
    fn a_press_elsewhere_keeps_what_was_typed() {
        let mut scene = scene_with(vec![]);
        scene.handle(Event::Pressed(Point::new(10.0, 10.0), 1), Tool::Text);
        scene.handle(Event::Released(Point::new(10.0, 10.0)), Tool::Text);
        // the shell hands every keystroke over as it lands
        scene.set_edit_text("typed, never ⏎");
        scene.handle(Event::Pressed(Point::new(400.0, 400.0), 1), Tool::Select);
        assert!(scene.editing.is_none());
        assert_eq!(scene.canvas.nodes.len(), 1);
        assert_eq!(scene.canvas.nodes[0].text, "typed, never ⏎");
    }

    #[test]
    fn the_face_is_set_on_words_only_kept_in_the_file_and_worn_from_the_hand() {
        let mut scene = scene_with(vec![
            node("t", Shape::Label, 0.0, 0.0, 120.0, 22.0),
            node("b", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        let mut card = node("c", Shape::Rect, 0.0, 300.0, 240.0, 108.0);
        card.kind = "file".into();
        card.file = Some("note.md".into());
        scene.canvas.nodes.push(card);
        scene.canvas.edges.push(vault::Edge {
            id: "e".into(),
            from_node: "t".into(),
            to_node: "b".into(),
            ..vault::Edge::default()
        });
        scene.selected = ["t", "b", "c"]
            .map(|id| Selection::Node(id.into()))
            .into_iter()
            .chain([Selection::Edge("e".into())])
            .collect();
        scene.set_mono(true);
        assert!(scene.canvas.node("t").unwrap().mono());
        assert!(scene.canvas.node("b").unwrap().mono());
        assert!(
            !scene.canvas.node("c").unwrap().mono(),
            "a card shows a note"
        );
        assert!(scene.canvas.edge("e").unwrap().mono());

        // in the file as `fontFamily`, and gone when it's the hand again
        let json = crate::history::canvas_json(&scene.canvas);
        assert!(
            json.contains(r#""fontFamily":"mono""#) || json.contains(r#""fontFamily": "mono""#)
        );
        scene.set_mono(false);
        assert!(!crate::history::canvas_json(&scene.canvas).contains("fontFamily"));

        // picked before drawing: the text tool's next label is born mono
        scene.selected.clear();
        scene.with_next(Tool::Text, |s| s.set_mono(true));
        scene.handle(Event::Pressed(Point::new(500.0, 500.0), 1), Tool::Text);
        scene.handle(Event::Released(Point::new(500.0, 500.0)), Tool::Text);
        assert!(scene.canvas.nodes.last().unwrap().mono());
    }

    #[test]
    fn an_end_comes_loose_lands_along_a_side_and_the_middle_bends() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("2", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        scene.handle(Event::Pressed(Point::new(20.0, 20.0), 1), Tool::Arrow);
        scene.handle(Event::Released(Point::new(320.0, 20.0)), Tool::Arrow);
        let edge = scene.canvas.edges[0].id.clone();
        let ends = |s: &Scene| {
            let line = s.edge_points(s.canvas.edge(&edge).unwrap()).unwrap();
            (line[0], *line.last().unwrap())
        };

        // the head's end dragged off into nothing: loose, where it was let go
        let (_, head) = ends(&scene);
        scene.handle(Event::Pressed(head, 1), Tool::Select);
        scene.handle(
            Event::Dragged(Point::new(300.0, 300.0), false),
            Tool::Select,
        );
        scene.handle(Event::Released(Point::new(300.0, 300.0)), Tool::Select);
        assert_eq!(ends(&scene).1, Point::new(300.0, 300.0));
        let point = scene.canvas.edge(&edge).unwrap().to_node.clone();
        assert_eq!(scene.canvas.node(&point).unwrap().shape(), Shape::Point);

        // ...and back onto node 2's top, a quarter of the way along: pinned
        // there, and the loose end it left is gone
        scene.handle(Event::Pressed(Point::new(300.0, 300.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(325.0, 2.0), false), Tool::Select);
        scene.handle(Event::Released(Point::new(325.0, 2.0)), Tool::Select);
        let e = scene.canvas.edge(&edge).unwrap();
        assert_eq!(
            (e.to_node.as_str(), e.to_side.as_deref()),
            ("2", Some("top"))
        );
        assert_eq!(e.to_at, Some(0.25));
        assert_eq!(ends(&scene).1, Point::new(325.0, 0.0));
        assert!(scene.canvas.node(&point).is_none());

        // the middle, dragged: the line passes through where it was let go
        let middle = scene.view.to_world(scene.bend_handle().unwrap());
        scene.handle(Event::Pressed(middle, 1), Tool::Select);
        scene.handle(
            Event::Dragged(Point::new(200.0, 150.0), false),
            Tool::Select,
        );
        scene.handle(Event::Released(Point::new(200.0, 150.0)), Tool::Select);
        assert!(scene.canvas.edge(&edge).unwrap().bend.is_some());
        let handle = scene.view.to_world(scene.bend_handle().unwrap());
        assert!((handle - Point::new(200.0, 150.0)).hypot() < 0.01);
        let line = scene
            .edge_points(scene.canvas.edge(&edge).unwrap())
            .unwrap();
        assert!(distance_to_path(&line, Point::new(200.0, 150.0)) < 2.0);

        // dragged back onto the straight middle: no bend in the file
        let (a, b) = ends(&scene);
        scene.handle(Event::Pressed(Point::new(200.0, 150.0), 1), Tool::Select);
        scene.handle(Event::Dragged(a.midpoint(b), false), Tool::Select);
        scene.handle(Event::Released(a.midpoint(b)), Tool::Select);
        assert_eq!(scene.canvas.edge(&edge).unwrap().bend, None);
    }

    #[test]
    fn ctrl_snaps_a_move_onto_the_nodes_around_it_and_lets_go_without() {
        let mut scene = scene_with(vec![
            node("a", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("b", Shape::Rect, 300.0, 200.0, 60.0, 40.0),
        ]);
        scene.align = true;
        // b dragged to land 5px off a's left edge, nothing near on y
        scene.handle(Event::Pressed(Point::new(330.0, 220.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(35.0, 203.0), false), Tool::Select);
        let r = scene.canvas.node("b").unwrap().rect();
        assert_eq!((r.x0, r.y0), (0.0, 183.0), "left onto left, y as put");
        let guides = scene.guides().to_vec();
        assert_eq!(guides.len(), 1);
        assert_eq!(guides[0].0.x, 0.0);
        assert_eq!(
            (guides[0].0.y, guides[0].1.y),
            (0.0, 223.0),
            "from a down to b"
        );
        scene.handle(Event::Released(Point::new(35.0, 203.0)), Tool::Select);
        assert!(scene.guides().is_empty());

        // centres line up too: b's middle onto a's middle, on both axes
        scene.handle(Event::Pressed(Point::new(30.0, 203.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(51.0, 21.0), false), Tool::Select);
        let r = scene.canvas.node("b").unwrap().rect();
        assert_eq!((r.center().x, r.center().y), (50.0, 20.0));
        scene.handle(Event::Released(Point::new(51.0, 21.0)), Tool::Select);

        // without Ctrl, it goes exactly where it's put
        scene.align = false;
        scene.handle(Event::Pressed(Point::new(50.0, 20.0), 1), Tool::Select);
        scene.handle(Event::Dragged(Point::new(53.0, 22.0), false), Tool::Select);
        let r = scene.canvas.node("b").unwrap().rect();
        assert_eq!((r.x0, r.y0), (23.0, 2.0));
        assert!(scene.guides().is_empty());
    }

    #[test]
    fn esc_takes_back_the_growth_and_a_cleared_label_takes_its_arrows() {
        let mut scene = scene_with(vec![
            node("t", Shape::Label, 0.0, 0.0, 120.0, 22.0),
            node("b", Shape::Rect, 300.0, 0.0, 100.0, 40.0),
        ]);
        scene.canvas.node_mut("t").unwrap().text = "hi".into();
        scene.canvas.edges.push(vault::Edge {
            id: "e".into(),
            from_node: "t".into(),
            to_node: "b".into(),
            ..vault::Edge::default()
        });
        // typed long, then Esc: the words and the box as they were
        scene.begin_edit(Selection::Node("t".into()));
        scene.set_edit_text("a much longer sentence than hi");
        scene.fit_label("t", Size::new(400.0, 44.0));
        scene.cancel_edit();
        let t = scene.canvas.node("t").unwrap();
        assert_eq!(
            (t.text.as_str(), t.rect().width(), t.rect().height()),
            ("hi", 120.0, 22.0)
        );

        // cleared and committed: the label goes, and its arrow with it
        scene.begin_edit(Selection::Node("t".into()));
        scene.set_edit_text("");
        scene.commit_edit();
        assert!(scene.canvas.node("t").is_none());
        assert!(scene.canvas.edges.is_empty());
    }

    #[test]
    fn a_pasted_loose_end_is_left_behind_and_a_one_turn_elbow_has_no_bend() {
        let mut scene = scene_with(vec![
            node("1", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            vault::Node::point("p".into(), Point::new(400.0, 300.0)),
        ]);
        let copied = scene.canvas.nodes.clone();
        scene.paste_nodes(Point::new(0.0, 500.0), &copied);
        let points = scene
            .canvas
            .nodes
            .iter()
            .filter(|n| n.shape() == Shape::Point)
            .count();
        assert_eq!(points, 1, "only the original");

        // right side out, top side in: one corner, nothing in the middle to bend
        let mut scene = scene_with(vec![
            node("a", Shape::Rect, 0.0, 0.0, 100.0, 40.0),
            node("b", Shape::Rect, 300.0, 200.0, 100.0, 40.0),
        ]);
        scene.canvas.edges.push(vault::Edge {
            id: "e".into(),
            from_node: "a".into(),
            from_side: Some("right".into()),
            to_node: "b".into(),
            to_side: Some("top".into()),
            route: Some("elbow".into()),
            ..vault::Edge::default()
        });
        scene.selected = vec![Selection::Edge("e".into())];
        assert_eq!(scene.bend_handle(), None);
        // across both ends: a middle leg, so a handle
        scene.canvas.edge_mut("e").unwrap().to_side = Some("left".into());
        assert!(scene.bend_handle().is_some());
    }
}
