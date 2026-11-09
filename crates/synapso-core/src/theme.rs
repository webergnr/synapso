//! Semantic token layer — 1:1 translation of design/tokens.css.
//! SINGLE SOURCE OF TRUTH: widgets reference only these values.
#![allow(dead_code)] // the token scale is complete by contract, used or not

use peniko::Color;

/// A drop shadow as three numbers — shell-neutral on purpose: Floem has no
/// such type and reads the fields directly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub y: f32,
    pub blur: f32,
}

const fn shadow(color: Color, y: f32, blur: f32) -> Shadow {
    Shadow { color, y, blur }
}

const fn c(hex: u32) -> Color {
    Color::new([
        ((hex >> 16) & 0xFF) as f32 / 255.0,
        ((hex >> 8) & 0xFF) as f32 / 255.0,
        (hex & 0xFF) as f32 / 255.0,
        1.0,
    ])
}

const fn ca(hex: u32, a: f32) -> Color {
    let mut components = c(hex).components;
    components[3] = a;
    Color::new(components)
}

// typography (px)
pub const TEXT_XS: f32 = 10.0; // status bar, captions, kbd
pub const TEXT_SM: f32 = 11.0; // chrome: tabs, menus, rail, buttons
pub const TEXT_BASE: f32 = 13.0; // editor body, inputs, cards
pub const TEXT_MD: f32 = 15.0; // h3, card titles
pub const TEXT_LG: f32 = 18.0; // h2
pub const TEXT_XL: f32 = 24.0; // h1

// spacing
pub const SPACE_1: f32 = 2.0;
pub const SPACE_2: f32 = 4.0;
pub const SPACE_3: f32 = 6.0;
pub const SPACE_4: f32 = 8.0;
pub const SPACE_5: f32 = 12.0;
pub const SPACE_6: f32 = 16.0;
pub const SPACE_7: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;

// radius
pub const RADIUS_1: f32 = 3.0;
pub const RADIUS_2: f32 = 4.0;
pub const RADIUS_3: f32 = 6.0;

// borders
pub const BORDER_W: f32 = 1.0;
pub const SPINE_W: f32 = 2.0; // the doc spine — signature element

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tokens {
    pub bg_base: Color,
    pub surface_panel: Color,
    pub surface_raised: Color,
    pub surface_overlay: Color,
    pub surface_inset: Color,
    pub border_subtle: Color,
    pub border_strong: Color,
    pub text_primary: Color,
    pub text_muted: Color,
    pub text_faint: Color,
    pub text_link: Color,
    pub on_accent: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub danger: Color,
    pub danger_dim: Color,
    pub doc: [Color; 4], // coral, amber, violet, cyan
    pub doc_dim: [Color; 4],
    pub canvas_bg: Color,
    pub canvas_dot: Color,
    pub connector: Color,
    pub scrim: Color,
    pub selection: Color,
    pub search_hl: Color,
    pub search_hl_active: Color,
    pub shadow_1: Shadow, // canvas cards
    pub shadow_2: Shadow, // rail, menus
    pub shadow_3: Shadow, // palette
}

/// Every theme the picker offers, in menu order. Adding one is a row here.
///
/// The names are the design's own bare words — `design/tokens.css` calls the
/// two blocks `[data-theme="dark"]` and `[data-theme="light"]`, and the status
/// bar prints `◐ {{ theme }}` with nothing else in it
/// (`design/Workspace.dc.html`, the status bar's theme slot). A brand in front
/// of them ("Syn Dark") is a
/// word the design never shows.
pub const THEMES: [(&str, Tokens); 2] = [("dark", DARK), ("light", LIGHT)];

pub const DARK: Tokens = Tokens {
    bg_base: c(0x131418),
    surface_panel: c(0x1A1C21),
    surface_raised: c(0x22252C),
    surface_overlay: c(0x262A32),
    surface_inset: c(0x0E0F12),
    border_subtle: c(0x2B2F38),
    border_strong: c(0x3A4050),
    text_primary: c(0xD6D9E0),
    text_muted: c(0x8A91A0),
    text_faint: c(0x5B6270),
    text_link: c(0x56C8D8),
    on_accent: c(0x0C1F14),
    accent: c(0x4FD08A),
    accent_dim: ca(0x4FD08A, 0.14),
    danger: c(0xE5484D),
    danger_dim: ca(0xE5484D, 0.14),
    doc: [c(0xE8737F), c(0xE3B341), c(0xA78BFA), c(0x56C8D8)],
    doc_dim: [
        ca(0xE8737F, 0.12),
        ca(0xE3B341, 0.12),
        ca(0xA78BFA, 0.12),
        ca(0x56C8D8, 0.12),
    ],
    canvas_bg: c(0x101115),
    canvas_dot: c(0x25282E),
    connector: c(0x4A5162),
    scrim: ca(0x090A0D, 0.55),
    selection: ca(0x4FD08A, 0.22),
    search_hl: ca(0xE3B341, 0.32),
    search_hl_active: c(0xE3B341),
    shadow_1: shadow(ca(0x000000, 0.40), 1.0, 2.0),
    shadow_2: shadow(ca(0x000000, 0.45), 4.0, 12.0),
    shadow_3: shadow(ca(0x000000, 0.55), 12.0, 32.0),
};

