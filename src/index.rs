//! The vault as the shell sees it: a folder tree, every document's resolved
//! colour (its own, else the nearest folder's), and the link graph read off
//! `[[mentions]]` — what the tree, the tabs and the graph view all draw from.
//!
//! Rebuilt from disk on demand (after a create, rename, delete or save), never
//! per frame.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use synapso_core::vault::{self, DocColor, Prefs};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum DocKind {
    Note,
    Canvas,
}

#[derive(Clone, PartialEq, Debug)]
pub struct DocEntry {
    /// Relative to the vault root, `/`-separated: `auth/auth-flow.md`.
    pub rel: String,
    pub path: PathBuf,
    pub kind: DocKind,
    /// File stem: what `[[mentions]]` name.
    pub stem: String,
    /// The folder it sits in, `""` for the root, else `auth/` with the slash.
    pub folder: String,
    pub excerpt: String,
    pub modified: Option<std::time::SystemTime>,
    pub created: Option<std::time::SystemTime>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Folder {
    /// `auth/storage/`, `""` for the root.
    pub rel: String,
    pub name: String,
    pub depth: usize,
}

/// A note's links and excerpt as just written (`Index::note_written`).
#[derive(Clone, PartialEq, Debug)]
pub struct NoteChange {
    links: Vec<Link>,
    excerpt: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Mention {
    pub source: String,
    pub target: String,
    pub before: String,
    pub matched: String,
    pub after: String,
}

/// A colour as the sidecar holds it, resolved down the folder chain.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tone {
    /// One of the eight palette slots.
    Slot(usize),
    /// Explicitly neutral, or nothing set anywhere up the chain.
    Neutral,
    Custom(u32),
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Index {
    pub root: PathBuf,
    pub folders: Vec<Folder>,
    pub docs: Vec<DocEntry>,
    pub links: Vec<Link>,
    /// Folder or document key → what the sidecar says, before inheritance.
    pub own: BTreeMap<String, Tone>,
}

pub const TRASH: &str = ".trash";

thread_local! {
    /// The colour popover's hover: (key, tone) shown everywhere until it
    /// leaves; `None` tone previews "back to the folder".
    static PREVIEW: std::cell::RefCell<Option<(String, Option<Tone>)>> = const { std::cell::RefCell::new(None) };
}

pub fn set_preview(p: Option<(String, Option<Tone>)>) {
    PREVIEW.with(|cell| *cell.borrow_mut() = p);
}

fn to_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The sidecar key of a document: its relative path without the extension.
/// A note at the root keys by bare name, exactly as the flat vault did.
pub fn doc_key(rel: &str) -> String {
    rel.rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(rel)
        .to_string()
}

/// The sidecar key a folder or document is coloured under: a folder keeps its
/// `/`, a note drops `.md`, a canvas keeps `.canvas` — so `plan.md` and
/// `plan.canvas` are two colours, and a canvas the Floem shell coloured (its
/// vault was flat: `plan.canvas`) already sits under its key.
pub fn color_key(rel: &str) -> String {
    if rel.ends_with('/') || rel.ends_with(".canvas") {
        rel.to_string()
    } else {
        doc_key(rel)
    }
}

pub fn parent_folder(rel: &str) -> String {
    match rel.trim_end_matches('/').rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/"),
        None => String::new(),
    }
}

fn tone_of(c: &DocColor) -> Tone {
    match c {
        DocColor::Neutral => Tone::Neutral,
        DocColor::Preset(i) => Tone::Slot(*i % 8),
        DocColor::Custom(hex) => u32::from_str_radix(hex.trim_start_matches('#'), 16)
            .map(|v| Tone::Custom(0xFF00_0000 | v))
            .unwrap_or(Tone::Neutral),
    }
}

fn excerpt_of(text: &str) -> String {
    let lines = synapso_core::doc::Lines::parse(text);
    lines.body_preview(EXCERPT)
}

/// How much of a note a card or a preview quotes.
const EXCERPT: usize = 160;

/// What a card pointing at `#Heading` or `#^id` quotes in place of the
/// note's opening: everything under that heading, or the line wearing that
/// anchor. `None` when the note has no such thing.
pub fn section_excerpt(text: &str, section: &str) -> Option<String> {
    let lines = synapso_core::doc::Lines::parse(text);
    match section.strip_prefix('^') {
        Some(id) => lines.block(id),
        None => lines.section(section, EXCERPT),
    }
}

impl Index {
    pub fn load(root: &Path) -> Self {
        let mut folders = vec![];
        let mut docs = vec![];
        let mut stack = vec![(root.to_path_buf(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = read.flatten().map(|e| e.path()).collect();
            entries.sort();
            for path in entries.into_iter().rev() {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if name.starts_with('.') {
                    continue;
                }
                if path.is_dir() {
                    // a linked folder can point back up the tree: never follow one
                    if path.is_symlink() {
                        continue;
                    }
                    let rel = format!("{}/", to_rel(root, &path));
                    folders.push(Folder { rel, name, depth });
                    stack.push((path, depth + 1));
                    continue;
                }
                let kind = match path.extension().and_then(|e| e.to_str()) {
                    Some("md") => DocKind::Note,
                    Some("canvas") => DocKind::Canvas,
                    _ => continue,
                };
                let rel = to_rel(root, &path);
                let meta = std::fs::metadata(&path).ok();
                let excerpt = if kind == DocKind::Note {
                    vault::read(&path)
                        .map(|t| excerpt_of(&t))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                docs.push(DocEntry {
                    folder: parent_folder(&rel),
                    stem: vault::name(&path),
                    rel,
                    path,
                    kind,
                    excerpt,
                    modified: meta.as_ref().and_then(|m| m.modified().ok()),
                    created: meta.as_ref().and_then(|m| m.created().ok()),
                });
            }
        }
        folders.sort_by(|a, b| a.rel.cmp(&b.rel));
        docs.sort_by(|a, b| a.rel.cmp(&b.rel));

        let prefs = Prefs::load(root);
        let own = own_tones(root, &prefs, &folders, &docs);
        let mut index = Index {
            root: root.to_path_buf(),
            folders,
            docs,
            links: vec![],
            own,
        };
        index.links = index.read_links();
        index
    }

    pub fn doc(&self, rel: &str) -> Option<&DocEntry> {
        self.docs.iter().find(|d| d.rel == rel)
    }

    pub fn by_stem(&self, stem: &str) -> Option<&DocEntry> {
        let stem = stem.trim();
        self.docs.iter().find(|d| {
            d.stem.eq_ignore_ascii_case(stem) || doc_key(&d.rel).eq_ignore_ascii_case(stem)
        })
    }

    /// The colour a document or folder shows: its own, else the nearest
    /// folder's, else neutral. A colour being previewed in the popover wins.
    pub fn tone(&self, key_rel: &str) -> Tone {
        let mut key = color_key(key_rel);
        let preview = PREVIEW.with(|p| p.borrow().clone());
        loop {
            if let Some((pk, pt)) = &preview
                && *pk == key
            {
                match pt {
                    Some(t) => return *t,
                    None => {
                        // previewing "back to the folder": skip own
                        let up = parent_folder(&key);
                        if up.is_empty() {
                            return Tone::Neutral;
                        }
                        key = up;
                        continue;
                    }
                }
            }
            if let Some(t) = self.own.get(&key) {
                return *t;
            }
            let up = parent_folder(&key);
            if up.is_empty() {
                return Tone::Neutral;
            }
            key = up;
        }
    }

    /// The colour an item would have with none of its own: its folder's.
    pub fn handed_down(&self, key_rel: &str) -> Tone {
        let up = parent_folder(&color_key(key_rel));
        if up.is_empty() {
            Tone::Neutral
        } else {
            self.tone(&up)
        }
    }

    /// Where the colour comes from: `None` when the item set its own.
    pub fn inherited_from(&self, key_rel: &str) -> Option<String> {
        let key = color_key(key_rel);
        if self.own.contains_key(&key) {
            return None;
        }
        let mut up = parent_folder(&key);
        while !up.is_empty() {
            if self.own.contains_key(&up) {
                return Some(up);
            }
            up = parent_folder(&up);
        }
        None
    }

    /// The graph's group: the top-level folder, `""` at the root.
    pub fn group(&self, rel: &str) -> String {
        match rel.split_once('/') {
            Some((top, _)) => format!("{top}/"),
            None => String::new(),
        }
    }

    fn read_links(&self) -> Vec<Link> {
        let mut out = vec![];
        for d in self.docs.iter().filter(|d| d.kind == DocKind::Note) {
            let Ok(text) = vault::read(&d.path) else {
                continue;
            };
            out.extend(self.links_in(&d.rel, &text));
        }
        // one link per pair and direction
        let mut seen = HashSet::new();
        out.retain(|l| seen.insert((l.from.clone(), l.to.clone())));
        out
    }

    /// The `[[links]]` in one note's text, one per target.
    fn links_in(&self, from: &str, text: &str) -> Vec<Link> {
        let mut out = vec![];
        for line in text.lines() {
            let mut rest = line;
            let mut offset = 0;
            while let Some(start) = rest.find("[[") {
                let Some(len) = rest[start + 2..].find("]]") else {
                    break;
                };
                let inner = &rest[start + 2..start + 2 + len];
                let target = inner.split(['|', '#']).next().unwrap_or("").trim();
                let target = target.strip_prefix("canvas:").unwrap_or(target).trim();
                if let Some(t) = self.by_stem(target)
                    && t.rel != from
                {
                    let abs = offset + start;
                    let before: String = line[..abs]
                        .chars()
                        .rev()
                        .take(40)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    let after_at = abs + 4 + len;
                    let after: String = line
                        .get(after_at..)
                        .unwrap_or("")
                        .chars()
                        .take(30)
                        .collect();
                    out.push(Link {
                        from: from.to_string(),
                        to: t.rel.clone(),
                        before,
                        after,
                    });
                }
                offset += start + 4 + len;
                rest = &rest[start + 4 + len..];
            }
        }
        let mut seen = HashSet::new();
        out.retain(|l| seen.insert(l.to.clone()));
        out
    }

    /// What a note just written changes in the index: its links out and its
    /// excerpt, read from `text`. `None` when neither moved, so an autosave
    /// that only touched words in the middle of a line costs one parse and
    /// no re-render.
    ///
    /// ⚠️ Saving a note never told the index. Its links were read once, at
    /// load and on a full `reindex` (a canvas drop, a rename, a colour): a
    /// `[[link]]` typed between two notes stayed off the graph until
    /// something else happened to reindex.
    pub fn note_written(&self, rel: &str, text: &str) -> Option<NoteChange> {
        let links = self.links_in(rel, text);
        let excerpt = excerpt_of(text);
        let old: Vec<&Link> = self.outgoing(rel);
        let same_links = old.len() == links.len() && old.iter().zip(&links).all(|(a, b)| *a == b);
        let same_excerpt = self.doc(rel).is_some_and(|d| d.excerpt == excerpt);
        (!same_links || !same_excerpt).then_some(NoteChange { links, excerpt })
    }

    /// Put a [`note_written`](Index::note_written) change in.
    pub fn apply(&mut self, rel: &str, change: NoteChange) {
        self.links.retain(|l| l.from != rel);
        self.links.extend(change.links);
        if let Some(d) = self.docs.iter_mut().find(|d| d.rel == rel) {
            d.excerpt = change.excerpt;
            d.modified = Some(std::time::SystemTime::now());
        }
    }

    pub fn backlinks(&self, rel: &str) -> Vec<&Link> {
        self.links.iter().filter(|l| l.to == rel).collect()
    }

    pub fn outgoing(&self, rel: &str) -> Vec<&Link> {
        self.links.iter().filter(|l| l.from == rel).collect()
    }

    pub fn degree(&self, rel: &str) -> usize {
        self.links
            .iter()
            .filter(|l| l.from == rel || l.to == rel)
            .count()
    }

    /// Plain text in `rel` that names another note without linking it:
    /// `device unlink` for `device-unlink.md`.
    pub fn mentions(&self, rel: &str) -> Vec<Mention> {
        let Some(d) = self.doc(rel) else {
            return vec![];
        };
        let Ok(text) = vault::read(&d.path) else {
            return vec![];
        };
        let linked: HashSet<&str> = self.outgoing(rel).iter().map(|l| l.to.as_str()).collect();
        let mut out = vec![];
        for t in self
            .docs
            .iter()
            .filter(|t| t.kind == DocKind::Note && t.rel != rel)
        {
            if linked.contains(t.rel.as_str()) || t.stem.len() < 4 {
                continue;
            }
            let phrase = t.stem.replace(['-', '_'], " ");
            for line in text.lines() {
                if line.contains("[[")
                    && line
                        .to_lowercase()
                        .contains(&format!("[[{}", t.stem.to_lowercase()))
                {
                    continue;
                }
                if let Some(std::ops::Range { start: at, end }) = find_ci(line, &phrase) {
                    out.push(Mention {
                        source: rel.to_string(),
                        target: t.rel.clone(),
                        before: line[..at]
                            .chars()
                            .rev()
                            .take(30)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect(),
                        matched: line[at..end].to_string(),
                        after: line[end..].chars().take(30).collect(),
                    });
                    break;
                }
            }
        }
        out
    }

    /// Notes per top-level folder, in tree order.
    pub fn groups(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for d in &self.docs {
            *counts.entry(self.group(&d.rel)).or_default() += 1;
        }
        counts.into_iter().collect()
    }

    pub fn children(&self, folder: &str) -> (Vec<&Folder>, Vec<&DocEntry>) {
        let fs = self
            .folders
            .iter()
            .filter(|f| parent_folder(&f.rel) == folder)
            .collect();
        let ds = self.docs.iter().filter(|d| d.folder == folder).collect();
        (fs, ds)
    }

    /// Adjacency for BFS, undirected.
    pub fn neighbours(&self) -> HashMap<&str, Vec<&str>> {
        let mut map: HashMap<&str, Vec<&str>> = HashMap::new();
        for l in &self.links {
            map.entry(&l.from).or_default().push(&l.to);
            map.entry(&l.to).or_default().push(&l.from);
        }
        map
    }
}

fn own_tones(
    root: &Path,
    prefs: &Prefs,
    folders: &[Folder],
    docs: &[DocEntry],
) -> BTreeMap<String, Tone> {
    // `Prefs` keeps its map private; ask it per key it could hold. A missing
    // key comes back as the fallback we pass, which we read as "not set".
    const UNSET: usize = usize::MAX;
    let mut own = BTreeMap::new();
    let keys = folders
        .iter()
        .map(|f| f.rel.clone())
        .chain(docs.iter().map(|d| color_key(&d.rel)));
    for key in keys {
        match prefs.color(&key, UNSET) {
            DocColor::Preset(UNSET) => {}
            other => {
                own.insert(key, tone_of(&other));
            }
        }
    }
    // the Floem shell keyed a note's colour by its bare name; a nested note
    // coloured there still reads, until it is coloured again here — when it
    // is the only note of that name, so the colour stays on the note it was for
    let mut names: HashMap<&str, usize> = HashMap::new();
    for d in docs.iter().filter(|d| d.kind == DocKind::Note) {
        *names.entry(&d.stem).or_default() += 1;
    }
    for d in docs
        .iter()
        .filter(|d| d.kind == DocKind::Note && names[d.stem.as_str()] == 1)
    {
        let key = doc_key(&d.rel);
        if own.contains_key(&key) {
            continue;
        }
        if let other @ (DocColor::Neutral | DocColor::Custom(_) | DocColor::Preset(0..UNSET)) =
            prefs.color(&d.stem, UNSET)
        {
            own.insert(key, tone_of(&other));
        }
    }
    let _ = root;
    own
}

/// Where `needle` sits in `line`, ignoring case, as byte offsets into `line`.
///
/// ⚠️ lowercasing can change a char's length (`İ` is two bytes, `i̇` three), so
/// the lowercased line keeps, per byte, where in `line` its char started.
fn find_ci(line: &str, needle: &str) -> Option<std::ops::Range<usize>> {
    let mut lower = String::with_capacity(line.len());
    let mut from = Vec::with_capacity(line.len() + 1);
    for (i, ch) in line.char_indices() {
        for l in ch.to_lowercase() {
            lower.push(l);
            from.extend(std::iter::repeat_n(i, l.len_utf8()));
        }
    }
    from.push(line.len());
    let needle = needle.to_lowercase();
    let at = lower.find(&needle)?;
    let end = at + needle.len();
    // a match that starts or ends inside one char's lowercasing is no match
    let whole = |k: usize| k == 0 || from[k - 1] != from[k];
    (!needle.is_empty() && whole(at) && whole(end)).then(|| from[at]..from[end])
}

/// Set (or with `None`, clear back to the folder's) the colour of a folder or
/// document, and persist the sidecar. Edits the JSON in place so every other
/// key the sidecar holds (closed tabs, recents) rides through untouched.
pub fn set_tone(root: &Path, key_rel: &str, tone: Option<Tone>) -> std::io::Result<()> {
    let key = color_key(key_rel);
    let file = root.join(".synapso.json");
    let mut json: serde_json::Value = match vault::read(&file) {
        Ok(text) => serde_json::from_str(&text).map_err(std::io::Error::other)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e),
    };
    let obj = json
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("sidecar is not an object"))?;
    let colors = obj
        .entry("colors")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| std::io::Error::other("colors is not an object"))?;
    match tone {
        Some(Tone::Slot(i)) => {
            colors.insert(key, serde_json::json!(i));
        }
        Some(Tone::Neutral) => {
            colors.insert(key, serde_json::Value::Null);
        }
        Some(Tone::Custom(v)) => {
            colors.insert(key, serde_json::json!(format!("#{:06X}", v & 0xFFFFFF)));
        }
        None => {
            colors.remove(&key);
            // a nested note may still read the Floem shell's bare-name key
            // (`own_tones`): back to the folder retires that too, unless the
            // name is a note at the root, whose own key it is
            if let Some((_, name)) = key.rsplit_once('/')
                && !name.is_empty()
                && !name.ends_with(".canvas")
                && !root.join(format!("{name}.md")).exists()
            {
                colors.remove(name);
            }
        }
    }
    let text = serde_json::to_string_pretty(&json).map_err(std::io::Error::other)?;
    vault::write(&file, &text)
}

pub fn rel_of(root: &Path, path: &Path) -> String {
    to_rel(root, path)
}

pub fn file_name(path: &Path) -> String {
    vault::display_name(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("synapso-index-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, text) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root
    }

    const CANVAS: &str = r#"{"nodes":[],"edges":[]}"#;

    #[test]
    fn a_link_typed_between_two_notes_reaches_the_graph_as_it_is_saved() {
        let root = vault("live-links", &[("a.md", "alone"), ("b.md", "the other")]);
        let mut index = Index::load(&root);
        assert!(index.links.is_empty());
        // a.md written with a link: it's in, and the excerpt with it
        let change = index
            .note_written("a.md", "see [[b]]")
            .expect("a link appeared");
        index.apply("a.md", change);
        assert_eq!(index.outgoing("a.md").len(), 1);
        assert_eq!(index.outgoing("a.md")[0].to, "b.md");
        assert_eq!(index.backlinks("b.md").len(), 1);
        // written again unchanged: nothing to do, no re-render
        assert_eq!(index.note_written("a.md", "see [[b]]"), None);
        // the link taken out: gone from the graph
        let change = index.note_written("a.md", "see b").unwrap();
        index.apply("a.md", change);
        assert!(index.links.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_note_stands_out_only_with_a_colour_its_folder_does_not_hand_down() {
        let index = Index {
            own: BTreeMap::from([
                ("auth/".to_string(), Tone::Slot(0)),
                ("auth/flow".to_string(), Tone::Slot(3)),
                ("auth/same".to_string(), Tone::Slot(0)),
                ("loose".to_string(), Tone::Slot(5)),
            ]),
            ..Default::default()
        };
        let stands_out = |rel: &str| index.tone(rel) != index.handed_down(rel);
        assert!(stands_out("auth/flow.md"));
        assert!(!stands_out("auth/same.md"), "its own, but the folder's");
        assert!(!stands_out("auth/tokens.md"), "inherited");
        assert!(stands_out("loose.md"), "the root hands down none");
        assert!(!stands_out("plain.md"));
        assert_eq!(index.handed_down("auth/deep/x.md"), Tone::Slot(0));
    }

    #[test]
    fn a_note_and_a_canvas_of_one_name_keep_two_colours() {
        let root = vault(
            "twins",
            &[
                ("plan.md", ""),
                ("plan.canvas", CANVAS),
                (".synapso.json", r#"{"colors":{"plan.canvas":3}}"#),
            ],
        );
        set_tone(&root, "plan.md", Some(Tone::Slot(1))).unwrap();
        let index = Index::load(&root);
        assert_eq!(index.tone("plan.md"), Tone::Slot(1));
        assert_eq!(index.tone("plan.canvas"), Tone::Slot(3));
        set_tone(&root, "plan.canvas", Some(Tone::Slot(5))).unwrap();
        let index = Index::load(&root);
        assert_eq!(index.tone("plan.canvas"), Tone::Slot(5));
        assert_eq!(index.tone("plan.md"), Tone::Slot(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn back_to_the_folder_retires_the_floem_colour() {
        let root = vault(
            "legacy",
            &[
                ("auth/flow.md", ""),
                (".synapso.json", r#"{"colors":{"flow":2,"auth/":4}}"#),
            ],
        );
        assert_eq!(Index::load(&root).tone("auth/flow.md"), Tone::Slot(2));
        set_tone(&root, "auth/flow.md", None).unwrap();
        assert_eq!(Index::load(&root).tone("auth/flow.md"), Tone::Slot(4));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_floem_colour_stays_on_the_note_that_owns_the_name() {
        let root = vault(
            "owner",
            &[
                ("flow.md", ""),
                ("auth/flow.md", ""),
                (".synapso.json", r#"{"colors":{"flow":2}}"#),
            ],
        );
        let index = Index::load(&root);
        assert_eq!(index.tone("flow.md"), Tone::Slot(2));
        assert_eq!(index.tone("auth/flow.md"), Tone::Neutral);
        set_tone(&root, "auth/flow.md", None).unwrap();
        assert_eq!(Index::load(&root).tone("flow.md"), Tone::Slot(2));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_match_is_cut_from_the_line_as_written() {
        // `İ` lowercases to three bytes: offsets into the lowercased line are off by one
        let line = "İ test.";
        assert_eq!(find_ci(line, "test").map(|r| &line[r]), Some("test"));
        let line = "a Árvore Grande b";
        assert_eq!(
            find_ci(line, "árvore grande").map(|r| &line[r]),
            Some("Árvore Grande")
        );
        assert_eq!(find_ci("nothing here", "test"), None);
    }

    /// A card pointing at `#Heading` quotes that section and one pointing at
    /// `#^id` the line wearing the anchor, not the note's opening. A name the
    /// note doesn't have quotes nothing, and the card keeps the opening.
    #[test]
    fn a_section_card_quotes_its_section_and_an_anchor_card_its_line() {
        let note = "# Auth\n\nthe opening\n\n## Token\n\nthe gateway holds it\n\n\
                    ### Detail\n\ndeeper\n\n## Next\n\nthe line ^gate-1\n";
        assert_eq!(
            section_excerpt(note, "token").as_deref(),
            Some("the gateway holds it\nDetail\ndeeper"),
            "down to the next heading as high as its own"
        );
        assert_eq!(
            section_excerpt(note, "^gate-1").as_deref(),
            Some("the line")
        );
        assert_eq!(section_excerpt(note, "nowhere"), None);
        assert_eq!(section_excerpt(note, "^nope"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_folder_is_not_followed() {
        let root = vault("link", &[("a/note.md", "")]);
        std::os::unix::fs::symlink(&root, root.join("a/up")).unwrap();
        let index = Index::load(&root);
        assert_eq!(index.docs.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
