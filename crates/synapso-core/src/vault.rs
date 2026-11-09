//! The vault: a plain folder of `.md` notes and `.canvas` diagrams.
//!
//! Nothing proprietary — notes are readable markdown and canvases are
//! [JSON Canvas](https://jsoncanvas.org) 1.0, so Obsidian can open the same
//! folder. Shapes are the one extension: a `"shape"` key on a text node.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The vault on screen: `$SYNAPSO_VAULT` if set, else the folder the last
/// ⌘O picked, else `~/Synapso`. Created on first use.
///
/// Asking here instead of carrying a `PathBuf` is what makes ⌘O possible:
/// [`set_root`] moves the whole app at once, with nothing threaded through it.
/// It is called inside list rebuilds, so it stays two `stat`s at worst.
///
/// ⚠️ what [`set_root`] chose is the ONLY thing remembered between calls. The
/// env var is re-read every time on purpose: the shell's test harness
/// points `SYNAPSO_VAULT` at a fresh folder per test and builds a `State`, and
/// a cache filled on the first call would hand every later test the first
/// test's vault.
pub fn root() -> PathBuf {
    if let Some(path) = PICKED.lock().ok().and_then(|c| c.clone()) {
        return path;
    }
    let path = std::env::var_os("SYNAPSO_VAULT")
        .map(PathBuf::from)
        .or_else(|| remembered().filter(|p| p.is_dir()))
        .unwrap_or_else(|| home().join("Synapso"));
    if !path.is_dir() {
        let _ = fs::create_dir_all(&path);
    }
    path
}

/// The folder ⌘O (or `synapso <folder>`) picked in THIS session, which
/// outranks everything else.
/// `None` until someone picks one.
static PICKED: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Point the app at another folder, and remember it for the next launch.
///
/// ⚠️ the caller flushes the pending edit BEFORE this — ADR-001's rule, one
/// step further out: the autosave timer holds a path, and a re-root that
/// happened first would let a 600 ms beat land in the folder the words never
/// came from.
pub fn set_root(path: &Path) -> io::Result<PathBuf> {
    let path = path.to_path_buf();
    fs::create_dir_all(&path)?;
    remember(&path);
    if let Ok(mut picked) = PICKED.lock() {
        *picked = Some(path.clone());
    }
    Ok(path)
}

/// The user-level config folder — one file, holding one path. ⚠️ NOT in the
/// vault: which vault to open cannot live inside the vault it names.
///
/// ⚠️ `$SYNAPSO_CONFIG` overrides it. Without that hook a test of "the app
/// remembers the folder you picked" would write over the real one on the
/// machine running the test.
fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SYNAPSO_CONFIG") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    {
        home().join("Library/Application Support/synapso")
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".config"))
            .join("synapso")
    }
}

/// The folder ⌘O picked last, if any. A plain line of text: nothing else is
/// kept at this level, and a JSON file for one string would be ceremony.
pub fn remembered() -> Option<PathBuf> {
    let text = fs::read_to_string(config_dir().join("vault")).ok()?;
    let line = text.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// How big the interface is drawn, as a multiplier of the screen's own scale.
///
/// ⚠️ this exists because a 1x screen is not a 2x screen with fewer pixels: the
/// design is drawn in 10 px chrome, which a retina panel renders in 20 device
/// pixels and a plain one in 10. Zooming the whole interface keeps every
/// proportion the design set and buys the pixels back, which picking bigger
/// tokens one by one does not.
///
/// Beside the vault pointer, and for the same reason: it cannot live inside a
/// vault, because it is not about any one vault.
pub fn zoom() -> Option<f64> {
    let text = fs::read_to_string(config_dir().join("zoom")).ok()?;
    text.trim().parse().ok().filter(|z: &f64| z.is_finite())
}

/// Remember it for the next launch. Silent on failure, as [`remember`] is.
pub fn remember_zoom(zoom: f64) {
    let dir = config_dir();
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(dir.join("zoom"), format!("{zoom}"));
    }
}

/// A config folder that will not write is not an error worth surfacing: the
/// vault still opened, it just will not be the default next time.
fn remember(path: &Path) {
    let dir = config_dir();
    if fs::create_dir_all(&dir).is_ok() {
        let _ = fs::write(dir.join("vault"), path.to_string_lossy().as_bytes());
    }
}

/// `$HOME`, or `%USERPROFILE%` on Windows, where `HOME` is usually unset.
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(PathBuf::new, PathBuf::from)
}

/// Files with the given extension, sorted by name.
pub fn list(root: &Path, ext: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == ext))
        .collect();
    paths.sort();
    paths
}

/// File name without extension — the vault's identity for a document: what
/// prefs, dedup and rename key on. ⚠️ not what the design shows — the chrome
/// (tabs, breadcrumbs, status bar) prints the full file name, [`display_name`].
pub fn name(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into()
}

/// File name WITH extension — what the design prints everywhere a document is
/// named in the chrome (rail tabs, breadcrumbs, status bar). Doc cards on the
/// canvas already carry the full `node.file` string, so this just brings the
/// rest of the chrome in line with them.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into()
}

/// A caption as it has to read inside `[[ ]]`: one line, single-spaced.
///
/// ⚠️ Canvas text is deliberately multi-line, a mention cannot be. `inline`
/// reads mentions one line at a time, so a newline between the brackets
/// leaves `[[` on one line and `]]` on the next — the link is dead the moment
/// the note is reopened. Both halves of the mention system fold through here,
/// so what is written is what is later matched.
pub fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `stem.ext` in `root`, with `-2`, `-3`… appended until it is free.
pub fn unique_path(root: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem = sanitize(stem);
    let mut path = root.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while path.exists() {
        path = root.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    path
}

/// Latin letters carrying marks, flattened — enough for pt-BR and the
/// languages next door. Anything else the slug turns into a dash.
const FOLD: [(&str, char); 8] = [
    ("àáâãäåāăą", 'a'),
    ("èéêëēĕėęě", 'e'),
    ("ìíîïĩīĭįı", 'i'),
    ("òóôõöøōŏő", 'o'),
    ("ùúûüũūŭůű", 'u'),
    ("çćĉċč", 'c'),
    ("ñńņňŉ", 'n'),
    ("ýÿŷ", 'y'),
];

/// A file name out of whatever was typed: lowercase, marks folded, one dash
/// between words. `a VERY LOUD file with maçã` → `a-very-loud-file-with-maca`.
/// The slug is also what the UI shows, so a note is called what its file is.
pub fn sanitize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.trim().chars() {
        let c = c.to_lowercase().next().unwrap_or(c);
        let c = FOLD
            .iter()
            .find(|(marked, _)| marked.contains(c))
            .map_or(c, |(_, plain)| *plain);
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let cleaned = out.trim_matches('-');
    if cleaned.is_empty() {
        return "untitled".into();
    }
    // ⚠️ Windows still refuses these, extension or not: `con.md` cannot be
    // created. They are ordinary words, so someone will type one.
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if RESERVED.contains(&cleaned) {
        return format!("{cleaned}-note");
    }
    cleaned.to_string()
}

/// Write via a temp file + rename, so a crash mid-write cannot truncate a note.
///
/// The temp name carries this process's pid, so two Synapso instances saving
/// the same file never share (and clobber) one temp file; `sync_all` forces
/// the bytes to disk before the rename, so a crash right after cannot leave
/// a truncated file in place.
pub fn write(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let mut f = fs::File::create(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f); // Windows refuses to rename a file whose handle is still open
    fs::rename(&tmp, path)
}

/// ⚠️ Fallible on purpose. A permission error or half-written file used to
/// come back as an empty string, which the editor showed as an empty note —
/// and the next autosave wrote that emptiness over the real one.
///
/// ⚠️ CRLF is folded to LF here, once, at the edge.
///
/// The block parser already tolerates either (`str::lines` eats the `\r`) — but
/// `split_front` matches the literal `"---\n"`, so a note written on Windows
/// had its frontmatter read as a horizontal rule plus paragraphs and saved
/// back that way. Windows is a target platform, so this is data loss, not a
/// preference.
///
/// normalising at the one place a note enters the app beats teaching
/// every reader about `\r`. The cost is that a CRLF file comes back LF — but
/// only if it is edited, and the body was already being written LF.
pub fn read(path: &Path) -> io::Result<String> {
    Ok(fs::read_to_string(path)?.replace("\r\n", "\n"))
}

/// What a file looked like when we last read or wrote it. `None` means it was
/// not there — which is a state worth telling apart from any other.
pub type Stamp = Option<(std::time::SystemTime, u64)>;

pub fn stamp(path: &Path) -> Stamp {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Write only if the file on disk is still the one we loaded.
///
/// mtime + length, not a content hash. Cheap, and it catches the
/// case that actually happens — Obsidian (or a second Synapso) saved the same
/// note while it was open here. Returns the new stamp to carry forward.
pub fn write_checked(path: &Path, text: &str, expected: Stamp) -> io::Result<Stamp> {
    if stamp(path) != expected {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "changed on disk",
        ));
    }
    write(path, text)?;
    Ok(stamp(path))
}

/// Deleted files go here instead of away. Hidden, and inside the vault, so a
/// deletion is one `mv` from being undone by hand.
pub const TRASH: &str = ".trash";

