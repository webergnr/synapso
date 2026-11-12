//! Synapso: notes and canvas in one workspace over a plain folder of Markdown
//! and JSON Canvas files — folder tree, tab rail, note sheet, canvas, graph —
//! drawn with Freya and fed by `synapso-core`.

// a release build on Windows opens no console window behind the app
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod behave;
mod board;
mod colorpop;
mod field;
mod format;
mod graph;
mod icons;
mod index;
mod keys;
mod note;
mod overlays;
mod rail;
mod rich;
mod session;
mod stats;
mod status;
mod theme;
mod titlebar;
mod toast;
mod tree;

use std::{collections::HashSet, path::PathBuf};

use freya::animation::*;
use freya::prelude::*;

use crate::{
    app::{App, Caret, ColorPick, GraphState, Overlay, View},
    index::{DocKind, Index},
    theme::*,
};

const JB_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const JB_MEDIUM: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf");
const JB_BOLD: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf");
const JB_ITALIC: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf");
const EXCALIFONT: &[u8] = include_bytes!("../assets/fonts/Excalifont-Regular.ttf");

fn main() {
    // `synapso <folder>`: ⌘O from a shell — that folder becomes the vault, now
    // and next launch. Created if missing, and canonical: a relative path
    // remembered as typed would open another folder from the next shell.
    // Dashed args (an old Finder's `-psn_…`, a stray `--help`) aren't folders.
    if let Some(dir) = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .find(|a| !a.to_string_lossy().starts_with('-'))
    {
        let opened = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::canonicalize(&dir))
            .map(|dir| PathBuf::from(plain(&dir.to_string_lossy())))
            .and_then(|dir| synapso_core::vault::set_root(&dir));
        if let Err(e) = opened {
            eprintln!("synapso: can't open {}: {e}", dir.display());
            std::process::exit(1);
        }
    }
    let mut config = LaunchConfig::new();
    if let Some(stats) = stats::Stats::from_env() {
        config = config.with_plugin(stats);
    }
    // Windows has no system bar (`titlebar` draws one): the plugin's bands
    // along the edges resize the window, as the system's border would
    if cfg!(windows) {
        config = config.with_plugin(freya::borderless::BorderlessPlugin::new().with_thickness(5.));
    }
    launch(
        config
            .with_font(FONT, JB_REGULAR)
            .with_font(FONT, JB_MEDIUM)
            .with_font(FONT, JB_BOLD)
            .with_font(FONT, JB_ITALIC)
            .with_font(HAND, EXCALIFONT)
            .with_default_font(FONT)
            .with_window(
                WindowConfig::new(app)
                    .with_on_close(|_, _| {
                        // nothing typed is lost to the close button
                        note::flush_note();
                        board::flush_board();
                        // ⚠️ leave HERE, saved, instead of answering `Close`:
                        // Freya would drop the window inside this callback and
                        // the run loop then turns once more, and on a Mac with a
                        // Touch Bar AppKit's Touch Bar finder takes its observer
                        // off the dead `WinitView` and aborts the app
                        // (NSRangeException, "nextResponder"). One window, so
                        // closing it is quitting.
                        // ponytail: a second window needs the fix in the fork
                        // (drop the window after the run loop stops)
                        std::process::exit(0)
                    })
                    .with_size(1600., 940.)
                    .with_background(DARK.bg_base)
                    .with_title("Synapso")
                    .with_decorations(!cfg!(windows))
                    .with_window_attributes(|attrs, _| {
                        // ⚠️ the Mac's bar goes and `titlebar` takes its place:
                        // transparent + full-size content + no title is the
                        // recipe that KEEPS the traffic lights and lets the
                        // content run under them (`with_titlebar_hidden`
                        // would take the lights too)
                        #[cfg(target_os = "macos")]
                        let attrs = {
                            use freya::winit::platform::macos::WindowAttributesExtMacOS;
                            attrs
                                .with_title_hidden(true)
                                .with_titlebar_transparent(true)
                                .with_fullsize_content_view(true)
                        };
                        // without decorations Windows drops the shadow and
                        // the round corners of Windows 11 unless asked for them
                        // and the logo on the bar, Alt-Tab and the taskbar:
                        // the exe's own icon (build.rs) is Explorer's only
                        #[cfg(windows)]
                        let attrs = {
                            use freya::winit::platform::windows::{
                                CornerPreference, WindowAttributesExtWindows,
                            };
                            let icon = LaunchConfig::window_icon(include_bytes!(
                                "../assets/icon/synapso-windows-256.png"
                            ));
                            attrs
                                .with_undecorated_shadow(true)
                                .with_corner_preference(CornerPreference::Round)
                                .with_window_icon(Some(icon.clone()))
                                .with_taskbar_icon(Some(icon))
                        };
                        attrs
                    }),
            ),
    )
}

