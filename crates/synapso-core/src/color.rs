//! Colour as the vault and the canvas see it: HSV for the choosing, hex for
//! the storing, `peniko::Color` as the runtime type (the shell converts at its
//! boundary). design: the `choose colour` panel of the new-note dialog; what
//! gets stored is always plain RGB.

use peniko::Color;

/// Hue in degrees, saturation and value in `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    pub fn color(self) -> Color {
        let (h, s, v) = (self.h.rem_euclid(360.0) / 60.0, self.s, self.v);
        let c = v * s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = v - c;
        Color::new([r + m, g + m, b + m, 1.0])
    }

    pub fn of(color: Color) -> Self {
        let [r, g, b] = [
            color.components[0],
            color.components[1],
            color.components[2],
        ];
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let span = max - min;
        let h = if span <= f32::EPSILON {
            0.0
        } else if max == r {
            60.0 * (((g - b) / span) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / span + 2.0)
        } else {
            60.0 * ((r - g) / span + 4.0)
        };
        Self {
            h: h.rem_euclid(360.0),
            s: if max <= f32::EPSILON { 0.0 } else { span / max },
            v: max,
        }
    }

    /// The fully saturated version — the colour the hue rail points at.
    pub fn pure(self) -> Color {
        Self {
            h: self.h,
            s: 1.0,
            v: 1.0,
        }
        .color()
    }
}

pub fn to_hex(c: Color) -> String {
    let byte = |f: f32| (f.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X}",
        byte(c.components[0]),
        byte(c.components[1]),
        byte(c.components[2])
    )
}

/// `#RGB` and `#RRGGBB`, with or without the `#`.
pub fn from_hex(text: &str) -> Option<Color> {
    let hex = text.trim().trim_start_matches('#');
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()?;
    let channel = |a: u8, b: u8| f32::from(a * 16 + b) / 255.0;
    match digits.len() {
        3 => Some(Color::new([
            channel(digits[0], digits[0]),
            channel(digits[1], digits[1]),
            channel(digits[2], digits[2]),
            1.0,
        ])),
        6 => Some(Color::new([
            channel(digits[0], digits[1]),
            channel(digits[2], digits[3]),
            channel(digits[4], digits[5]),
            1.0,
        ])),
        _ => None,
    }
}

/// The six colours JSON Canvas names by number, in its order: red, orange,
/// yellow, green, cyan, purple. Fixed by the spec — unlike the note palette
/// these do not follow the theme, because the number in someone else's file
/// means a colour and not "slot N of my theme".
pub const CANVAS_PRESETS: [&str; 6] = [
    "#E5484D", "#E8913B", "#E3B341", "#4FD08A", "#56C8D8", "#A78BFA",
];

/// A JSON Canvas colour: `#RRGGBB`, or one of the presets `"1"`..`"6"`.
///
/// ⚠️ Reading the presets is not optional. `from_hex("3")` is `None`, so
/// without these six arms a canvas coloured in Obsidian opens with no colour
/// at all. Writing them is — we always write hex, which is unambiguous,
/// theme-independent and round-trips exactly.
pub fn canvas(value: &str) -> Option<Color> {
    match value.trim() {
        preset @ ("1" | "2" | "3" | "4" | "5" | "6") => {
            let i = preset.parse::<usize>().ok()? - 1;
            from_hex(CANVAS_PRESETS[i])
        }
        hex => from_hex(hex),
    }
}
