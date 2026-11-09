//! The design's 16×16 stroke icons, verbatim from `Workspace.dc.html`.
use freya::prelude::*;

macro_rules! icon {
    ($(#[$meta:meta])* $name:ident, $body:literal) => {
        $(#[$meta])*
        pub const $name: (&str, &[u8]) = (
            stringify!($name),
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">"#,
                $body,
                "</svg>"
            )
            .as_bytes(),
        );
    };
}

icon!(
    PENCIL,
    r#"<path d="M10.2 2.6 L13.4 5.8 L6.2 13 H3 V9.8 Z"/><path d="M9 3.8 L12.2 7"/>"#
);
icon!(
    TRASH,
    r#"<path d="M2.8 4.4h10.4"/><path d="M6.4 4.4V3.2h3.2v1.2"/><path d="M4.2 4.4l.6 8.2a.9.9 0 0 0 .9.8h4.6a.9.9 0 0 0 .9-.8l.6-8.2"/><path d="M6.8 6.8v4"/><path d="M9.2 6.8v4"/>"#
);
icon!(
    HEADING,
    r#"<path d="M4 3.5v9"/><path d="M12 3.5v9"/><path d="M4 8h8"/>"#
);
icon!(
    FORMAT,
    r#"<path d="M3.2 12.8 8 3.4l4.8 9.4"/><path d="M5.2 9.2h5.6"/>"#
);
icon!(
    LINK,
    r#"<path d="M9.4 6.6a2.9 2.9 0 0 1 0 4.1l-1.6 1.6a2.9 2.9 0 0 1-4.1-4.1l.9-.9"/><path d="M6.6 9.4a2.9 2.9 0 0 1 0-4.1l1.6-1.6a2.9 2.9 0 0 1 4.1 4.1l-.9.9"/>"#
);
icon!(
    BLOCKS,
    r#"<rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M6.2 6.9 4.6 8.5l1.6 1.6"/><path d="M9.8 6.9l1.6 1.6-1.6 1.6"/>"#
);
icon!(
    LISTS,
    r#"<path d="M6 4.2h8"/><path d="M6 8h8"/><path d="M6 11.8h8"/><path d="M3 4.2h.01"/><path d="M3 8h.01"/><path d="M3 11.8h.01"/>"#
);
icon!(
    TABLE,
    r#"<rect x="2" y="3" width="12" height="10" rx="1.5"/><path d="M2 6.4h12"/><path d="M6.4 6.4V13"/>"#
);

icon!(
    T_SELECT,
    r#"<path d="M4 2 L12 8 L8 9 L10 13 L8.5 14 L6.5 10 L4 12 Z"/>"#
);
icon!(
    T_RECT,
    r#"<rect x="2.5" y="3.5" width="11" height="9" rx="1"/>"#
);
icon!(T_ELLIPSE, r#"<ellipse cx="8" cy="8" rx="5.5" ry="4.5"/>"#);
icon!(T_TRIANGLE, r#"<path d="M8 3 L14 13 H2 Z"/>"#);
icon!(T_ARROW, r#"<path d="M3 13 L12 4 M12 4 H7.5 M12 4 V8.5"/>"#);
icon!(T_LINE, r#"<path d="M3 13 L13 3"/>"#);
icon!(
    T_PEN,
    r#"<path d="M10.2 2.6 L13.4 5.8 L6.2 13 H3 V9.8 Z M9 3.8 L12.2 7"/>"#
);
icon!(T_TEXT, r#"<path d="M4 4 H12 M8 4 V13"/>"#);
icon!(
    T_FRAME,
    r#"<path d="M5 2 V14 M11 2 V14 M2 5 H14 M2 11 H14"/>"#
);

// The style bar's own (ADR-004), drawn as the Floem shell drew them
// (becf30f:src/icons.rs). Straight is `T_LINE`; the shapes are the rail's.
icon!(S_CURVE, r#"<path d="M2.5 12 C6.5 12 9.5 4 13.5 4"/>"#);
icon!(S_ELBOW, r#"<path d="M2.5 12 H8 V4 H13.5"/>"#);
// ⚠️ two long segments, not dots: dots read as an ellipsis, a menu that
// opens. And a 3px gap: the round caps close anything narrower
icon!(S_DASH, r#"<path d="M2.5 8 H6.5 M9.5 8 H13.5"/>"#);
icon!(
    S_HEAD_END,
    r#"<path d="M2.5 8 H13.5 M9 4.5 L13.5 8 L9 11.5"/>"#
);
icon!(
    S_HEAD_START,
    r#"<path d="M2.5 8 H13.5 M7 4.5 L2.5 8 L7 11.5"/>"#
);
// ⚠️ two of the three lines are short, not one: with a single short line the
// three read as the same picture
icon!(
    S_ALIGN_LEFT,
    r#"<path d="M2.5 3.5 H9 M2.5 8 H13.5 M2.5 12.5 H9"/>"#
);
icon!(
    S_ALIGN_CENTER,
    r#"<path d="M4.75 3.5 H11.25 M2.5 8 H13.5 M4.75 12.5 H11.25"/>"#
);
icon!(
    S_ALIGN_RIGHT,
    r#"<path d="M7 3.5 H13.5 M2.5 8 H13.5 M7 12.5 H13.5"/>"#
);
// a thickness key *is* its thickness: `vault::STROKES`
icon!(
    S_WEIGHT_THIN,
    r#"<path stroke-width="1.5" d="M2.5 8 H13.5"/>"#
);
icon!(
    S_WEIGHT_MEDIUM,
    r#"<path stroke-width="3" d="M2.5 8 H13.5"/>"#
);
icon!(
    S_WEIGHT_THICK,
    r#"<path stroke-width="5" d="M2.5 8 H13.5"/>"#
);

pub fn icon(src: (&'static str, &'static [u8]), size: f32, color: Color) -> SvgViewer {
    SvgViewer::new(src)
        .width(Size::px(size))
        .height(Size::px(size))
        .color(color)
        .stroke(color)
}

// The design's `⌕`, which JetBrains Mono has no glyph for (other fonts stand
// in on macOS; Windows draws tofu): a lens with its handle down to the
// LEFT, as every font draws U+2315 and so as the mock shows it.
icon!(
    // ⚠️ for the `⌕` labels in note.rs, graph.rs and overlays.rs to move to
    #[allow(dead_code)]
    SEARCH,
    r#"<circle cx="9.2" cy="6.8" r="4"/><path d="M6.3 9.7 2.5 13.5"/>"#
);
icon!(FOLDER, r#"<path d="M2 4h4.2l1.3 1.6H14v6.9H2z"/>"#);
icon!(
    FOLDER_PLUS,
    r#"<path d="M2 4h4.2l1.3 1.6H14v6.9H2z"/><path d="M8 7.3v3.6M6.2 9.1h3.6"/>"#
);
icon!(
    FILE_PLUS,
    r#"<path d="M9 2.5H4.5v11h7V5z"/><path d="M9 2.5V5h2.5"/><path d="M8 7.5v4M6 9.5h4"/>"#
);
icon!(
    GRAPH,
    r#"<circle cx="4" cy="4" r="2"/><circle cx="12" cy="6" r="2"/><circle cx="7" cy="12.5" r="2"/><path d="M5.8 4.6l4.3 1"/><path d="M4.7 5.9l1.6 4.7"/><path d="M10.8 7.6 8.3 11"/>"#
);
icon!(
    CLOSE,
    r#"<path stroke-width="2.6" d="M3 3l10 10M13 3 3 13"/>"#
);
icon!(
    DROP,
    r#"<path d="M8 2.5v8"/><path d="M4.8 7.4 8 10.6l3.2-3.2"/><path d="M2.5 11.5v2h11v-2"/>"#
);
// The modifier key: the cloverleaf on the Mac, the caret (Ctrl) elsewhere.
icon!(
    MOD_MAC,
    r#"<path d="M6 6h4v4H6z M6 6V4.5A1.5 1.5 0 1 0 4.5 6H6 M10 6V4.5A1.5 1.5 0 1 1 11.5 6H10 M10 10v1.5a1.5 1.5 0 1 0 1.5-1.5H10 M6 10v1.5A1.5 1.5 0 1 1 4.5 10H6"/>"#
);
icon!(MOD_WIN, r#"<path d="M4 10 8 5.5l4 4.5"/>"#);
// The Windows bar's own buttons, drawn the way the system draws them.
icon!(WIN_MIN, r#"<path stroke-width="1.6" d="M2 8h12"/>"#);
icon!(WIN_MAX, r#"<path stroke-width="1.7" d="M3 3h10v10H3z"/>"#);
icon!(
    WIN_CLOSE,
    r#"<path stroke-width="1.5" d="M2.5 2.5l11 11M13.5 2.5l-11 11"/>"#
);
