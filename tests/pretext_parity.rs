use personal_site::pretext::*;
use serde_json::Value;
use std::{collections::HashMap, io::Read};
use unicode_segmentation::UnicodeSegmentation;

struct RecordedBackend {
    words: Vec<WordSegment>,
    graphemes: HashMap<String, Vec<String>>,
    widths: HashMap<String, f64>,
}
impl TextBackend for RecordedBackend {
    fn words(&mut self, _: &str, _: Option<&str>) -> Result<Vec<WordSegment>> {
        Ok(self.words.clone())
    }
    fn graphemes(&mut self, text: &str) -> Result<Vec<String>> {
        Ok(self
            .graphemes
            .get(text)
            .cloned()
            .unwrap_or_else(|| text.graphemes(true).map(str::to_owned).collect()))
    }
    fn measure(&mut self, _: &str, text: &str) -> Result<f64> {
        self.widths
            .get(text)
            .copied()
            .ok_or_else(|| LayoutError(format!("unrecorded measurement {text:?}")))
    }
}
fn cases() -> Vec<Value> {
    let mut decoder =
        flate2::read::GzDecoder::new(&include_bytes!("fixtures/pretext/cases.json.gz")[..]);
    let mut json = String::new();
    decoder.read_to_string(&mut json).unwrap();
    serde_json::from_str(&json).unwrap()
}
fn close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "{label}: actual {actual}, expected {expected}"
    );
}
#[test]
fn frozen_oracle_preparation_and_incremental_layout() {
    for (index, case) in cases().iter().enumerate() {
        let backend = RecordedBackend {
            words: serde_json::from_value(case["wordSegments"].clone()).unwrap(),
            graphemes: serde_json::from_value(case["graphemes"].clone()).unwrap(),
            widths: serde_json::from_value(case["measurements"].clone()).unwrap(),
        };
        let profile = serde_json::from_value(case["profile"].clone()).unwrap();
        let options = serde_json::from_value(case["options"].clone()).unwrap();
        let text = case["text"].as_str().unwrap();
        let mut engine = TextEngine::new(backend, profile);
        let prepared = engine
            .prepare_with_segments(text, "16px Test", options)
            .unwrap_or_else(|e| panic!("case {index} {text:?}: {e}"));
        assert_eq!(
            serde_json::to_value(prepared.segments()).unwrap(),
            case["prepared"]["segments"],
            "segments case {index} {text:?} options {}",
            case["options"]
        );
        assert_eq!(
            serde_json::to_value(prepared.kinds()).unwrap(),
            case["prepared"]["kinds"],
            "kinds case {index}"
        );
        let expected_levels = &case["prepared"]["segLevels"];
        if expected_levels.is_null() {
            assert!(prepared.segment_levels().is_none());
        } else {
            let levels: Vec<i8> = (0..prepared.segments().len())
                .map(|i| expected_levels[i.to_string()].as_i64().unwrap() as i8)
                .collect();
            assert_eq!(
                prepared.segment_levels().unwrap(),
                levels.as_slice(),
                "bidi case {index}"
            );
        }
        close(
            measure_natural_width(prepared.geometry()).unwrap(),
            case["naturalWidth"].as_f64().unwrap(),
            &format!("natural width case {index}"),
        );
        for entry in case["layouts"].as_array().unwrap() {
            let width = entry["width"].as_f64().unwrap();
            let expected: Vec<LayoutLine> = serde_json::from_value(entry["lines"].clone()).unwrap();
            let actual = layout_with_lines(&prepared, width, 24.0).unwrap();
            let expected_batch: LayoutWithLines =
                serde_json::from_value(entry["batch"].clone()).unwrap();
            assert_eq!(actual, expected_batch, "batch case {index} width {width}");
            let measured_layout = layout(prepared.geometry(), width, 24.0).unwrap();
            let expected_layout: Layout = serde_json::from_value(entry["layout"].clone()).unwrap();
            assert_eq!(
                measured_layout, expected_layout,
                "geometry-only layout case {index}"
            );
            assert_eq!(
                actual.lines.len(),
                expected.len(),
                "line count case {index} {text:?} width {width} options {}",
                case["options"]
            );
            for (a, e) in actual.lines.iter().zip(&expected) {
                assert_eq!(
                    (&a.text, a.start, a.end),
                    (&e.text, e.start, e.end),
                    "line case {index} width {width}"
                );
                close(
                    a.width,
                    e.width,
                    &format!("line width case {index} width {width}"),
                );
            }
            let stats = measure_line_stats(prepared.geometry(), width).unwrap();
            assert_eq!(stats.line_count, expected.len());
            close(
                stats.max_line_width,
                entry["stats"]["maxLineWidth"].as_f64().unwrap(),
                &format!("stats case {index} width {width}"),
            );
        }
    }
}
#[test]
fn invalid_inputs_return_errors_and_empty_text_has_zero_height() {
    let case = &cases()[0];
    let mut engine = TextEngine::new(
        RecordedBackend {
            words: vec![],
            graphemes: HashMap::new(),
            widths: serde_json::from_value(case["measurements"].clone()).unwrap(),
        },
        EngineProfile::default(),
    );
    let p = engine
        .prepare("", "16px Test", PrepareOptions::default())
        .unwrap();
    assert_eq!(layout(&p, 100.0, 24.0).unwrap().height, 0.0);
    assert!(layout(&p, f64::NAN, 24.0).is_err());
    assert!(layout(&p, -1.0, 24.0).is_err());
    assert!(layout_next_line_range(
        &p,
        LayoutCursor {
            segment_index: 1,
            grapheme_index: 0
        },
        100.0
    )
    .is_err());
}