pub fn trash(root: &Path, path: &Path) -> io::Result<PathBuf> {
    let dir = root.join(TRASH);
    fs::create_dir_all(&dir)?;
    let ext = path
        .extension()
        .map_or_else(String::new, |e| e.to_string_lossy().into_owned());
    let to = unique_path(&dir, &name(path), &ext);
    fs::rename(path, &to)?;
    Ok(to)
}

// --- preferences ---------------------------------------------------------

/// A note's colour: one of the theme's four, or one someone picked.
///
/// Untagged on purpose — a preset stays the number it has always been in
/// `.synapso.json`, so a vault written by an older build still reads. The
/// preset is an *index*, not a colour, so it follows the theme; a picked
/// colour is exactly what was picked, in both themes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocColor {
    /// An explicit neutral choice, distinct from an automatic preset.
    Neutral,
    Preset(usize),
    /// `#RRGGBB` — a string, so the sidecar stays as readable as the notes are.
    Custom(String),
}

impl Default for DocColor {
    fn default() -> Self {
        Self::Preset(0)
    }
}

impl DocColor {
    pub fn resolve(&self, palette: [peniko::Color; 4]) -> peniko::Color {
        match self {
            Self::Neutral => peniko::Color::from_rgb8(128, 128, 128),
            Self::Preset(i) => palette[i % palette.len()],
            // a hand-edited sidecar can say anything; fall back, never panic
            Self::Custom(hex) => crate::color::from_hex(hex).unwrap_or(palette[0]),
        }
    }

    pub fn custom(color: peniko::Color) -> Self {
        Self::Custom(crate::color::to_hex(color))
    }
}

/// The one thing a folder of markdown cannot hold: which color a note was
/// given. Kept in a hidden sidecar so the notes stay clean (frontmatter would
/// show up inside the editor).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    colors: std::collections::BTreeMap<String, DocColor>,
    /// Documents taken off the tab rail or out of the board switcher, one key
    /// per file (`auth/plan`, `auth/plan.canvas`). *Closed*, not *open*, so a
    /// vault with no sidecar — or one written by an older build — starts with
    /// everything open instead of nothing.
    #[serde(
        default,
        rename = "closed_files",
        skip_serializing_if = "std::collections::BTreeSet::is_empty"
    )]
    closed: std::collections::BTreeSet<String>,
    /// Documents in the order they were last opened, most recent first, by
    /// the same keys — what the picker's `recent` scope is sorted by
    /// (`design/Workspace.dc.html:345`). Relative to the vault, so it survives
    /// the vault being moved; capped, so a long session cannot grow the
    /// sidecar without limit.
    #[serde(
        default,
        rename = "recent_files",
        skip_serializing_if = "Vec::is_empty"
    )]
    recent: Vec<String>,
    /// ⚠️ the two lists as older builds kept them, by bare name, so `plan.md`
    /// and `plan.canvas` (or `a/plan.canvas` and `b/plan.canvas`) shared one
    /// entry. Read once by [`Prefs::migrate_names`], which empties them.
    #[serde(
        default,
        rename = "closed",
        skip_serializing_if = "std::collections::BTreeSet::is_empty"
    )]
    named_closed: std::collections::BTreeSet<String>,
    #[serde(default, rename = "recent", skip_serializing_if = "Vec::is_empty")]
    named_recent: Vec<String>,
}

/// How many documents `recent` remembers. The picker shows six.
const RECENT_KEPT: usize = 50;

impl Prefs {
    fn file(root: &Path) -> PathBuf {
        root.join(".synapso.json")
    }

    /// a missing or unreadable sidecar is four default colors, not an
    /// error — nothing a person wrote lives in here.
    pub fn load(root: &Path) -> Self {
        read(&Self::file(root))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Persist the sidecar. Callers may treat a lost colour as cosmetic, but a
    /// tab transition must surface this error or the UI claims a choice that a
    /// restart immediately forgets.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        write(&Self::file(root), &json)
    }

    /// The chosen color, or `fallback` (the position-derived one) if never set.
    pub fn color(&self, note: &str, fallback: usize) -> DocColor {
        self.colors
            .get(note)
            .cloned()
            .unwrap_or(DocColor::Preset(fallback))
    }

    pub fn canvas_color(&self, name: &str) -> DocColor {
        self.colors
            .get(&format!("{name}.canvas"))
            .cloned()
            .unwrap_or(DocColor::Neutral)
    }

    pub fn set_color(&mut self, note: &str, color: DocColor) {
        self.colors.insert(note.to_string(), color);
    }

    pub fn is_open(&self, note: &str) -> bool {
        !self.closed.contains(note)
    }

    pub fn set_open(&mut self, note: &str, open: bool) {
        if open {
            self.closed.remove(note);
        } else {
            self.closed.insert(note.to_string());
        }
    }

    /// The documents last opened, most recent first.
    pub fn recent(&self) -> &[String] {
        &self.recent
    }

    /// This one was just opened: to the front, once.
    pub fn touch(&mut self, note: &str) {
        self.recent.retain(|seen| seen != note);
        self.recent.insert(0, note.to_string());
        self.recent.truncate(RECENT_KEPT);
    }

    /// Move the lists an older build kept by bare name onto per-file keys.
    /// `docs` is every document as (bare name, key); a name stood for every
    /// document that has it, so each of them takes the entry. True when
    /// anything moved, for the caller to save.
    pub fn migrate_names(&mut self, docs: &[(&str, &str)]) -> bool {
        if self.named_closed.is_empty() && self.named_recent.is_empty() {
            return false;
        }
        let closed = std::mem::take(&mut self.named_closed);
        for (name, key) in docs {
            if closed.contains(*name) {
                self.closed.insert(key.to_string());
            }
        }
        // what this build already wrote is newer: it stays in front
        for name in std::mem::take(&mut self.named_recent) {
            for (_, key) in docs.iter().filter(|(n, _)| *n == name) {
                if !self.recent.iter().any(|seen| seen == key) {
                    self.recent.push(key.to_string());
                }
            }
        }
        self.recent.truncate(RECENT_KEPT);
        true
    }

    /// `from` and `to` are keys; a folder's (ending in `/`) takes every key
    /// under it along.
    pub fn rename(&mut self, from: &str, to: &str) {
        let moved = |key: &str| -> Option<String> {
            if key == from {
                Some(to.to_string())
            } else if from.ends_with('/') {
                key.strip_prefix(from).map(|rest| format!("{to}{rest}"))
            } else {
                None
            }
        };
        self.colors = std::mem::take(&mut self.colors)
            .into_iter()
            .map(|(key, color)| (moved(&key).unwrap_or(key), color))
            .collect();
        self.closed = std::mem::take(&mut self.closed)
            .into_iter()
            .map(|key| moved(&key).unwrap_or(key))
            .collect();
        // ⚠️ the renamed note keeps its PLACE in the list. Dropping it and
        // re-touching would shove it to the front, which is a lie: renaming a
        // note is not opening it.
        for seen in &mut self.recent {
            if let Some(key) = moved(seen) {
                *seen = key;
            }
        }
    }

    pub fn forget(&mut self, note: &str) {
        self.colors.remove(note);
        self.closed.remove(note);
        self.recent.retain(|seen| seen != note);
    }
}

// --- JSON Canvas ---------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Ellipse,
    Triangle,
    /// Bare label, no box.
    Label,
    /// A `group` node — the design's frame.
    Frame,
    /// A `file` node — the doc-reference card.
    File,
    /// A freehand stroke. Still a `text` node in the file: the spec's node
    /// types are enumerated, so `"type": "draw"` would be invalid rather than
    /// an extension.
    Draw,
    /// Where a loose arrow end rests: a node with no body, drawn as nothing.
    /// JSON Canvas has no edge without a node at each end, so a loose end
    /// is a node of its own (`"shape": "point"`); another reader shows a
    /// tiny empty card there.
    Point,
}

impl Shape {
    pub fn as_str(self) -> &'static str {
        match self {
            Shape::Rect => "rect",
            Shape::Ellipse => "ellipse",
            Shape::Triangle => "triangle",
            Shape::Label => "label",
            Shape::Frame => "frame",
            Shape::File => "file",
            Shape::Draw => "draw",
            Shape::Point => "point",
        }
    }

    /// Where the caption sits when the file does not say — ADR-004 §D2: what
    /// this build already did, so no existing canvas changes appearance.
    pub fn default_align(self) -> Align {
        match self {
            Shape::Label | Shape::Frame => Align::Left,
            _ => Align::Center,
        }
    }

    /// Default size when the tool is clicked instead of dragged.
    pub fn default_size(self) -> (f32, f32) {
        match self {
            Shape::Ellipse => (170.0, 64.0),
            Shape::Triangle => (90.0, 70.0),
            Shape::Label => (120.0, 22.0),
            Shape::Frame => (320.0, 220.0),
            Shape::File => (240.0, 108.0),
            Shape::Point => (POINT_SIZE, POINT_SIZE),
            // a drawing is only ever the size of what was drawn
            Shape::Draw | Shape::Rect => (150.0, 56.0),
        }
    }
}

/// Where a node's caption sits inside its own box.
///
/// ⚠️ Absent means the shape's own default (`Shape::default_align`), NOT
/// `Left` — ADR-004 §D2. `shape` is our extension, so there is no "the
/// alignment another reader gives it" to converge on, and every canvas that
/// already exists has to keep looking the way it looks today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub fn as_str(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Align::Left),
            "center" => Some(Align::Center),
            "right" => Some(Align::Right),
            _ => None,
        }
    }
}