pub const LIGHT: Tokens = Tokens {
    bg_base: c(0xF6F7F9),
    surface_panel: c(0xFFFFFF),
    surface_raised: c(0xFFFFFF),
    surface_overlay: c(0xFFFFFF),
    surface_inset: c(0xEFF1F4),
    border_subtle: c(0xE2E5EA),
    border_strong: c(0xC7CCD5),
    text_primary: c(0x23262D),
    text_muted: c(0x646B79),
    text_faint: c(0x9AA1AD),
    text_link: c(0x0E7A8A),
    on_accent: c(0xFFFFFF),
    accent: c(0x1E9E5F),
    accent_dim: ca(0x1E9E5F, 0.12),
    danger: c(0xCE2C31),
    danger_dim: ca(0xCE2C31, 0.10),
    doc: [c(0xC4414E), c(0x9A7011), c(0x7554D6), c(0x0D8296)],
    doc_dim: [
        ca(0xC4414E, 0.10),
        ca(0x9A7011, 0.10),
        ca(0x7554D6, 0.10),
        ca(0x0D8296, 0.10),
    ],
    canvas_bg: c(0xF0F1F4),
    canvas_dot: c(0xD5D9E0),
    connector: c(0xA9B0BC),
    scrim: ca(0x17191F, 0.30),
    selection: ca(0x1E9E5F, 0.18),
    search_hl: ca(0x9A7011, 0.22),
    search_hl_active: c(0x9A7011),
    shadow_1: shadow(ca(0x141821, 0.08), 1.0, 2.0),
    shadow_2: shadow(ca(0x141821, 0.12), 4.0, 12.0),
    shadow_3: shadow(ca(0x141821, 0.18), 12.0, 32.0),
};

/// The palettes above, read back out of the design.
///
/// ⚠️ every hex here is typed by hand, and a hand-typed hex is the single
/// easiest thing in this repo to get quietly wrong: nothing crashes, one bar is
/// four points too dark, and it survives every review because nobody diffs a
/// screenshot against a stylesheet. So the stylesheet checks the file instead.
/// `design/tokens.css` is the source of truth by its own first line; this reads
/// it at compile time and fails the moment a value drifts from it — including
/// a token the design ADDS, which is the drift that would otherwise go
/// unnoticed for a release.
#[cfg(test)]
mod against_the_design {
    use super::*;
    use std::collections::BTreeMap;

    /// ⚠️ `include_str!` and not a runtime read: a test that opens a file finds
    /// nothing when the harness runs from somewhere else, and a skipped test
    /// looks exactly like a passing one.
    const CSS: &str = include_str!("../../../design/tokens.css");

    /// Tokens that are not colours and have no field here.
    /// `--shadow-focus` is written in terms of two other vars. The group
    /// colours past the fourth belong to the shell's eight-colour palette
    /// (`src/theme.rs`), and its own test holds those to this file.
    const NOT_OURS: [&str; 9] = [
        "shadow-focus",
        "doc-5",
        "doc-5-dim",
        "doc-6",
        "doc-6-dim",
        "doc-7",
        "doc-7-dim",
        "doc-8",
        "doc-8-dim",
    ];

    /// The `--name: value` pairs inside `[data-theme="<which>"] { … }`.
    ///
    /// ⚠️ split on `;`, never on lines: the light block puts a colour and its
    /// dim twin on one line, and a line-wise parser reads only the first.
    fn block(which: &str) -> BTreeMap<String, String> {
        let head = format!("[data-theme=\"{which}\"]");
        let body = CSS
            .split_once(&head)
            .unwrap_or_else(|| panic!("no {head} block in design/tokens.css"))
            .1;
        let body = body.split_once('{').expect("no brace").1;
        let body = body.split_once('}').expect("unclosed brace").0;
        let mut out = BTreeMap::new();
        for piece in decomment(body).split(';') {
            let Some((name, value)) = piece.split_once(':') else {
                continue;
            };
            let name = name.trim();
            if let Some(name) = name.strip_prefix("--") {
                out.insert(name.to_string(), value.trim().to_string());
            }
        }
        out
    }

    fn decomment(css: &str) -> String {
        let mut out = String::with_capacity(css.len());
        let mut rest = css;
        while let Some((before, after)) = rest.split_once("/*") {
            out.push_str(before);
            rest = after.split_once("*/").map_or("", |(_, tail)| tail);
        }
        out.push_str(rest);
        out
    }