/// A canonical path as everything else spells it. On Windows `canonicalize`
/// answers `\\?\C:\…` (or `\\?\UNC\server\…`), while ⌘O's picker and every
/// path the OS hands in (a dropped file's) say `C:\…`: the same vault
/// remembered in two spellings matched no file dropped into it.
fn plain(path: &str) -> String {
    match path.strip_prefix(r"\\?\") {
        Some(rest) => match rest.strip_prefix(r"UNC\") {
            Some(share) => format!(r"\\{share}"),
            None => rest.to_string(),
        },
        None => path.to_string(),
    }
}

/// The vault: the folder ⌘O last picked, else `$SYNAPSO_VAULT`, else
/// `~/Synapso` (`vault::root`). For a scratch vault to play in, point
/// `SYNAPSO_VAULT` at `fixture/vault` (`python fixture/gen_vault.py`).
pub fn vault_root() -> PathBuf {
    synapso_core::vault::root()
}

const SPLIT_DEFAULT: f32 = 0.42;

pub fn ctrl(e: &KeyboardEventData) -> bool {
    e.modifiers.contains(Modifiers::CONTROL) || e.modifiers.contains(Modifiers::META)
}

fn app() -> impl IntoElement {
    let app = use_provide_context(|| {
        #[cfg(target_os = "macos")]
        dock_icon();
        let index = Index::load(&vault_root());
        let session::Session {
            tabs,
            canvases,
            note,
            canvas,
        } = session::restore(&index);
        let pane = first_pane(&note, &canvas);
        App {
            graph: State::create(GraphState {
                focus: note.clone(),
                history: vec![],
                branch: false,
                depth: 2,
                hidden: HashSet::new(),
                week: 0,
            }),
            index: State::create(index),
            tabs: State::create(tabs),
            canvases: State::create(canvases),
            note: State::create(note),
            canvas: State::create(canvas),
            pane: State::create(pane),
            view: State::create(View::Split),
            split: State::create(SPLIT_DEFAULT),
            tree: State::create(false),
            tree_pinned: State::create(false),
            last_view: State::create(View::Split),
            dark: State::create(true),
            overlay: State::create(Overlay::None),
            toast: State::create(None),
            caret: State::create(Caret::default()),
            dirty: State::create(HashSet::new()),
            color_pick: State::create(None),
            flush: State::create(0),
            naming: State::create(None),
            last_undo: State::create(None),
            drag_doc: State::create(None),
            drop_at: State::create(None),
            board_rev: State::create(0),
            note_rev: State::create(0),
            reveal: State::create(None),
            land: State::create(None),
            find: State::create(None),
            find_at: State::create(0),
            insert: State::create(None),
            tree_menu: State::create(None),
            canvas_zoom: State::create(1.0),
            note_menu: State::create(None),
        }
    });

    let dark = *app.dark.read();
    theme::set_dark(dark);
    let root = app.root();
    let history = titlebar::use_history(app);
    let mut dragging = use_state(|| false);
    let width = use_state(|| 1300f32);
    let mut width_w = width;
    let left = use_state(|| 300f32);
    let mut left_w = left;

    // notes / split / canvas glide instead of jumping
    let view = app.view;
    let split = app.split;
    let target = use_memo(move || match *view.read() {
        View::Notes => 1.0f32,
        View::Canvas | View::Graph => 0.0,
        View::Split => *split.read(),
    });
    let ratio_anim = use_animation_transition(target, |from: f32, to| {
        AnimNum::new(from, to)
            .time(320)
            .function(Function::Expo)
            .ease(Ease::Out)
    });
    let ratio = if *dragging.read() {
        *split.read()
    } else {
        ratio_anim.read().value()
    };

    let on_key = move |e: Event<KeyboardEventData>| {
        app::set_mods(&e, true);
        let mut overlay = app.overlay;
        if matches!(e.key, Key::Named(NamedKey::Escape)) {
            if *overlay.read() != Overlay::None {
                overlay.set(Overlay::None);
            }
            if colorpop::close_strip(app) {
                return;
            }
            let mut pick = app.color_pick;
            if pick.read().is_some() {
                index::set_preview(None);
                pick.set(None);
                return;
            }
            // a name being typed: this Esc cancels it and the drawer stays. Cleared
            // here, not left to the field, which may be gone (its row folded
            // away) and would leave `naming` set, and the drawer unclosable
            // (the field's own cancel runs after this and is idempotent)
            let mut naming = app.naming;
            if naming.peek().is_some() {
                naming.set(None);
                return;
            }
            if *overlay.read() == Overlay::None && *app.tree.read() && !*app.tree_pinned.read() {
                let mut t = app.tree;
                t.set(false);
            }
            return;
        }
        if !ctrl(&e) {
            return;
        }
        let shift = e.modifiers.contains(Modifiers::SHIFT);
        if std::env::var_os("SYNAPSO_DEBUG").is_some() {
            eprintln!("chord {:?} {:?} {:?}", e.code, e.key, e.modifiers);
        }
        let ch = match &e.code {
            Code::KeyK => "k",
            Code::KeyP => "p",
            Code::KeyF => "f",
            Code::KeyE => "e",
            Code::KeyG => "g",
            Code::Digit1 => "1",
            Code::Digit2 => "2",
            Code::Digit3 => "3",
            Code::Digit4 => "4",
            Code::KeyL => "l",
            Code::KeyN => "n",
            Code::KeyO => "o",
            Code::KeyW => "w",
            Code::KeyZ => "z",
            _ => return,
        };
        let mut view = app.view;
        match (ch, shift) {
            ("k", false) => overlay.set(Overlay::Palette),
            ("p", false) => overlay.set(Overlay::Picker),
            ("f", true) => overlay.set(Overlay::Search),
            ("f", false) => {
                let mut find = app.find;
                if find.read().is_none() {
                    find.set(Some(String::new()));
                }
            }
            ("e", true) => {
                let mut t = app.tree;
                let open = *t.read();
                t.set(!open);
            }
            ("1", _) => view.set(View::Notes),
            ("2", _) => view.set(View::Split),
            ("3", _) => view.set(View::Canvas),
            ("4", _) => {
                if *view.read() != View::Graph {
                    app.toggle_graph();
                }
            }
            ("g", false) => app.toggle_graph(),
            ("l", true) => toggle_theme(app),
            ("n", false) => new_note(app, None),
            ("o", false) => open_vault(app),
            ("w", false) => {
                let rel = match view.read().pane(*app.pane.read()) {
                    DocKind::Canvas => app.canvas.read().clone(),
                    DocKind::Note => app.note.read().clone(),
                };
                app.close_tab(&rel);
            }
            ("z", false) => {
                // ⌘Z where no editor holds the keyboard: take back the last trash
                let last = app.last_undo.read().clone();
                if let Some(u) = last
                    && (*view.read() == View::Graph
                        || app.note.read().is_empty()
                        || app.toast.read().is_some())
                {
                    app.undo(&u);
                }
            }
            _ => {}
        }
    };

    let tree_open = *app.tree.read();
    let full = *app.view.read() == View::Graph;

    rect()
        // ⚠️ the theme is a plain flag that `c()` reads, not a signal: a
        // component whose props did not change keeps the old colours, so a
        // flip remounts the whole window (state worth keeping lives above).
        // So does ⌘O: the panes key by the path inside the vault, and a note
        // both vaults hold (`README.md`) kept the first vault's buffer
        .key((dark, root))
        .expanded()
        .background(c().bg_base)
        .color(c().text_primary)
        .font_family(FONT)
        .font_size(TEXT_BASE)
        .on_global_key_down(on_key)
        .on_file_drop(move |e: Event<FileEventData>| take_files(app, &e.file_paths))
        .on_global_key_up(move |e: Event<KeyboardEventData>| {
            app::set_mods(&e, false);
        })
        .on_global_pointer_up(move |_| {
            // a tree drag that ended anywhere but the board is dropped
            if app.drag_doc.read().is_some() {
                spawn(async move {
                    freya::prelude::timer(std::time::Duration::from_millis(30)).await;
                    let mut d = app.drag_doc;
                    d.set(None);
                    let mut h = app.drop_at;
                    h.set(None);
                });
            }
        })
        .maybe_child(titlebar::shown().then_some(titlebar::TitleBar { history }))
        .child(
            // everything under the window bar: the floating tree is placed
            // from here, not from the window's top
            rect()
                .width(Size::fill())
                .height(Size::fill())
                .content(Content::Flex)
                .child(
                    rect()
                        .horizontal()
                        .width(Size::fill())
                        // ⚠️ flex, not `Size::func(parent - 24)`: a func sees
                        // PHYSICAL pixels, so at 2x it left the 24px status
                        // bar 12 and pushed the rest off the window
                        .height(Size::flex(1.))
                        .maybe_child((*app.view.read() != View::Notes).then_some(rail::Rail {}))
                        .maybe_child(
                            (tree_open && *app.tree_pinned.read())
                                .then_some(tree::Tree { floating: false }),
                        )
                        .child(if full {
                            graph::GraphView {}.into_element()
                        } else {
                            rect()
                                .horizontal()
                                .width(Size::fill())
                                .height(Size::fill())
                                .on_sized(move |e: Event<SizedEventData>| {
                                    width_w.set(e.area.width());
                                    left_w.set(e.area.min_x());
                                })
                                .on_global_pointer_move(move |e: Event<PointerEventData>| {
                                    if *dragging.read() {
                                        let mut split = app.split;
                                        let x = e.global_location().x as f32 - *left.read();
                                        split.set((x / *width.read()).clamp(0.22, 0.75));
                                    }
                                })
                                .on_global_pointer_up(move |_| {
                                    if *dragging.read() {
                                        dragging.set(false);
                                    }
                                })
                                .child(
                                    rect()
                                        .width(Size::px((*width.read() * ratio).max(0.)))
                                        .height(Size::fill())
                                        .overflow(Overflow::Clip)
                                        // ⚠️ mouse, not pointer, down: the panes'
                                        // chips and rails stop the pointer event
                                        .on_mouse_down(move |_| {
                                            let mut p = app.pane;
                                            p.set_if_modified(DocKind::Note);
                                        })
                                        .child({
                                            let rel = app.note.read().clone();
                                            if rel.is_empty() {
                                                note::empty_pane(app).into_element()
                                            } else {
                                                note::NotePane { rel }.into_element()
                                            }
                                        }),
                                )
                                .child(divider(ratio, dragging))
                                .child(
                                    rect()
                                        .width(Size::fill())
                                        .height(Size::fill())
                                        .overflow(Overflow::Clip)
                                        .on_mouse_down(move |_| {
                                            let mut p = app.pane;
                                            p.set_if_modified(DocKind::Canvas);
                                        })
                                        .child(board::Board {
                                            rel: app.canvas.read().clone(),
                                            rev: *app.board_rev.read(),
                                        }),
                                )
                                .into_element()
                        }),
                )
                .child(tree::FloatingTree {})
                .child(status::StatusBar {}),
        )
        .child(overlays::Overlays {})
        .child(colorpop::ColorPop {})
        .child(note::NoteMenu {})
        .child(toast::ToastHost {})
}

