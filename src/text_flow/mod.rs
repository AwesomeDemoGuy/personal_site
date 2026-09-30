pub mod chips;
pub mod geometry;
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedLine {
    pub text: String,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FlowLayout {
    pub lines: Vec<PositionedLine>,
    pub height: f64,
    pub line_height: f64,
}
#[cfg(feature = "hydrate")]
pub mod controller;
