use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WhiteSpace {
    #[default]
    Normal,
    PreWrap,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WordBreak {
    #[default]
    Normal,
    KeepAll,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareOptions {
    pub white_space: WhiteSpace,
    pub word_break: WordBreak,
    pub letter_spacing: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentKind {
    Text,
    Space,
    PreservedSpace,
    Tab,
    ZeroWidthBreak,
    SoftHyphen,
    Glue,
    HardBreak,
}
impl SegmentKind {
    pub(crate) fn consumes_at_start(self) -> bool {
        matches!(self, Self::Space | Self::ZeroWidthBreak | Self::SoftHyphen)
    }
    pub(crate) fn breaks_after(self) -> bool {
        matches!(
            self,
            Self::Space
                | Self::PreservedSpace
                | Self::Tab
                | Self::ZeroWidthBreak
                | Self::SoftHyphen
        )
    }
    pub(crate) fn boundary(self) -> bool {
        matches!(
            self,
            Self::Space | Self::PreservedSpace | Self::ZeroWidthBreak | Self::HardBreak
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineProfile {
    pub line_fit_epsilon: f64,
    #[serde(rename = "carryCJKAfterClosingQuote")]
    pub carry_cjk_after_closing_quote: bool,
    pub break_keep_all_after_punctuation: bool,
    pub prefer_prefix_widths_for_breakable_runs: bool,
    pub prefer_early_soft_hyphen_break: bool,
}
impl Default for EngineProfile {
    fn default() -> Self {
        Self {
            line_fit_epsilon: 0.005,
            carry_cjk_after_closing_quote: false,
            break_keep_all_after_punctuation: true,
            prefer_prefix_widths_for_breakable_runs: false,
            prefer_early_soft_hyphen_break: false,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutCursor {
    pub segment_index: usize,
    pub grapheme_index: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineRange {
    pub width: f64,
    pub start: LayoutCursor,
    pub end: LayoutCursor,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutLine {
    pub text: String,
    pub width: f64,
    pub start: LayoutCursor,
    pub end: LayoutCursor,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStats {
    pub line_count: usize,
    pub max_line_width: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub line_count: usize,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutWithLines {
    pub line_count: usize,
    pub height: f64,
    pub lines: Vec<LayoutLine>,
}

#[derive(Clone, Debug)]
pub(crate) struct MeasuredSegment {
    pub width: f64,
    pub fit: f64,
    pub paint: f64,
    pub kind: SegmentKind,
    pub advances: Option<Vec<f64>>,
    pub preferred: Vec<usize>,
    pub spacing_count: usize,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chunk {
    pub start: usize,
    pub end: usize,
    pub consumed_end: usize,
}
/// Immutable geometry. Font/locale changes require preparing a new handle.
#[derive(Clone, Debug)]
pub struct PreparedText {
    pub(crate) measured: Vec<MeasuredSegment>,
    pub(crate) chunks: Vec<Chunk>,
    pub(crate) letter_spacing: f64,
    pub(crate) hyphen_width: f64,
    pub(crate) tab_stop: f64,
    pub(crate) profile: EngineProfile,
    pub(crate) simple: bool,
}
/// Text-bearing preparation required by materialization APIs.
#[derive(Clone, Debug)]
pub struct PreparedTextWithSegments {
    pub(crate) geometry: PreparedText,
    pub(crate) segments: Vec<String>,
    pub(crate) kinds: Vec<SegmentKind>,
    pub(crate) segment_levels: Option<Vec<i8>>,
    pub(crate) graphemes: Vec<Vec<String>>,
}
impl PreparedTextWithSegments {
    pub fn geometry(&self) -> &PreparedText {
        &self.geometry
    }
    pub fn segments(&self) -> &[String] {
        &self.segments
    }
    pub fn kinds(&self) -> &[SegmentKind] {
        &self.kinds
    }
    /// Original Pretext metadata; this is not a full bidi shaping implementation.
    pub fn segment_levels(&self) -> Option<&[i8]> {
        self.segment_levels.as_deref()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutError(pub String);
impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for LayoutError {}
pub type Result<T> = std::result::Result<T, LayoutError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordSegment {
    pub text: String,
    pub word_like: bool,
}
/// Expensive browser services are used during preparation, never line walking.
pub trait TextBackend {
    fn words(&mut self, text: &str, locale: Option<&str>) -> Result<Vec<WordSegment>>;
    fn graphemes(&mut self, text: &str) -> Result<Vec<String>>;
    fn measure(&mut self, font: &str, text: &str) -> Result<f64>;
    fn emoji_correction(&mut self, _font: &str) -> Result<f64> {
        Ok(0.0)
    }
    fn clear_cache(&mut self) {}
}
