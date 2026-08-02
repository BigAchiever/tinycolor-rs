//! TinyColor, ported to Rust.
//!
//! Upstream: https://github.com/bgrins/TinyColor
//! Pinned commit: see `upstream/PINNED_COMMIT`.
//!
//! Layering, bottom-up:
//!   `jsnum`     — JavaScript numeric semantics (parseFloat/parseInt/Math.round)
//!   `names`     — generated CSS colour-name table
//!   `luminance` — generated sRGB linearisation table (libm-independent)
//!   `convert`   — colour-space conversions, 1:1 with upstream
//!   `parse`     — CSS string parsing and `inputToRGB`
//!   `color`     — the `TinyColor` type, modifications, combinations, statics
//!   `rpc`       — JSON bridge used by the wasm and cli transports
//!
//! The port contains no `unsafe`; this is enforced at compile time by
//! `unsafe_code = "forbid"` in Cargo.toml rather than merely asserted.

pub mod color;
pub mod convert;
pub mod jsnum;
pub mod luminance;
pub mod names;
pub mod parse;
pub mod rpc;

pub use color::TinyColor;
pub use convert::{Hsl, Hsv, Rgb, Unit};
pub use parse::{ColorObj, Input};

/// Number of CSS colour names carried over from upstream.
pub fn name_count() -> usize {
    names::NAMES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_table_loaded() {
        assert_eq!(name_count(), 149);
        assert!(names::NAMES.iter().any(|(k, v)| *k == "red" && *v == "f00"));
    }

    #[test]
    fn hex_names_flip_is_last_wins() {
        // Upstream builds hex->name by flipping name->hex, so colliding hexes
        // resolve to the *last* name in source order (see DECISIONS.md D-004).
        let hits: Vec<_> = names::HEX_NAMES
            .iter()
            .filter(|(hex, _)| *hex == "0ff")
            .map(|(_, name)| *name)
            .collect();
        assert!(hits.len() > 1, "expected aqua/cyan collision, got {hits:?}");
    }
}