/// The spec's geometry is whole pixels. We think in `f32` because the board
/// does, and round on the way out so other readers get what they expect.
mod px {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i64(value.round() as i64)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
        // a coordinate outside this range casts to f32::INFINITY, and the
        // next save would rewrite the file as i64::MAX
        Ok(f64::deserialize(d)?.clamp(-1e7, 1e7) as f32)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    /// JSON Canvas node type: `text` | `file` | `group` | `link`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(with = "px")]
    pub x: f32,
    #[serde(with = "px")]
    pub y: f32,
    #[serde(with = "px")]
    pub width: f32,
    #[serde(with = "px")]
    pub height: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// JSON Canvas: `#heading` inside the referenced file — a section card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    /// Synapso extension: the slice of the note this card quotes, for a
    /// selection that is not a whole heading. Other readers see a file node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Synapso extension: which shape a `text` node is drawn as. Other JSON
    /// Canvas readers ignore it and show a plain text node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    /// Synapso extension: degrees clockwise about the node's centre. Absent
    /// means upright, which is what every other JSON Canvas reader assumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    /// Synapso extension: the text size a node was scaled to, in px. Absent
    /// means the default for its shape.
    #[serde(rename = "fontSize", default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    /// Synapso extension: a bold canvas caption. Absent is regular text.
    #[serde(
        rename = "fontWeight",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub font_weight: Option<serde_json::Value>,
    /// Synapso extension: `left` | `center` | `right` for the caption. Absent
    /// means the shape's own default — see [`Align`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    /// Synapso extension: `mono` writes the caption in the app's face
    /// (JetBrains Mono). Absent is the board's hand (Excalifont), the default
    /// for what is written on a canvas.
    #[serde(
        rename = "fontFamily",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<String>,
    /// ⚠️ Synapso extension: a freehand stroke, as points in `0..=1` of the
    /// node's own box — so moving and resizing the node carry the drawing
    /// with it for free.
    ///
    /// this is the ONE extension whose loss destroys content rather
    /// than appearance. A lost `shape` is still a node with its text; a lost
    /// `route` is still an arrow; a lost `draw` is an **empty box where the
    /// drawing was**. The ceiling: the stroke lives in a key only Synapso
    /// promises to keep, so a round-trip through another editor can hand back
    /// a blank node. The upgrade path is the spec growing a path node — there
    /// is nowhere better to put it today, and a sidecar file would lose the
    /// drawing *silently* when the `.canvas` is copied on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw: Option<Vec<[f32; 2]>>,
    /// Synapso extension: outline thickness in px, for the drawing and for
    /// any shape's border. Same key as an edge's.
    #[serde(
        rename = "strokeWidth",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub stroke_width: Option<f32>,
    /// Spec fields Synapso does not draw yet: a `link` node's address and a
    /// group's background image. Typed so they survive a save untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(
        rename = "backgroundStyle",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub background_style: Option<String>,
    /// ⚠️ Everything this build has never heard of — a newer JSON Canvas, or
    /// another editor's extension. Kept verbatim, because the alternative is
    /// deleting someone else's data every time we save.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Node {
    pub fn new(id: String, shape: Shape, rect: kurbo::Rect, text: &str) -> Self {
        let (kind, file, label) = match shape {
            Shape::Frame => ("group", None, Some(text.to_string())),
            Shape::File => ("file", Some(text.to_string()), None),
            _ => ("text", None, None),
        };
        Self {
            id,
            kind: kind.into(),
            x: rect.x0 as f32,
            y: rect.y0 as f32,
            width: rect.width() as f32,
            height: rect.height() as f32,
            text: if kind == "text" {
                text.into()
            } else {
                String::new()
            },
            file,
            subpath: None,
            excerpt: None,
            label,
            color: None,
            // absent already means Rect per `shape()` — writing it anyway on
            // every default text node is pure file noise
            shape: (kind == "text" && shape != Shape::Rect).then(|| shape.as_str().to_string()),
            rotation: None,
            font_size: None,
            font_weight: None,
            align: None,
            font_family: None,
            draw: None,
            stroke_width: None,
            url: None,
            background: None,
            background_style: None,
            extra: serde_json::Map::new(),
        }
    }

    /// The heading a `file` node points into, without the `#`.
    pub fn section(&self) -> Option<&str> {
        self.subpath.as_deref().map(|s| s.trim_start_matches('#'))
    }

    /// What the inline editor starts with. A section card has to show its
    /// `#heading` too, or committing the edit would silently drop it.
    pub fn edit_label(&self) -> String {
        match self.section() {
            Some(section) => format!("{}#{section}", self.caption()),
            None => self.caption().to_string(),
        }
    }

    /// A freehand stroke in a box the size of what was drawn.
    ///
    /// ⚠️ The text is not decoration. `Node.text` is skipped when empty and
    /// `text` is *required* on a `text` node, so an unnamed drawing would
    /// serialise into an illegal node. It is also what another reader shows
    /// in place of the stroke it cannot draw.
    /// A loose arrow end at `at` (`Shape::Point`). Its text is a dot for the
    /// same reason a drawing's isn't empty: `text` is required on a text
    /// node, and a dot is what another reader shows in its place.
    pub fn point(id: String, at: kurbo::Point) -> Self {
        let half = f64::from(POINT_SIZE) / 2.0;
        let rect = kurbo::Rect::new(at.x - half, at.y - half, at.x + half, at.y + half);
        Self::new(id, Shape::Point, rect, "·")
    }

    pub fn drawing(id: String, rect: kurbo::Rect, points: Vec<[f32; 2]>) -> Self {
        Self {
            draw: Some(points),
            shape: None,
            ..Self::new(id, Shape::Rect, rect, "(drawing)")
        }
    }

    pub fn shape(&self) -> Shape {
        if self.draw.is_some() {
            return Shape::Draw;
        }
        match self.kind.as_str() {
            "group" => Shape::Frame,
            "file" => Shape::File,
            _ => match self.shape.as_deref() {
                Some("ellipse") => Shape::Ellipse,
                Some("triangle") => Shape::Triangle,
                Some("label") => Shape::Label,
                Some("point") => Shape::Point,
                _ => Shape::Rect,
            },
        }
    }

    pub fn rect(&self) -> kurbo::Rect {
        kurbo::Rect::from_origin_size(
            kurbo::Point::new(self.x as f64, self.y as f64),
            kurbo::Size::new(self.width as f64, self.height as f64),
        )
    }

    pub fn set_rect(&mut self, r: kurbo::Rect) {
        self.x = r.x0 as f32;
        self.y = r.y0 as f32;
        self.width = r.width() as f32;
        self.height = r.height() as f32;
    }

    /// The size the node's text is drawn at — its own, or the default given.
    pub fn text_size(&self, default: f32) -> f32 {
        self.font_size.unwrap_or(default).clamp(6.0, 200.0)
    }

    pub fn set_text_size(&mut self, size: f32) {
        self.font_size = Some(size.clamp(6.0, 200.0));
    }

    pub fn bold(&self) -> bool {
        self.font_weight
            .as_ref()
            .is_some_and(|weight| match weight {
                serde_json::Value::String(weight) => {
                    weight.eq_ignore_ascii_case("bold")
                        || weight.parse::<u16>().is_ok_and(|weight| weight >= 600)
                }
                serde_json::Value::Number(weight) => {
                    weight.as_u64().is_some_and(|weight| weight >= 600)
                }
                serde_json::Value::Bool(weight) => *weight,
                _ => false,
            })
    }

    pub fn set_bold(&mut self, bold: bool) {
        self.font_weight = bold.then(|| serde_json::Value::String("bold".to_string()));
    }

    /// Whether the caption is in the app's face rather than the hand.
    pub fn mono(&self) -> bool {
        self.font_family.as_deref() == Some(MONO)
    }

    pub fn set_mono(&mut self, mono: bool) {
        self.font_family = mono.then(|| MONO.to_string());
    }

    /// Where the caption sits, when the file says so. `None` is "never set",
    /// and the caller falls back to `self.shape().default_align()`.
    pub fn align(&self) -> Option<Align> {
        self.align.as_deref().and_then(Align::parse)
    }

    /// ⚠️ Always writes the key, even for what the shape would have done
    /// anyway. The default depends on the *shape*, and the shape can change
    /// afterwards — dropping "center" off a rect would silently flip the
    /// caption to the left the day it became a label.
    pub fn set_align(&mut self, align: Align) {
        self.align = Some(align.as_str().to_string());
    }

    /// Redraw a `text` node as another shape.
    ///
    /// ⚠️ Only a `text` node has one to change: a card, a frame and a drawing
    /// take their shape from `type` and `draw`, so writing the key on them
    /// would put a claim in the file that nothing ever reads back.
    pub fn set_shape(&mut self, shape: Shape) {
        let mine = matches!(
            shape,
            Shape::Rect | Shape::Ellipse | Shape::Triangle | Shape::Label
        );
        // a loose end is no box to reshape
        if self.kind != "text" || self.draw.is_some() || !mine || self.shape() == Shape::Point {
            return;
        }
        self.shape = Some(shape.as_str().to_string());
    }

    /// Outline thickness in px — the drawing's stroke, or a shape's border.
    pub fn stroke(&self) -> f32 {
        self.stroke_width.unwrap_or(STROKES[0])
    }

