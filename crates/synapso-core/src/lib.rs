//! The model, the parsers, the geometry — no shell.
//!
//! This crate exists so the coupling the Floem TRD (docs/floem-migration-trd.md)
//! only counted becomes a compiler error: nothing in here may depend on a UI toolkit
//! or any other windowing/UI crate, so any shell can sit on the same logic.
//! Geometry is `kurbo` and colour is
//! `peniko`, the vocabulary Floem already speaks.

pub mod bench;
pub mod board;
pub mod card;
pub mod color;
pub mod doc;
pub mod find;
pub mod history;
pub mod i18n;
pub mod inline;
pub mod sheet;
pub mod table;
pub mod theme;
pub mod vault;

// Re-exported so the shell speaks the SAME kurbo/peniko this crate was built
// against — a shell-side `kurbo = "0.13"` could resolve to a different point
// release and the types would silently not unify.
pub use kurbo;
pub use peniko;