/// ⌘O: pick another folder; it becomes the vault, now and next launch.
///
/// ⚠️ a NATIVE modal, run from inside the event loop: it spins its own loop
/// until answered, so no script can drive it. What happens after the pick is
/// `vault::set_root` (proved in `tests/open_vault.rs`) and a fresh session.
pub fn open_vault(app: App) {
    note::flush_note();
    board::flush_board();
    let Some(picked) = rfd::FileDialog::new()
        .set_title("open a folder of .md files")
        .set_directory(app.root())
        .pick_folder()
    else {
        return;
    };
    let root = match synapso_core::vault::set_root(&picked) {
        Ok(root) => root,
        Err(e) => {
            app.toast(
                format!("can't open {}", picked.display()),
                e.to_string(),
                None,
            );
            return;
        }
    };
    let index = Index::load(&root);
    let session::Session {
        tabs,
        canvases,
        note,
        canvas,
    } = session::restore(&index);
    let mut p = app.pane;
    p.set(first_pane(&note, &canvas));
    let (mut i, mut t, mut n, mut c) = (app.index, app.tabs, app.note, app.canvas);
    i.set(index);
    t.set(tabs);
    let mut cs = app.canvases;
    cs.set(canvases);
    n.set(note.clone());
    c.set(canvas);
    let (mut d, mut u, mut g) = (app.dirty, app.last_undo, app.graph);
    d.set(HashSet::new());
    u.set(None);
    g.write().focus = note;
    app.toast(format!("opened {}", root.display()), "", None);
}

