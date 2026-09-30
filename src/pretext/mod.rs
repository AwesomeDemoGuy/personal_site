//! Rust translation of the frozen Pretext 0.0.8 algorithms.
//! See tests/fixtures/pretext/LICENSE for upstream attribution.
//!
//! A [`TextEngine`] owns its backend, locale, and bounded measurement caches.
//! Preparation invokes the backend; subsequent layout and materialization are
//! pure Rust and keep the prepared handle unchanged. Browser implementations
//! are available with `hydrate`; native callers provide a [`TextBackend`].
//!
//! Widths use `f64`. Zero and infinite widths are supported. Negative widths,
//! NaN, invalid cursors, and non-finite letter spacing return [`LayoutError`].
//! Segment/grapheme cursors avoid exposing UTF-8 or UTF-16 offsets. Bidi levels
//! retain the oracle's metadata behavior and do not implement full shaping.
mod analysis;
mod bidi;
#[cfg(feature = "hydrate")]
pub mod browser;
mod data;
mod line_break;
mod line_text;
mod measurement;
mod types;
mod unicode;
#[cfg(all(feature = "hydrate", feature = "pretext-validation"))]
mod validation;
pub use line_break::{
    layout, layout_next_line_range, measure_line_stats, measure_natural_width, walk_line_ranges,
};
pub use line_text::{layout_next_line, layout_with_lines, materialize_line_range};
pub use measurement::TextEngine;
pub use types::*;