    /// `#RRGGBB` or `rgba(r, g, b, a)` — the only two forms the file uses.
    fn color_of(value: &str) -> Color {
        if let Some(hex) = value.strip_prefix('#') {
            let n = u32::from_str_radix(hex, 16).expect("not hex");
            assert_eq!(hex.len(), 6, "{value}: only #RRGGBB is handled");
            return c(n);
        }
        let inside = value
            .strip_prefix("rgba(")
            .and_then(|v| v.strip_suffix(')'))
            .unwrap_or_else(|| panic!("{value}: neither #RRGGBB nor rgba()"));
        let parts: Vec<&str> = inside.split(',').map(str::trim).collect();
        assert_eq!(parts.len(), 4, "{value}: rgba wants four parts");
        let ch = |at: usize| parts[at].parse::<u32>().expect("not a channel");
        ca(
            (ch(0) << 16) | (ch(1) << 8) | ch(2),
            parts[3].parse::<f32>().expect("not an alpha"),
        )
    }

    /// `0 <y>px <blur>px <color>` — the only form the file uses.
    fn shadow_of(value: &str) -> Shadow {
        let (offsets, color) = value.split_once("rgba").expect("no rgba in shadow");
        let px: Vec<f32> = offsets
            .split_whitespace()
            .filter_map(|w| w.strip_suffix("px"))
            .map(|w| w.parse().expect("not a length"))
            .collect();
        assert_eq!(px.len(), 2, "{value}: wanted a y and a blur");
        shadow(color_of(&format!("rgba{color}")), px[0], px[1])
    }

    fn same(what: &str, got: Color, want: Color) {
        let (a, b) = (got.components, want.components);
        let off = (0..4).map(|i| (a[i] - b[i]).abs()).fold(0.0_f32, f32::max);
        assert!(off < 1e-6, "{what}: rust has {a:?}, the design says {b:?}");
    }

    fn check(which: &str, rust: &Tokens) {
        let css = block(which);
        let mut seen: Vec<&str> = Vec::new();
        let mut color = |name: &'static str, got: Color| {
            seen.push(name);
            let want = css
                .get(name)
                .unwrap_or_else(|| panic!("{which}: the design has no --{name}"));
            same(&format!("{which} --{name}"), got, color_of(want));
        };
        color("bg-base", rust.bg_base);
        color("surface-panel", rust.surface_panel);
        color("surface-raised", rust.surface_raised);
        color("surface-overlay", rust.surface_overlay);
        color("surface-inset", rust.surface_inset);
        color("border-subtle", rust.border_subtle);
        color("border-strong", rust.border_strong);
        color("text-primary", rust.text_primary);
        color("text-muted", rust.text_muted);
        color("text-faint", rust.text_faint);
        color("text-link", rust.text_link);
        color("on-accent", rust.on_accent);
        color("accent", rust.accent);
        color("accent-dim", rust.accent_dim);
        color("danger", rust.danger);
        color("danger-dim", rust.danger_dim);
        color("doc-1", rust.doc[0]);
        color("doc-2", rust.doc[1]);
        color("doc-3", rust.doc[2]);
        color("doc-4", rust.doc[3]);
        color("doc-1-dim", rust.doc_dim[0]);
        color("doc-2-dim", rust.doc_dim[1]);
        color("doc-3-dim", rust.doc_dim[2]);
        color("doc-4-dim", rust.doc_dim[3]);
        color("canvas-bg", rust.canvas_bg);
        color("canvas-dot", rust.canvas_dot);
        color("connector", rust.connector);
        color("scrim", rust.scrim);
        color("selection", rust.selection);
        color("search-hl", rust.search_hl);
        color("search-hl-active", rust.search_hl_active);

        for (name, got) in [
            ("shadow-1", rust.shadow_1),
            ("shadow-2", rust.shadow_2),
            ("shadow-3", rust.shadow_3),
        ] {
            seen.push(name);
            let want = shadow_of(&css[name]);
            same(&format!("{which} --{name}"), got.color, want.color);
            assert_eq!(
                (got.y, got.blur),
                (want.y, want.blur),
                "{which} --{name}: geometry"
            );
        }

        // the half that catches a token the design GAINED: anything left over
        // is a colour on screen in the export that this binary cannot paint
        let missed: Vec<&String> = css
            .keys()
            .filter(|k| !seen.contains(&k.as_str()) && !NOT_OURS.contains(&k.as_str()))
            .collect();
        assert!(
            missed.is_empty(),
            "{which}: design/tokens.css has tokens `Tokens` does not: {missed:?}"
        );
    }

    #[test]
    fn dark_is_the_design() {
        check("dark", &DARK);
    }

    #[test]
    fn light_is_the_design() {
        check("light", &LIGHT);
    }
}