/// The Dock and ⌘⇥ show a terminal-launched binary with the generic
/// executable glyph — a bundled .app reads its `.icns`, a bare one has
/// nothing. So the logo is embedded and handed to AppKit at startup, and
/// both launch paths look the same. Called from the first render, after the
/// app has finished launching: the Dock ignores an icon set before that.
#[cfg(target_os = "macos")]
fn dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("../assets/icon/synapso-512.png"));
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    // unsafe: the binding cannot see that AppKit just retains the image
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}

pub fn toggle_theme(app: App) {
    // whatever is typed goes to disk before the panes are rebuilt
    crate::note::flush_note();
    crate::board::flush_board();
    let mut dark = app.dark;
    let d = *dark.read();
    dark.set(!d);
}

/// ⌘N: a note in the open note's folder, opened with its name ready to type.
pub fn new_note(app: App, folder: Option<String>) {
    let root = app.root();
    let folder = folder.unwrap_or_else(|| index::parent_folder(&app.note.read()));
    let dir = root.join(&folder);
    let _ = std::fs::create_dir_all(&dir);
    let path = synapso_core::vault::unique_path(&dir, "untitled", "md");
    if synapso_core::vault::write(&path, "").is_err() {
        app.toast("could not create a note", folder, None);
        return;
    }
    app.reindex();
    let rel = index::rel_of(&root, &path);
    app.open(&rel);
    let mut naming = app.naming;
    naming.set(Some(rel));
}

