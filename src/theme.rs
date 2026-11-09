//! The design's tokens (`design/tokens.css`), dark and light. `c()` is the
//! palette in force; flipping the theme re-keys the root so everything
//! repaints from the other one.
#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, Ordering};

use freya::prelude::Color;

const fn hex(v: u32) -> Color {
    Color::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
const fn hexa(v: u32, a: f32) -> Color {
    Color::from_argb((a * 255.) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const FONT: &str = "JetBrains Mono";
/// What is drawn on the board by hand — free text, captions, edge labels
/// (Weber, 2026-10-28). Cards keep `FONT`: they show a note.
pub const HAND: &str = "Excalifont";

pub struct Palette {
    pub dark: bool,
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
    pub doc: [Color; 8],
    pub doc_dim: [Color; 8],
    pub canvas_bg: Color,
    pub canvas_dot: Color,
    pub connector: Color,
    pub scrim: Color,
    pub selection: Color,
    pub search_hl: Color,
    pub search_hl_active: Color,
    /// Alpha of the drop shadows: the light theme's are much softer.
    pub shadow: f32,
}

pub static DARK: Palette = Palette {
    dark: true,
    bg_base: hex(0x131418),
    surface_panel: hex(0x1A1C21),
    surface_raised: hex(0x22252C),
    surface_overlay: hex(0x262A32),
    surface_inset: hex(0x0E0F12),
    border_subtle: hex(0x2B2F38),
    border_strong: hex(0x3A4050),
    text_primary: hex(0xD6D9E0),
    text_muted: hex(0x8A91A0),
    text_faint: hex(0x5B6270),
    text_link: hex(0x56C8D8),
    on_accent: hex(0x0C1F14),
    accent: hex(0x4FD08A),
    accent_dim: hexa(0x4FD08A, 0.14),
    danger: hex(0xE5484D),
    danger_dim: hexa(0xE5484D, 0.14),
    doc: [
        hex(0xE8737F),
        hex(0xE3B341),
        hex(0xA78BFA),
        hex(0x56C8D8),
        hex(0xEE9A5B),
        hex(0xA9C95B),
        hex(0x7B9CF5),
        hex(0xE37BC9),
    ],
    doc_dim: [
        hexa(0xE8737F, 0.12),
        hexa(0xE3B341, 0.12),
        hexa(0xA78BFA, 0.12),
        hexa(0x56C8D8, 0.12),
        hexa(0xEE9A5B, 0.12),
        hexa(0xA9C95B, 0.12),
        hexa(0x7B9CF5, 0.12),
        hexa(0xE37BC9, 0.12),
    ],
    canvas_bg: hex(0x101115),
    canvas_dot: hex(0x25282E),
    connector: hex(0x4A5162),
    scrim: hexa(0x090A0D, 0.55),
    selection: hexa(0x4FD08A, 0.22),
    search_hl: hexa(0xE3B341, 0.32),
    search_hl_active: hex(0xE3B341),
    shadow: 0.45,
};

pub static LIGHT: Palette = Palette {
    dark: false,
    bg_base: hex(0xF6F7F9),
    surface_panel: hex(0xFFFFFF),
    surface_raised: hex(0xFFFFFF),
    surface_overlay: hex(0xFFFFFF),
    surface_inset: hex(0xEFF1F4),
    border_subtle: hex(0xE2E5EA),
    border_strong: hex(0xC7CCD5),
    text_primary: hex(0x23262D),
    text_muted: hex(0x646B79),
    text_faint: hex(0x9AA1AD),
    text_link: hex(0x0E7A8A),
    on_accent: hex(0xFFFFFF),
    accent: hex(0x1E9E5F),
    accent_dim: hexa(0x1E9E5F, 0.12),
    danger: hex(0xCE2C31),
    danger_dim: hexa(0xCE2C31, 0.10),
    doc: [
        hex(0xC4414E),
        hex(0x9A7011),
        hex(0x7554D6),
        hex(0x0D8296),
        hex(0xB65A1C),
        hex(0x5E7A14),
        hex(0x3F63D1),
        hex(0xAE3B93),
    ],
    doc_dim: [
        hexa(0xC4414E, 0.10),
        hexa(0x9A7011, 0.10),
        hexa(0x7554D6, 0.10),
        hexa(0x0D8296, 0.10),
        hexa(0xB65A1C, 0.10),
        hexa(0x5E7A14, 0.10),
        hexa(0x3F63D1, 0.10),
        hexa(0xAE3B93, 0.10),
    ],
    canvas_bg: hex(0xF0F1F4),
    canvas_dot: hex(0xD5D9E0),
    connector: hex(0xA9B0BC),
    scrim: hexa(0x17191F, 0.30),
    selection: hexa(0x1E9E5F, 0.18),
    search_hl: hexa(0x9A7011, 0.22),
    search_hl_active: hex(0x9A7011),
    shadow: 0.12,
};

static IS_DARK: AtomicBool = AtomicBool::new(true);

pub fn c() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn set_dark(dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
}

pub const SLOT_NAMES: [&str; 8] = [
    "coral", "amber", "violet", "cyan", "orange", "lime", "blue", "magenta",
];

impl Palette {
    /// A resolved tone as a colour; neutral is the connector grey.
    pub fn tone(&self, tone: crate::index::Tone) -> Color {
        match tone {
            crate::index::Tone::Slot(i) => self.doc[i % 8],
            crate::index::Tone::Neutral => self.connector,
            crate::index::Tone::Custom(v) => Color::new(v),
        }
    }
    pub fn tone_dim(&self, tone: crate::index::Tone) -> Color {
        match tone {
            crate::index::Tone::Slot(i) => self.doc_dim[i % 8],
            crate::index::Tone::Neutral => self.connector.with_a(30),
            crate::index::Tone::Custom(v) => Color::new(v).with_a(30),
        }
    }
    pub fn shadow(&self, k: f32) -> Color {
        Color::from_argb((255. * (self.shadow * k).min(1.)) as u8, 0, 0, 0)
    }
}

pub const TEXT_XS: f32 = 10.;
pub const TEXT_SM: f32 = 11.;
pub const TEXT_BASE: f32 = 13.;
pub const TEXT_MD: f32 = 15.;
pub const TEXT_LG: f32 = 18.;
pub const TEXT_XL: f32 = 24.;

/// A JSON Canvas colour: `"1"`–`"8"` → a palette slot. Hex colours aren't
/// drawn yet.
pub fn doc_index_compat(color: Option<&str>) -> Option<usize> {
    color
        .and_then(|c| c.parse::<usize>().ok())
        .filter(|i| (1..=8).contains(i))
        .map(|i| i - 1)
}

#[cfg(test)]
mod against_the_design {
    use super::*;

    /// ⚠️ `include_str!`, so a moved file is a compile error, not a skipped test.
    const CSS: &str = include_str!("../design/tokens.css");

    /// `--doc-N` and `--doc-N-dim` from one theme's block, as RGBA bytes.
    fn group(which: &str, n: usize) -> ([u8; 4], [u8; 4]) {
        let head = format!("[data-theme=\"{which}\"]");
        let body = CSS.split(&head).nth(1).expect("theme block");
        let body = &body[..body.find('}').expect("block end")];
        let value = |name: &str| {
            body.split(';')
                // a comment between two tokens rides at the head of the next one
                .filter_map(|kv| {
                    kv.rsplit("*/")
                        .next()?
                        .trim()
                        .strip_prefix("--")?
                        .split_once(':')
                })
                .find(|(k, _)| k.trim() == name)
                .map(|(_, v)| v.split("/*").next().unwrap().trim().to_string())
                .unwrap_or_else(|| panic!("{which}: no --{name}"))
        };
        let solid = value(&format!("doc-{n}"));
        let v = u32::from_str_radix(solid.trim_start_matches('#'), 16).unwrap();
        let rgb = [(v >> 16) as u8, (v >> 8) as u8, v as u8];
        let dim = value(&format!("doc-{n}-dim"));
        let parts: Vec<f32> = dim
            .trim_start_matches("rgba(")
            .trim_end_matches(')')
            .split(',')
            .map(|p| p.trim().parse().unwrap())
            .collect();
        (
            [rgb[0], rgb[1], rgb[2], 255],
            [
                parts[0] as u8,
                parts[1] as u8,
                parts[2] as u8,
                (parts[3] * 255.).round() as u8,
            ],
        )
    }

    fn bytes(c: Color) -> [u8; 4] {
        [c.r(), c.g(), c.b(), c.a()]
    }

    fn check(which: &str, p: &Palette) {
        for i in 0..8 {
            let (solid, dim) = group(which, i + 1);
            assert_eq!(bytes(p.doc[i]), solid, "{which} --doc-{}", i + 1);
            let got = bytes(p.doc_dim[i]);
            assert_eq!(got[..3], dim[..3], "{which} --doc-{}-dim", i + 1);
            assert!(
                got[3].abs_diff(dim[3]) <= 1,
                "{which} --doc-{}-dim alpha",
                i + 1
            );
        }
    }

    #[test]
    fn the_eight_group_colours_are_the_design() {
        check("dark", &DARK);
        check("light", &LIGHT);
    }
}