    pub fn set_stroke(&mut self, px: f32) {
        self.stroke_width = (px != STROKES[0]).then_some(px);
    }

    /// Degrees clockwise; zero when the node was never turned.
    pub fn angle(&self) -> f32 {
        self.rotation.unwrap_or(0.0)
    }

    pub fn radians(&self) -> f32 {
        self.angle().to_radians()
    }

    /// Wraps into 0..360 and drops a rotation that rounds to nothing, so an
    /// upright node keeps the key out of the file.
    pub fn set_angle(&mut self, degrees: f32) {
        let wrapped = degrees.rem_euclid(360.0);
        self.rotation = (0.05..=359.95).contains(&wrapped).then_some(wrapped);
    }

    /// A world point in the node's own upright frame — everything that reasons
    /// about edges and handles does it here, where the rectangle is a rectangle.
    pub fn local(&self, p: kurbo::Point) -> kurbo::Point {
        turn(p, self.rect().center(), -self.radians())
    }

    /// What the node shows and what search matches on.
    pub fn caption(&self) -> &str {
        match self.shape() {
            Shape::Frame => self.label.as_deref().unwrap_or_default(),
            Shape::File => self.file.as_deref().unwrap_or_default(),
            _ => &self.text,
        }
    }

    /// The other half of [`Node::edit_label`]: what the inline editor hands
    /// back when it is done.
    ///
    /// ⚠️ A section card shows `note.md#Heading` while it is being edited, and
    /// that `#Heading` is the `subpath`, not part of the file name. Writing the
    /// whole string back through `set_caption` — which is what happened — left
    /// the card pointing at a note called `note.md#Heading`, which exists
    /// nowhere, while `subpath` still said `#Heading` as well. The join lives
    /// in `edit_label`, so the split lives here beside it.
    pub fn set_edit_label(&mut self, value: String) {
        if self.shape() == Shape::File {
            match value.split_once('#') {
                Some((file, section)) => {
                    self.file = Some(file.trim().to_string());
                    self.subpath = Some(format!("#{}", section.trim()));
                    return;
                }
                // the `#part` was deleted: it is a whole-note card now
                None => self.subpath = None,
            }
        }
        self.set_caption(value);
    }

    pub fn set_caption(&mut self, value: String) {
        match self.shape() {
            // an empty label is no label: the key leaves the file, same as
            // an edge's label already does (see `flush_edit`)
            Shape::Frame => self.label = (!value.is_empty()).then_some(value),
            Shape::File => self.file = Some(value),
            _ => self.text = value,
        }
    }
}

/// How a connector gets from one node to the other.
///
/// ⚠️ Absent means `Curve`, and that is the interop decision, not the key.
/// A straight line is what this build used to draw and what nothing else
/// draws — the divergence was the old default, not the neutral state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Route {
    #[default]
    Curve,
    Elbow,
    Straight,
}