/// A canvas next to the open note, opened in the board.
pub fn new_canvas(app: App, folder: Option<String>) {
    let root = app.root();
    let folder = folder.unwrap_or_else(|| index::parent_folder(&app.note.read()));
    let dir = root.join(&folder);
    let _ = std::fs::create_dir_all(&dir);
    let path = synapso_core::vault::unique_path(&dir, "untitled", "canvas");
    if synapso_core::vault::write(&path, "{}").is_err() {
        return;
    }
    app.reindex();
    let rel = index::rel_of(&root, &path);
    app.open(&rel);
    let mut naming = app.naming;
    naming.set(Some(rel));
}

/// The pane ⌘W starts on: the note's, unless only a board is open.
fn first_pane(note: &str, canvas: &str) -> DocKind {
    if note.is_empty() && !canvas.is_empty() {
        DocKind::Canvas
    } else {
        DocKind::Note
    }
}

/// Rename a document or folder in place. Refuses a name that's taken; the
/// sidecar colour and open tabs follow.
pub fn rename(app: App, rel: &str, new_name: &str) -> Result<String, String> {
    let root = app.root();
    let is_folder = rel.ends_with('/');
    let raw = new_name.trim();
    let raw = raw
        .strip_suffix(".md")
        .or_else(|| raw.strip_suffix(".canvas"))
        .unwrap_or(raw);
    let clean = synapso_core::vault::sanitize(raw);
    if clean.is_empty() {
        return Err("a name can't be empty".into());
    }
    let parent = index::parent_folder(rel);
    let new_rel = if is_folder {
        format!("{parent}{clean}/")
    } else {
        let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("md");
        format!("{parent}{clean}.{ext}")
    };
    if new_rel == rel {
        return Ok(new_rel);
    }
    let from = root.join(rel.trim_end_matches('/'));
    let to = root.join(new_rel.trim_end_matches('/'));
    // a case-only rename is the same file on Windows: let it through
    if to.exists() && !new_rel.eq_ignore_ascii_case(rel) {
        return Err(format!("{} already exists", new_rel.trim_end_matches('/')));
    }
    // what the panes hold reaches disk first: the rewrite below sees the board,
    // and the note pane remounts from the moved file
    note::flush_note();
    board::flush_board();
    std::fs::rename(&from, &to).map_err(|e| e.to_string())?;
    let rewired = relink_canvases(&root, rel, &new_rel);
    // the colour moves with it
    let key = index::color_key(rel);
    let tone = app.index.read().own.get(&key).copied();
    if let Some(t) = tone {
        let _ = index::set_tone(&root, rel, None);
        let _ = index::set_tone(&root, &new_rel, Some(t));
    }
    session::renamed(&root, rel, &new_rel);
    let fix = |s: &str| -> String {
        if is_folder && s.starts_with(rel) {
            format!("{new_rel}{}", &s[rel.len()..])
        } else if s == rel {
            new_rel.clone()
        } else {
            s.to_string()
        }
    };
    for mut list in [app.tabs, app.canvases] {
        let fixed: Vec<String> = list.read().iter().map(|t| fix(t)).collect();
        list.set(fixed);
    }
    let mut note = app.note;
    let n = fix(&note.read());
    note.set(n);
    let mut canvas = app.canvas;
    let cv = fix(&canvas.read());
    canvas.set(cv);
    app.reindex();
    if rewired > 0 {
        let mut rev = app.board_rev;
        *rev.write() += 1;
    }
    Ok(new_rel)
}

/// Every `.canvas` in the vault that points at `from` (or inside it, for a
/// folder) points at `to` afterwards. Loads and saves through the core so
/// unknown keys ride through. Returns how many boards changed.
fn relink_canvases(root: &std::path::Path, from: &str, to: &str) -> usize {
    let index = Index::load(root);
    let mut changed = 0;
    for d in index.docs.iter().filter(|d| d.kind == DocKind::Canvas) {
        let Ok(mut canvas) = synapso_core::vault::Canvas::load(&d.path) else {
            continue;
        };
        let stamp = synapso_core::vault::stamp(&d.path);
        let mut touched = false;
        for n in canvas.nodes.iter_mut() {
            let Some(f) = n.file.clone() else { continue };
            let new = if from.ends_with('/') && f.starts_with(from) {
                Some(format!("{to}{}", &f[from.len()..]))
            } else if f == from {
                Some(to.to_string())
            } else {
                None
            };
            if let Some(new) = new {
                n.file = Some(new);
                touched = true;
            }
        }
        if touched && canvas.save(&d.path, stamp).is_ok() {
            changed += 1;
        }
    }
    changed
}

pub fn new_folder(app: App, parent: &str) {
    let root = app.root();
    let base = root.join(parent);
    let mut n = 0;
    let path = loop {
        let name = if n == 0 {
            "new-folder".to_string()
        } else {
            format!("new-folder-{}", n + 1)
        };
        let p = base.join(&name);
        if !p.exists() {
            break p;
        }
        n += 1;
    };
    if std::fs::create_dir_all(&path).is_ok() {
        app.reindex();
        let rel = format!("{}/", index::rel_of(&root, &path));
        let mut naming = app.naming;
        naming.set(Some(rel));
    }
}

fn divider(ratio: f32, mut dragging: State<bool>) -> impl IntoElement {
    let mut hover = use_state(|| false);
    let shown = ratio > 0.001 && ratio < 0.999;
    rect()
        .width(Size::px(if shown { 6. } else { 0. }))
        .height(Size::fill())
        .center()
        .background(if *hover.read() || *dragging.read() {
            c().accent_dim
        } else {
            c().bg_base
        })
        .cursor(CursorIcon::ColResize)
        .on_pointer_enter(move |_| hover.set(true))
        .on_pointer_leave(move |_| hover.set(false))
        .on_pointer_down(move |_| dragging.set(true))
        .child(
            rect()
                .width(Size::px(2.))
                .height(Size::px(28.))
                .corner_radius(999.)
                .background(c().border_strong),
        )
}

/// A tiny bordered tag: `MD`, `doc`, `▾`, a shortcut (which draws its
/// modifier the platform's way).
pub fn tag(text: impl Into<String>) -> Element {
    let text = text.into();
    if text.starts_with('⌘') {
        return keys::chip(&text).into_element();
    }
    rect()
        .border(Border::new().fill(c().border_subtle).width(1.))
        .corner_radius(3.)
        .padding((0., 4.))
        .child(label().text(text).font_size(TEXT_XS).color(c().text_faint))
        .into_element()
}

/// The 2px colour line — tabs, tree rows, cards and the note gutter all wear
/// one, and every one of them opens the colour popover.
#[derive(PartialEq)]
pub struct Spine {
    pub key: String,
    pub tone: index::Tone,
    pub height: Size,
    pub glow: bool,
}