impl Route {
    /// `None` for the default, so the key stays out of the file.
    pub fn as_str(self) -> Option<&'static str> {
        match self {
            Route::Curve => None,
            Route::Elbow => Some("elbow"),
            Route::Straight => Some("straight"),
        }
    }

    pub fn parse(name: &str) -> Self {
        match name {
            "elbow" => Route::Elbow,
            "straight" => Route::Straight,
            _ => Route::Curve,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    #[serde(rename = "fromNode")]
    pub from_node: String,
    #[serde(rename = "fromSide", default, skip_serializing_if = "Option::is_none")]
    pub from_side: Option<String>,
    #[serde(rename = "toNode")]
    pub to_node: String,
    #[serde(rename = "toSide", default, skip_serializing_if = "Option::is_none")]
    pub to_side: Option<String>,
    /// `arrow` (default) or `none` — the line tool writes `none`.
    #[serde(rename = "toEnd", default, skip_serializing_if = "Option::is_none")]
    pub to_end: Option<String>,
    /// The head at the source — `arrow`, or absent for none. Carried through a
    /// save untouched since ADR-002; ADR-004 is where the style bar started
    /// setting it and the board started drawing it.
    #[serde(rename = "fromEnd", default, skip_serializing_if = "Option::is_none")]
    pub from_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Synapso extension: `mono` writes the label in the app's face
    /// (JetBrains Mono). Absent is the board's hand (Excalifont), the default
    /// for what is written on a canvas.
    #[serde(
        rename = "fontFamily",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<String>,
    /// Synapso extension: where along its pinned side (`fromSide`) the edge
    /// leaves, `0..=1` from the side's start (its top or left end). Absent is
    /// the middle. Ignored without a side: an unpinned end follows the nodes.
    #[serde(rename = "fromAt", default, skip_serializing_if = "Option::is_none")]
    pub from_at: Option<f32>,
    /// `fromAt`'s twin, for `toSide`.
    #[serde(rename = "toAt", default, skip_serializing_if = "Option::is_none")]
    pub to_at: Option<f32>,
    /// Synapso extension: where the middle of the line was dragged to, as an
    /// offset in px from the middle of its two ends. The line bends to pass
    /// through it, and keeps the bend as the nodes move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bend: Option<[f32; 2]>,
    /// Synapso extension: `elbow` | `straight`, absent being a curve. Two
    /// keys and not one enum, because where the line goes and what the link
    /// means are different questions — "dashed elbow" has to be sayable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// Synapso extension. Only ever `true` — off is the key not being there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<bool>,
    /// Synapso extension: line thickness in px. ⚠️ Not `width`, the name the
    /// ADR picked — on a node `width` is the spec's bounding box, and one
    /// concept must not answer to two names depending on where it lives.
    #[serde(
        rename = "strokeWidth",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub stroke_width: Option<f32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// `fontFamily`'s one value: the app's face. Anything else is the hand.
pub const MONO: &str = "mono";

/// The box a loose arrow end (`Shape::Point`) is given, in px: room to grab.
pub const POINT_SIZE: f32 = 12.0;

impl Edge {
    /// Whether the label is in the app's face rather than the hand.
    pub fn mono(&self) -> bool {
        self.font_family.as_deref() == Some(MONO)
    }

    pub fn set_mono(&mut self, mono: bool) {
        self.font_family = mono.then(|| MONO.to_string());
    }
    pub fn has_arrow(&self) -> bool {
        self.to_end.as_deref() != Some("none")
    }

    /// ⚠️ Absent is an arrow, so "on" is the key *going away* — the opposite
    /// of every other setter here, and it is the spec's default, not ours.
    pub fn set_arrow(&mut self, on: bool) {
        self.to_end = (!on).then(|| "none".to_string());
    }

    /// The head at the source. Already in the file and preserved since
    /// ADR-002; ADR-004 is where it started being drawn.
    pub fn back_arrow(&self) -> bool {
        self.from_end.as_deref() == Some("arrow")
    }

    pub fn set_back_arrow(&mut self, on: bool) {
        self.from_end = on.then(|| "arrow".to_string());
    }

    /// The face the user pinned this end to, if any. `None` is the default and
    /// means "whichever side faces the other node right now".
    pub fn face_from(&self) -> Option<Side> {
        self.from_side.as_deref().map(Side::parse)
    }

    pub fn face_to(&self) -> Option<Side> {
        self.to_side.as_deref().map(Side::parse)
    }

    pub fn route(&self) -> Route {
        self.route.as_deref().map_or(Route::Curve, Route::parse)
    }

    pub fn set_route(&mut self, route: Route) {
        self.route = route.as_str().map(str::to_string);
    }

    pub fn dashed(&self) -> bool {
        self.dash == Some(true)
    }

    pub fn set_dashed(&mut self, on: bool) {
        self.dash = on.then_some(true);
    }

    pub fn width(&self) -> f32 {
        self.stroke_width.unwrap_or(STROKES[0])
    }

    pub fn set_width(&mut self, px: f32) {
        self.stroke_width = (px != STROKES[0]).then_some(px);
    }
}

/// The three thicknesses the UI offers, in px. A **number** goes in the file,
/// not a name: restricting the interface costs the same line of code as
/// restricting the format, and only one of them locks the door. A slider was
/// rejected for the opposite reason — it hands back `1.7314`, and two arrows
/// nobody can ever match again.
pub const STROKES: [f32; 3] = [1.5, 3.0, 5.0];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Canvas {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// ⚠️ Same reason as `Node::extra`: a key this build has never heard of —
    /// a newer JSON Canvas, or another editor's — is kept verbatim instead of
    /// being deleted on the first save.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Canvas {
    /// A canvas that is not there yet is an empty one. A canvas that *is*
    /// there and will not parse is an error — loading it as empty is how a
    /// board full of work gets replaced by nothing on the next save.
    pub fn load(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = read(path)?;
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let canvas: Self = serde_json::from_str(&text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // `hit` searches topmost-first while `node`/`node_mut` search
        // first-match, so a duplicated id makes them disagree on which
        // object is which — a click can drag the wrong one. A file this
        // broken opens read-only instead of pretending it is fine.
        let mut seen = std::collections::HashSet::new();
        for id in canvas
            .nodes
            .iter()
            .map(|n| &n.id)
            .chain(canvas.edges.iter().map(|e| &e.id))
        {
            if !seen.insert(id.as_str()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("duplicate id: {id}"),
                ));
            }
        }
        Ok(canvas)
    }

    /// Saves only over the file we loaded — see [`write_checked`].
    ///
    /// ⚠️ `Node::text` is `skip_serializing_if = "String::is_empty"` (keeps
    /// file/link/group nodes clean), but the spec requires `"text"` on every
    /// `text` node even when it is empty. Patched back in here, once, at the
    /// one place a canvas is written, rather than giving up the skip and
    /// growing the key on every other node kind.
    pub fn save(&self, path: &Path, expected: Stamp) -> io::Result<Stamp> {
        let mut value = serde_json::to_value(self)?;
        if let Some(nodes) = value.get_mut("nodes").and_then(|v| v.as_array_mut()) {
            for node in nodes {
                let is_text = node.get("type").and_then(|t| t.as_str()) == Some("text");
                if let Some(obj) = node.as_object_mut() {
                    if is_text && !obj.contains_key("text") {
                        obj.insert("text".into(), serde_json::Value::String(String::new()));
                    }
                }
            }
        }
        write_checked(path, &serde_json::to_string_pretty(&value)?, expected)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Topmost node containing `p` — nodes are drawn in order, so later wins.
    /// Frames only catch clicks on their border band, or they would swallow
    /// every node inside them.
    pub fn hit(&self, p: kurbo::Point) -> Option<&Node> {
        self.nodes.iter().rev().find(|n| {
            let r = n.rect();
            let local = n.local(p);
            match n.shape() {
                Shape::Frame => r.contains(local) && !r.inflate(-10.0, -10.0).contains(local),
                _ => r.contains(local),
            }
        })
    }

    /// Ids are only unique within a file, so a counter is enough — but the
    /// file is hand-editable, so the counter cannot assume anyone respected
    /// it. `u32` overflowed on an id like `"4294967295"` (panics in debug,
    /// wraps to `"0"` in release — every new object then collides on `"0"`).
    /// `u64` push that ceiling out of reach for anything a person would type,
    /// and `u128` for the candidate means even `u64::MAX` cannot overflow the
    /// `+ 1`. The probe against every existing id is the actual guarantee: no
    /// gap-filling from 0, so a dangling edge from a foreign file can never
    /// reconnect to a reused id.
    pub fn next_id(&self) -> String {
        let used: std::collections::HashSet<&str> = self
            .nodes
            .iter()
            .map(|n| n.id.as_str())
            .chain(self.edges.iter().map(|e| e.id.as_str()))
            .chain(
                self.edges
                    .iter()
                    .flat_map(|e| [e.from_node.as_str(), e.to_node.as_str()]),
            )
            .collect();
        let max = used.iter().filter_map(|id| id.parse::<u64>().ok()).max();
        let mut candidate = u128::from(max.unwrap_or(0)) + 1;
        loop {
            let text = candidate.to_string();
            if !used.contains(text.as_str()) {
                return text;
            }
            candidate += 1;
        }
    }

    /// Deleting a node also deletes the edges that touch it — an edge with
    /// nothing left to connect is not a smaller edge, it is a dangling one.
    ///
    /// ⚠️ Split from a single `remove(id)` because a node id and an edge id
    /// share one namespace: retaining on both lists by bare id meant deleting
    /// node "3" also deleted an unrelated edge that happened to be id "3".
    /// The caller always knows which it selected (`board::Selection`), so the
    /// kind comes from there instead of being guessed here.
    pub fn remove_node(&mut self, id: &str) {
        self.nodes.retain(|n| n.id != id);
        self.edges.retain(|e| e.from_node != id && e.to_node != id);
    }

    /// An edge goes by its own id and takes nothing else with it.
    pub fn remove_edge(&mut self, id: &str) {
        self.edges.retain(|e| e.id != id);
    }

    pub fn edge(&self, id: &str) -> Option<&Edge> {
        self.edges.iter().find(|e| e.id == id)
    }

    pub fn edge_mut(&mut self, id: &str) -> Option<&mut Edge> {
        self.edges.iter_mut().find(|e| e.id == id)
    }
}

/// Where pasted pictures land, relative to the vault root. A folder, so the
/// note list stays a note list.
pub const ASSETS: &str = "assets";

/// The clipboard's picture, written into the vault as a PNG. Returns the path
/// *relative to the vault*, which is what both a `.canvas` file node and a
/// markdown `![](…)` want.
///
/// always PNG. A pasted screenshot has no format of its own — it is
/// raw pixels on the pasteboard — and re-encoding to anything else would only
/// add dependencies.
pub fn paste_image(root: &Path) -> io::Result<String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| io::Error::other(e.to_string()))?;
    let picture = clipboard
        .get_image()
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e.to_string()))?;

    let (width, height) = (picture.width as u32, picture.height as u32);
    let buffer =
        image::RgbaImage::from_raw(width, height, picture.bytes.into_owned()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "clipboard image is malformed")
        })?;

    let dir = root.join(ASSETS);
    fs::create_dir_all(&dir)?;
    let path = unique_path(&dir, "pasted", "png");
    buffer
        .save(&path)
        .map_err(|e| io::Error::other(e.to_string()))?;
    // ⚠️ the *file name*, not `name()` — that one drops the extension, and
    // without it nothing downstream can tell the picture from a note card
    Ok(format!(
        "{ASSETS}/{}",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

/// The files on the system clipboard — what a copy in Finder leaves there.
/// `None` when it holds no files at all.
///
/// ⚠️ ask this BEFORE [`paste_image`]: a file copied in Finder also puts the
/// file's icon on the pasteboard as pixels, so reading pixels first pastes a
/// picture of an icon — and a copied PDF would paste as one too.
pub fn clipboard_files() -> Option<Vec<PathBuf>> {
    arboard::Clipboard::new().ok()?.get().file_list().ok()
}

/// The size a pasted picture takes on the canvas: its own, shrunk to fit a
/// reasonable card. Screenshots are retina-huge and would swallow the board.
pub fn image_size(root: &Path, file: &str) -> kurbo::Size {
    let fallback = kurbo::Size::new(320.0, 200.0);
    let Ok(picture) = image::image_dimensions(root.join(file)) else {
        return fallback;
    };
    let (w, h) = (picture.0 as f64, picture.1 as f64);
    if w <= 0.0 || h <= 0.0 {
        return fallback;
    }
    let scale = (420.0 / w).min(320.0 / h).min(1.0);
    kurbo::Size::new((w * scale).round(), (h * scale).round())
}

/// Turn `p` about `centre` by `radians` (clockwise, screen axes).
pub fn turn(p: kurbo::Point, centre: kurbo::Point, radians: f32) -> kurbo::Point {
    if radians == 0.0 {
        return p;
    }
    let (sin, cos) = (radians as f64).sin_cos();
    let (dx, dy) = (p.x - centre.x, p.y - centre.y);
    kurbo::Point::new(
        centre.x + dx * cos - dy * sin,
        centre.y + dx * sin + dy * cos,
    )
}

/// A `file` node points at either a note or a picture.
pub fn is_image(name: &str) -> bool {
    let name = name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// The four faces JSON Canvas names — the only anchors an edge can have, and
/// the one place their geometry lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Top => "top",
            Side::Right => "right",
            Side::Bottom => "bottom",
            Side::Left => "left",
        }
    }

    /// A name this build does not know reads as `bottom` — which is what the
    /// old `side_point` fell through to, so a hand-edited file behaves the same.
    pub fn parse(name: &str) -> Self {
        match name {
            "top" => Side::Top,
            "right" => Side::Right,
            "left" => Side::Left,
            _ => Side::Bottom,
        }
    }

    /// The point `t` of the way along this face of `r`, from its top or left
    /// end: `0.5` is [`Side::point`].
    pub fn point_at(self, r: kurbo::Rect, t: f32) -> kurbo::Point {
        let t = f64::from(t.clamp(0.0, 1.0));
        match self {
            Side::Left => kurbo::Point::new(r.x0, r.y0 + r.height() * t),
            Side::Right => kurbo::Point::new(r.x1, r.y0 + r.height() * t),
            Side::Top => kurbo::Point::new(r.x0 + r.width() * t, r.y0),
            Side::Bottom => kurbo::Point::new(r.x0 + r.width() * t, r.y1),
        }
    }

    /// How far along this face of `r` the point nearest `p` is: the inverse
    /// of [`Side::point_at`].
    pub fn fraction(self, r: kurbo::Rect, p: kurbo::Point) -> f32 {
        let t = if self.horizontal() {
            (p.y - r.y0) / r.height().max(1.0)
        } else {
            (p.x - r.x0) / r.width().max(1.0)
        };
        t.clamp(0.0, 1.0) as f32
    }

    /// The middle of this face of `r`.
    pub fn point(self, r: kurbo::Rect) -> kurbo::Point {
        match self {
            Side::Left => kurbo::Point::new(r.x0, r.y0 + r.height() / 2.0),
            Side::Right => kurbo::Point::new(r.x1, r.y0 + r.height() / 2.0),
            Side::Top => kurbo::Point::new(r.x0 + r.width() / 2.0, r.y0),
            Side::Bottom => kurbo::Point::new(r.x0 + r.width() / 2.0, r.y1),
        }
    }

    /// Unit vector pointing out of the shape — the direction a connector
    /// leaves by, which is what bends a curve instead of kinking it.
    pub fn normal(self) -> kurbo::Vec2 {
        match self {
            Side::Left => kurbo::Vec2::new(-1.0, 0.0),
            Side::Right => kurbo::Vec2::new(1.0, 0.0),
            Side::Top => kurbo::Vec2::new(0.0, -1.0),
            Side::Bottom => kurbo::Vec2::new(0.0, 1.0),
        }
    }

    /// Left and right run along x; top and bottom along y.
    pub fn horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    /// The face of `r` that `p` is closest to — by distance to the edge line,
    /// not to its middle, or the long side of a wide box would never win.
    pub fn nearest(r: kurbo::Rect, p: kurbo::Point) -> Self {
        let gaps = [
            (Side::Top, (p.y - r.y0).abs()),
            (Side::Bottom, (p.y - r.y1).abs()),
            (Side::Left, (p.x - r.x0).abs()),
            (Side::Right, (p.x - r.x1).abs()),
        ];
        gaps.into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or(Side::Bottom, |(side, _)| side)
    }
}