impl Component for Spine {
    fn render(&self) -> impl IntoElement {
        let app = use_consume::<App>();
        let mut hover = use_state(|| false);
        let color = c().tone(self.tone);
        let mut pick = app.color_pick;
        let hov = *hover.read();
        let key = self.key.clone();
        rect()
            .width(Size::px(8.))
            .height(self.height.clone())
            .padding((0., 3.))
            .on_pointer_enter(move |_| hover.set(true))
            .on_pointer_leave(move |_| hover.set(false))
            .cursor(CursorIcon::Pointer)
            .on_pointer_down(move |e: Event<PointerEventData>| {
                e.stop_propagation();
                let g = e.global_location();
                pick.set(Some(ColorPick::at(key.clone(), g.x as f32, g.y as f32)));
            })
            // the row under the line must not take the click as its own
            .on_press(|e: Event<PressEventData>| e.stop_propagation())
            .child(
                rect()
                    .width(Size::px(if hov { 4. } else { 2. }))
                    .height(Size::fill())
                    .corner_radius(1.)
                    .background(color)
                    .shadow(if hov || self.glow {
                        Shadow::new().blur(8.).color(color)
                    } else {
                        Shadow::new()
                    }),
            )
    }
}

pub fn kind_of(rel: &str) -> DocKind {
    if rel.ends_with(".canvas") {
        DocKind::Canvas
    } else {
        DocKind::Note
    }
}

/// `auth/auth-flow.md` → `auth-flow.md`.
pub fn base_name(rel: &str) -> &str {
    rel.trim_end_matches('/').rsplit('/').next().unwrap_or(rel)
}

/// A plain value as a signal: written when it changes, so an animation
/// transition (which watches signals, not values) sees the change.
pub fn use_tracked<T: Clone + PartialEq + 'static>(value: T) -> State<T> {
    let mut state = use_state(|| value.clone());
    if *state.peek() != value {
        state.set(value);
    }
    state
}

/// Files dropped on the window where no pane took them: into the vault, and
/// the last note or board among them opened.
pub fn take_files(app: App, paths: &[PathBuf]) {
    let mut last = None;
    let beside = app.canvas.read().clone();
    for path in paths {
        if let Some(rel) = absorb(app, path, &beside) {
            last = Some(rel);
        }
    }
    app.reindex();
    if let Some(rel) = last.filter(|r| !synapso_core::vault::is_image(r)) {
        app.open(&rel);
    }
}

/// Where a file event happened, in the logical pixels every layout area and
/// pointer event uses.
///
/// ⚠️ Freya hands a file event's cursor over in PHYSICAL pixels: a pointer
/// event's is divided by the scale factor, a file event's is not
/// (`EmmitableEvent::new` in freya-core). On a 2x screen a picture dropped on
/// the board landed twice as far from the window's corner as the cursor —
/// mostly off the pane, so the drop looked like it did nothing. Take the
/// division out the day the fork scales it.
pub fn file_cursor(e: &FileEventData) -> (f64, f64) {
    let scale = *Platform::get().current_window().scale_factor.peek();
    (e.cursor.x / scale, e.cursor.y / scale)
}

/// A file dropped on the window, brought into the vault: a note or board
/// lands in the open note's folder, a picture in `assets/` beside `beside`
/// (the board or note it was dropped for). A file already in
/// the vault is used where it is; anything else is refused. Never overwrites.
pub fn absorb(app: App, path: &std::path::Path, beside: &str) -> Option<String> {
    let root = app.root();
    if let Ok(inside) = path.strip_prefix(&root) {
        return Some(inside.to_string_lossy().replace('\\', "/"));
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    let dir = match ext.as_str() {
        "md" | "canvas" => root.join(index::parent_folder(&app.note.read())),
        _ if synapso_core::vault::is_image(&path.to_string_lossy()) => root
            .join(index::parent_folder(beside))
            .join(synapso_core::vault::ASSETS),
        _ => {
            app.toast(
                format!("can't take {}", synapso_core::vault::display_name(path)),
                "only notes, boards and pictures",
                None,
            );
            return None;
        }
    };
    let _ = std::fs::create_dir_all(&dir);
    let to = synapso_core::vault::unique_path(&dir, &synapso_core::vault::name(path), &ext);
    std::fs::copy(path, &to).ok()?;
    Some(index::rel_of(&root, &to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vault_from_the_command_line_is_spelled_as_the_picker_spells_it() {
        assert_eq!(plain(r"\\?\C:\Users\me\notes"), r"C:\Users\me\notes");
        assert_eq!(
            plain(r"\\?\UNC\server\share\notes"),
            r"\\server\share\notes"
        );
        assert_eq!(plain("/home/me/notes"), "/home/me/notes");
        assert_eq!(plain(r"C:\already"), r"C:\already");
    }
}