/// Which side of `from` faces `to`.
pub fn facing_side(from: kurbo::Rect, to: kurbo::Rect) -> Side {
    let (dx, dy) = (
        to.center().x - from.center().x,
        to.center().y - from.center().y,
    );
    if dx.abs() > dy.abs() {
        if dx > 0.0 {
            Side::Right
        } else {
            Side::Left
        }
    } else if dy > 0.0 {
        Side::Bottom
    } else {
        Side::Top
    }
}

/// Where an edge leaves and where it arrives: the face the file pins when it
/// names one, and otherwise the face that looks at the other node.
///
/// ⚠️ Resolved every draw, on purpose. An unpinned side follows the nodes
/// around; a pinned one is a choice somebody made and nothing recomputes it.
///
/// A pinned side may also say where along it (`fromAt`/`toAt`). A loose end
/// (`Shape::Point`) is its point's centre, leaving toward the other end.
pub fn edge_ends(edge: &Edge, from: &Node, to: &Node) -> (kurbo::Point, Side, kurbo::Point, Side) {
    let (fr, tr) = (from.rect(), to.rect());
    let end =
        |node: &Node, r: kurbo::Rect, other: kurbo::Rect, pin: Option<Side>, at: Option<f32>| {
            if node.shape() == Shape::Point {
                return (r.center(), facing_side(r, other));
            }
            match (pin, at) {
                (Some(side), Some(t)) => (side.point_at(r, t), side),
                (Some(side), None) => (side.point(r), side),
                (None, _) => {
                    let side = facing_side(r, other);
                    (side.point(r), side)
                }
            }
        };
    let (a, sa) = end(from, fr, tr, edge.face_from(), edge.from_at);
    let (b, sb) = end(to, tr, fr, edge.face_to(), edge.to_at);
    (a, sa, b, sb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Point, Rect};

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64)
    }

    #[test]
    fn neutral_and_custom_colours_survive_the_sidecar_round_trip() {
        let mut prefs = Prefs::default();
        assert_eq!(prefs.canvas_color("new-board"), DocColor::Neutral);
        prefs.set_color("new-board", DocColor::Preset(3));
        assert_eq!(prefs.canvas_color("new-board"), DocColor::Neutral);
        assert_eq!(prefs.color("new-note", 2), DocColor::Preset(2));
        prefs.set_color("board.canvas", DocColor::Neutral);
        prefs.set_color("note", DocColor::Custom("#123ABC".into()));
        let json = serde_json::to_string(&prefs).unwrap();
        let mut back: Prefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.canvas_color("board"), DocColor::Neutral);
        assert_eq!(back.color("note", 0), DocColor::Custom("#123ABC".into()));
        back.rename("board.canvas", "renamed.canvas");
        assert_eq!(back.canvas_color("renamed"), DocColor::Neutral);
        // Old numeric and hexadecimal choices still deserialize unchanged.
        let old: Prefs = serde_json::from_str(r##"{"colors":{"one":2,"two":"#ABCDEF"}}"##).unwrap();
        assert_eq!(old.color("one", 0), DocColor::Preset(2));
        assert_eq!(old.color("two", 0), DocColor::Custom("#ABCDEF".into()));
    }

    #[test]
    fn recent_is_last_opened_first_with_no_repeats() {
        let mut prefs = Prefs::default();
        for note in ["a", "b", "c", "a"] {
            prefs.touch(note);
        }
        assert_eq!(prefs.recent(), ["a", "c", "b"]);
    }

    /// A rename is not an open: the note keeps the place it had.
    #[test]
    fn recent_follows_a_rename_without_jumping_the_queue() {
        let mut prefs = Prefs::default();
        for note in ["a", "b", "c"] {
            prefs.touch(note);
        }
        prefs.rename("a", "z");
        assert_eq!(prefs.recent(), ["c", "b", "z"]);
        prefs.forget("b");
        assert_eq!(prefs.recent(), ["c", "z"]);
    }

    /// An older build closed and listed by bare name: each file of that name
    /// takes the entry once, and the names leave the file.
    #[test]
    fn names_from_an_older_build_move_onto_each_file() {
        let mut prefs: Prefs =
            serde_json::from_str(r#"{"closed":["plan"],"recent":["x","plan","gone"]}"#).unwrap();
        let docs = [("plan", "plan"), ("plan", "plan.canvas"), ("x", "a/x")];
        assert!(prefs.migrate_names(&docs));
        assert!(!prefs.is_open("plan") && !prefs.is_open("plan.canvas"));
        assert!(prefs.is_open("a/x"));
        assert_eq!(prefs.recent(), ["a/x", "plan", "plan.canvas"]);
        let json = serde_json::to_string(&prefs).unwrap();
        assert!(!json.contains(r#""closed":"#) && !json.contains(r#""recent":"#));
        assert!(!prefs.migrate_names(&docs));
    }

    /// A folder rename carries what is keyed under it, and nothing that only
    /// starts with the same letters.
    #[test]
    fn a_folder_rename_carries_every_key_under_it() {
        let mut prefs = Prefs::default();
        for key in ["auth/x", "auth/sub/y.canvas", "authx/z"] {
            prefs.set_open(key, false);
            prefs.touch(key);
            prefs.set_color(key, DocColor::Preset(1));
        }
        prefs.rename("auth/", "login/");
        assert_eq!(prefs.recent(), ["authx/z", "login/sub/y.canvas", "login/x"]);
        for key in ["login/x", "login/sub/y.canvas", "authx/z"] {
            assert!(!prefs.is_open(key));
            assert_eq!(prefs.color(key, 0), DocColor::Preset(1));
        }
        assert!(prefs.is_open("auth/x"));
    }

    /// ⚠️ the sidecar round-trips: an older file with no `recent` key loads,
    /// and a vault nobody has opened anything in does not grow one.
    #[test]
    fn recent_round_trips_through_the_sidecar() {
        let mut prefs: Prefs = serde_json::from_str(r#"{"colors":{}}"#).unwrap();
        assert!(prefs.recent().is_empty());
        assert!(!serde_json::to_string(&prefs).unwrap().contains("recent"));
        prefs.touch("notes");
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(
            serde_json::from_str::<Prefs>(&json).unwrap().recent(),
            ["notes"]
        );
    }

    /// The style bar writes three keys the file did not have. Two of them are
    /// ours and one is the spec's — and the spec's one is backwards.
    #[test]
    fn the_style_bar_keys_say_what_they_mean_in_the_file() {
        let mut n = Node::new("1".into(), Shape::Rect, rect(0.0, 0.0, 10.0, 10.0), "hi");
        // untouched, the key is not in the file — that is what "absent" is for
        assert!(!serde_json::to_string(&n).unwrap().contains("align"));
        n.set_align(Align::Right);
        assert!(serde_json::to_string(&n)
            .unwrap()
            .contains(r#""align":"right""#));
        // ⚠️ written even when it matches the shape's default: the shape can
        // change later, and the default would change under it
        n.set_align(Align::Center);
        assert_eq!(n.align(), Some(Align::Center));
        assert!(serde_json::to_string(&n).unwrap().contains("center"));

        n.set_shape(Shape::Ellipse);
        assert_eq!(n.shape(), Shape::Ellipse);
        n.set_bold(true);
        assert!(n.bold());
        assert!(serde_json::to_string(&n)
            .unwrap()
            .contains(r#""fontWeight":"bold""#));
        n.set_bold(false);
        assert!(!n.bold());
        assert!(n.font_weight.is_none());
        let mut numeric = serde_json::to_value(&n).unwrap();
        numeric["fontWeight"] = serde_json::json!(700);
        let numeric: Node = serde_json::from_value(numeric).unwrap();
        assert!(numeric.bold());
        assert_eq!(numeric.font_weight, Some(serde_json::json!(700)));
        // …but a card, a frame and a drawing take their shape from `type` and
        // `draw`, and the key would be a claim nothing ever reads back
        let mut card = Node::new("2".into(), Shape::File, rect(0.0, 0.0, 10.0, 10.0), "n.md");
        card.set_shape(Shape::Ellipse);
        assert_eq!(card.shape(), Shape::File);
        assert!(card.shape.is_none(), "no shape key on a file node");
        let mut ink = Node::drawing("3".into(), rect(0.0, 0.0, 10.0, 10.0), vec![[0.0, 0.0]]);
        ink.set_shape(Shape::Triangle);
        assert_eq!(ink.shape(), Shape::Draw);

        let mut e = Edge::default();
        assert!(e.has_arrow() && !e.back_arrow(), "the spec's default");
        // ⚠️ ON is the key going away here: `toEnd` absent IS an arrow
        e.set_arrow(false);
        assert_eq!(e.to_end.as_deref(), Some("none"));
        e.set_arrow(true);
        assert!(e.to_end.is_none() && e.has_arrow());
        e.set_back_arrow(true);
        assert_eq!(e.from_end.as_deref(), Some("arrow"));
        assert!(e.back_arrow());
        e.set_back_arrow(false);
        assert!(e.from_end.is_none() && !e.back_arrow());
    }

    #[test]
    fn a_turned_node_is_hit_in_its_own_frame() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "1".into(),
            Shape::Rect,
            rect(0.0, 0.0, 100.0, 20.0),
            "wide",
        ));
        let far_right = kurbo::Point::new(95.0, 10.0);
        assert!(canvas.hit(far_right).is_some());

        canvas.nodes[0].set_angle(90.0);
        // a quarter turn puts the long axis vertical: the old right edge is
        // now below the centre, and the point that was inside is outside
        assert!(canvas.hit(kurbo::Point::new(50.0, 55.0)).is_some());
        assert!(canvas.hit(far_right).is_none());

        // upright again, and the key leaves the file rather than writing 0
        canvas.nodes[0].set_angle(360.0);
        assert_eq!(canvas.nodes[0].rotation, None);
        canvas.nodes[0].set_angle(-90.0);
        assert_eq!(canvas.nodes[0].angle(), 270.0);
        let json = serde_json::to_string(&canvas).unwrap();
        assert!(json.contains("\"rotation\":270"), "{json}");
    }

    #[test]
    fn shapes_survive_a_json_canvas_round_trip() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            canvas.next_id(),
            Shape::Ellipse,
            rect(10.0, 20.0, 100.0, 50.0),
            "token store",
        ));
        canvas.nodes.push(Node::new(
            canvas.next_id(),
            Shape::File,
            rect(0.0, 0.0, 240.0, 108.0),
            "auth-flow.md",
        ));

        let json = serde_json::to_string(&canvas).unwrap();
        assert!(json.contains("\"type\":\"text\""), "spec node type: {json}");
        assert!(json.contains("\"shape\":\"ellipse\""));

        let back: Canvas = serde_json::from_str(&json).unwrap();
        assert_eq!(back.nodes[0].shape(), Shape::Ellipse);
        assert_eq!(back.nodes[0].caption(), "token store");
        assert_eq!(back.nodes[1].shape(), Shape::File);
        assert_eq!(back.nodes[1].caption(), "auth-flow.md");
        assert_eq!(back.next_id(), "3");
    }

    #[test]
    fn a_plain_json_canvas_file_still_loads() {
        // No `shape` key — what Obsidian writes.
        let json = r#"{"nodes":[{"id":"a","type":"text","x":0,"y":0,"width":9,"height":9,
                       "text":"hi"}],"edges":[]}"#;
        let canvas: Canvas = serde_json::from_str(json).unwrap();
        assert_eq!(canvas.nodes[0].shape(), Shape::Rect);
    }

    #[test]
    fn hit_picks_the_topmost_node_and_frames_only_by_their_edge() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "1".into(),
            Shape::Frame,
            rect(0.0, 0.0, 200.0, 200.0),
            "f",
        ));
        canvas.nodes.push(Node::new(
            "2".into(),
            Shape::Rect,
            rect(50.0, 50.0, 50.0, 50.0),
            "r",
        ));

        assert_eq!(canvas.hit(Point::new(75.0, 75.0)).unwrap().id, "2");
        assert_eq!(canvas.hit(Point::new(2.0, 2.0)).unwrap().id, "1"); // frame border
        assert!(canvas.hit(Point::new(150.0, 150.0)).is_none()); // frame interior
        assert!(canvas.hit(Point::new(500.0, 500.0)).is_none());
    }

    #[test]
    fn removing_a_node_takes_its_edges_with_it() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "1".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "a",
        ));
        canvas.nodes.push(Node::new(
            "2".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "b",
        ));
        canvas.edges.push(Edge {
            id: "3".into(),
            from_node: "1".into(),
            to_node: "2".into(),
            ..Edge::default()
        });

        // an edge goes by its own id, and takes nothing with it
        canvas.remove_edge("3");
        assert_eq!(canvas.nodes.len(), 2);
        assert!(canvas.edges.is_empty());

        canvas.edges.push(Edge {
            id: "3".into(),
            from_node: "1".into(),
            to_node: "2".into(),
            ..Edge::default()
        });
        canvas.remove_node("1");
        assert_eq!(canvas.nodes.len(), 1);
        assert!(canvas.edges.is_empty());
    }

    /// A node id and an edge id share one namespace on disk, but `remove_node`
    /// and `remove_edge` no longer confuse the two — deleting node "3" must
    /// not touch an edge that merely happens to also be called "3".
    #[test]
    fn removing_a_node_does_not_delete_an_edge_sharing_its_id() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "3".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "a",
        ));
        canvas.nodes.push(Node::new(
            "4".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "b",
        ));
        canvas.nodes.push(Node::new(
            "5".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "c",
        ));
        // id "3" clashes with the node, but does not reference it
        canvas.edges.push(Edge {
            id: "3".into(),
            from_node: "4".into(),
            to_node: "5".into(),
            ..Edge::default()
        });

        canvas.remove_node("3");
        assert_eq!(canvas.nodes.len(), 2);
        assert_eq!(
            canvas.edges.len(),
            1,
            "edge \"3\" does not touch node \"3\""
        );

        canvas.remove_node("4");
        assert!(
            canvas.edges.is_empty(),
            "edge \"3\" does reference node \"4\" as an endpoint"
        );
    }

    /// The interop decision: a file with no Synapso extension in it draws the
    /// same here as in any other reader, and a default never writes a key.
    #[test]
    fn an_edge_with_no_extension_is_a_solid_curve() {
        let json = r#"{"nodes":[],"edges":[{"id":"e","fromNode":"a","toNode":"b"}]}"#;
        let mut canvas: Canvas = serde_json::from_str(json).unwrap();
        assert_eq!(canvas.edges[0].route(), Route::Curve);
        assert!(!canvas.edges[0].dashed());

        canvas.edges[0].set_route(Route::Elbow);
        canvas.edges[0].set_dashed(true);
        let out = serde_json::to_string(&canvas).unwrap();
        assert!(out.contains("\"route\":\"elbow\""), "{out}");
        assert!(out.contains("\"dash\":true"), "{out}");

        // back to the defaults and both keys leave the file entirely
        canvas.edges[0].set_route(Route::Curve);
        canvas.edges[0].set_dashed(false);
        let out = serde_json::to_string(&canvas).unwrap();
        assert!(!out.contains("route"), "{out}");
        assert!(!out.contains("dash"), "{out}");

        let back: Canvas = serde_json::from_str(&out).unwrap();
        assert_eq!(back.edges[0].route(), Route::Curve);
    }

    #[test]
    fn an_edge_leaves_by_the_face_that_looks_at_the_other_node() {
        let left = Node::new("1".into(), Shape::Rect, rect(0.0, 0.0, 100.0, 40.0), "");
        let right = Node::new("2".into(), Shape::Rect, rect(300.0, 0.0, 100.0, 40.0), "");
        let mut edge = Edge {
            id: "e".into(),
            from_node: "1".into(),
            to_node: "2".into(),
            ..Edge::default()
        };
        let (a, from, b, to) = edge_ends(&edge, &left, &right);
        assert_eq!((from, to), (Side::Right, Side::Left));
        assert_eq!(a, Point::new(100.0, 20.0));
        assert_eq!(b, Point::new(300.0, 20.0));

        // pinned by hand, it stays pinned however the nodes move
        edge.from_side = Some("bottom".into());
        let (_, from, _, _) = edge_ends(&edge, &left, &right);
        assert_eq!(from, Side::Bottom);

        // and where along that side, when it says
        edge.from_at = Some(0.25);
        let (a, _, _, _) = edge_ends(&edge, &left, &right);
        assert_eq!(a, Point::new(25.0, 40.0));

        // a loose end is its point's centre
        let loose = Node::point("3".into(), Point::new(500.0, 200.0));
        edge.to_node = "3".into();
        let (_, _, b, _) = edge_ends(&edge, &left, &loose);
        assert_eq!(b, Point::new(500.0, 200.0));
    }

    #[test]
    fn a_canvas_this_build_does_not_understand_survives_a_save() {
        // a link node, a group background, an edge color, and two keys from a
        // JSON Canvas newer than this one
        let json = r#"{"nodes":[
            {"id":"a","type":"link","x":0,"y":0,"width":10,"height":10,
             "url":"https://example.com","futureKey":{"deep":1}},
            {"id":"b","type":"group","x":0,"y":0,"width":10,"height":10,
             "background":"bg.png","backgroundStyle":"cover"}],
           "edges":[{"id":"e","fromNode":"a","toNode":"b","fromEnd":"arrow",
                     "color":"4","futureEdgeKey":"keep me"}]}"#;
        let canvas: Canvas = serde_json::from_str(json).unwrap();
        let back = serde_json::to_string(&canvas).unwrap();

        assert!(back.contains("\"url\":\"https://example.com\""), "{back}");
        assert!(back.contains("\"background\":\"bg.png\""), "{back}");
        assert!(back.contains("\"backgroundStyle\":\"cover\""), "{back}");
        assert!(back.contains("\"fromEnd\":\"arrow\""), "{back}");
        assert!(back.contains("\"color\":\"4\""), "{back}");
        assert!(back.contains("\"futureKey\":{\"deep\":1}"), "{back}");
        assert!(back.contains("\"futureEdgeKey\":\"keep me\""), "{back}");
        // and the spec's integer geometry stays integer
        assert!(back.contains("\"x\":0,\"y\":0"), "{back}");
    }

    /// AUDIT n14 — ADR item 6 says an unknown JSON Canvas key survives a save.
    /// Node and Edge got `#[serde(flatten)] extra`; the top-level `Canvas` did not,
    /// so anything beside "nodes"/"edges" is deleted the first time we write.
    #[test]
    fn audit_n14_a_top_level_canvas_key_survives_a_save() {
        let json = r#"{"nodes":[],"edges":[],"metadata":{"author":"weber"},"version":"1.0"}"#;
        let canvas: Canvas = serde_json::from_str(json).unwrap();
        let back = serde_json::to_string(&canvas).unwrap();
        assert!(
            back.contains("\"metadata\""),
            "top-level key was eaten: {back}"
        );
        assert!(
            back.contains("\"version\""),
            "top-level key was eaten: {back}"
        );
    }

    #[test]
    fn a_broken_canvas_is_an_error_not_an_empty_board() {
        let dir = std::env::temp_dir().join("synapso-vault-tests");
        fs::create_dir_all(&dir).unwrap();

        let missing = dir.join("never-written.canvas");
        let _ = fs::remove_file(&missing);
        assert!(Canvas::load(&missing).unwrap().nodes.is_empty());

        let broken = dir.join("broken.canvas");
        fs::write(&broken, "{\"nodes\": [ truncated").unwrap();
        assert!(Canvas::load(&broken).is_err());

        // and a write refuses when the file moved under us
        let note = dir.join("guarded.md");
        fs::write(&note, "mine").unwrap();
        let held = stamp(&note);
        fs::write(&note, "someone else's, saved from Obsidian").unwrap();
        assert!(write_checked(&note, "mine, edited", held).is_err());
        assert_eq!(read(&note).unwrap(), "someone else's, saved from Obsidian");
        // with the current stamp it goes through
        let fresh = write_checked(&note, "mine, edited", stamp(&note)).unwrap();
        assert_eq!(read(&note).unwrap(), "mine, edited");
        assert_ne!(fresh, None);
    }

    /// A file this build's own #37 bug could have written: a `u32::MAX` id.
    /// `next_id` used to overflow parsing it (panic in debug, wrap to "0" and
    /// collide forever in release).
    #[test]
    fn next_id_does_not_overflow_on_a_u32_max_id() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "4294967295".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "a",
        ));
        assert_eq!(canvas.next_id(), "4294967296");
    }

    /// Even the id one past `u64::MAX` must not panic — the probe's counter
    /// is `u128` precisely so `max + 1` never overflows.
    #[test]
    fn next_id_does_not_overflow_on_a_u64_max_id() {
        let mut canvas = Canvas::default();
        for id in ["1", "2", "18446744073709551615"] {
            canvas.nodes.push(Node::new(
                id.into(),
                Shape::Rect,
                rect(0.0, 0.0, 10.0, 10.0),
                "a",
            ));
        }
        let next = canvas.next_id();
        assert!(
            canvas.nodes.iter().all(|n| n.id != next),
            "next_id must not collide: {next}"
        );
    }

    /// A foreign file can carry a dangling edge pointing at a node id that
    /// does not exist yet. If `next_id` allocated that id, the dangling edge
    /// would silently reconnect to the freshly created object.
    #[test]
    fn next_id_skips_an_id_a_dangling_edge_already_points_at() {
        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "1".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "a",
        ));
        canvas.edges.push(Edge {
            id: "2".into(),
            from_node: "5".into(),
            to_node: "1".into(),
            ..Edge::default()
        });
        assert_ne!(canvas.next_id(), "5");
    }

    /// #36: `hit` searches topmost-first while `node`/`node_mut` search
    /// first-match, so a duplicated id makes them disagree on which object is
    /// which. Policy: reject the file as broken rather than guess.
    #[test]
    fn canvas_load_rejects_duplicate_ids() {
        let dir = std::env::temp_dir().join("synapso-duplicate-id-tests");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let dupe = dir.join("duplicate-ids.canvas");
        let json = r#"{"nodes":[
            {"id":"7","type":"text","x":0,"y":0,"width":10,"height":10,"text":"a"},
            {"id":"7","type":"text","x":20,"y":0,"width":10,"height":10,"text":"b"}],
           "edges":[]}"#;
        fs::write(&dupe, json).unwrap();
        let err = Canvas::load(&dupe).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        let fine = dir.join("unique-ids.canvas");
        let json = r#"{"nodes":[
            {"id":"1","type":"text","x":0,"y":0,"width":10,"height":10,"text":"a"},
            {"id":"2","type":"text","x":20,"y":0,"width":10,"height":10,"text":"b"}],
           "edges":[]}"#;
        fs::write(&fine, json).unwrap();
        assert_eq!(Canvas::load(&fine).unwrap().nodes.len(), 2);
    }

    /// #43: `Node::text` is skipped when empty, but the spec requires a
    /// `text` node to carry the key regardless. `Canvas::save` patches it
    /// back in on the way to disk.
    #[test]
    fn save_keeps_the_text_key_on_an_empty_text_node() {
        let dir = std::env::temp_dir().join("synapso-empty-text-tests");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty-text.canvas");

        let mut canvas = Canvas::default();
        canvas.nodes.push(Node::new(
            "1".into(),
            Shape::Rect,
            rect(0.0, 0.0, 10.0, 10.0),
            "",
        ));
        assert_eq!(canvas.nodes[0].text, "");

        canvas.save(&path, stamp(&path)).unwrap();
        let on_disk = read(&path).unwrap();
        assert!(on_disk.contains("\"text\": \"\""), "{on_disk}");
    }

    #[test]
    fn write_uses_a_pid_scoped_tmp_name_not_a_shared_one() {
        let dir = std::env::temp_dir().join("synapso-write-tmp-tests");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let note = dir.join("x.md");
        // the old, shared tmp name — as if another writer had one in flight
        let old_style_tmp = dir.join(".x.md.tmp");
        fs::write(&old_style_tmp, "someone else's in-flight write").unwrap();

        write(&note, "new content").unwrap();

        assert_eq!(read(&note).unwrap(), "new content");
        // the other writer's tmp file is untouched, not clobbered
        assert_eq!(
            fs::read_to_string(&old_style_tmp).unwrap(),
            "someone else's in-flight write"
        );
        // our own pid-scoped tmp file is gone after the rename
        let our_tmp = dir.join(format!(".x.md.{}.tmp", std::process::id()));
        assert!(!our_tmp.exists());
    }

    #[test]
    fn deleting_a_note_moves_it_into_the_trash() {
        let dir = std::env::temp_dir().join("synapso-trash-tests");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let note = dir.join("doomed.md");
        fs::write(&note, "still here").unwrap();

        let moved = trash(&dir, &note).unwrap();
        assert!(!note.exists());
        assert_eq!(read(&moved).unwrap(), "still here");
        assert_eq!(moved.parent().unwrap(), dir.join(TRASH));

        // a second note of the same name does not overwrite the first
        fs::write(&note, "the sequel").unwrap();
        let again = trash(&dir, &note).unwrap();
        assert_ne!(again, moved);
        assert_eq!(read(&moved).unwrap(), "still here");
    }

    /// ⚠️ the shell falls back to `list`'s first board after deleting the open
    /// one, so a trashed board that `list` still answers with would be reopened
    /// straight out of `.trash/` — and then saved back there.
    #[test]
    fn a_trashed_board_is_gone_from_the_listing() {
        let dir = std::env::temp_dir().join("synapso-trash-board-tests");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let doomed = dir.join("doomed.canvas");
        let kept = dir.join("kept.canvas");
        fs::write(&doomed, "{}").unwrap();
        fs::write(&kept, "{}").unwrap();

        let moved = trash(&dir, &doomed).unwrap();
        assert_eq!(moved.extension().unwrap(), "canvas", "and still a board");
        assert_eq!(
            list(&dir, "canvas"),
            vec![kept],
            "the trash is not the vault"
        );
    }

    #[test]
    fn a_typed_name_becomes_a_file_name() {
        assert_eq!(sanitize("a/b"), "a-b");
        assert_eq!(sanitize("  ../etc  "), "etc");
        assert_eq!(sanitize(""), "untitled");
        assert_eq!(sanitize("my file"), "my-file");
        assert_eq!(
            sanitize("a VERY LOUD file with maçã"),
            "a-very-loud-file-with-maca"
        );
        // runs of punctuation collapse instead of stacking dashes
        assert_eq!(sanitize("a  --  b!!"), "a-b");
        assert_eq!(sanitize("Ação & Reação"), "acao-reacao");
        assert_eq!(sanitize("!!!"), "untitled");
        // names Windows will not take, whatever the extension
        assert_eq!(sanitize("CON"), "con-note");
        assert_eq!(sanitize("lpt1"), "lpt1-note");
        assert_eq!(sanitize("console"), "console");
    }
}
