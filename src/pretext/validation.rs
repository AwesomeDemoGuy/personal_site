//! Browser-only test export; excluded from normal production builds.
use super::*;
use serde_json::json;
use wasm_bindgen::prelude::*;

/// Runs timed work within WASM; serialization and the JS/WASM boundary are excluded.
#[wasm_bindgen]
pub fn pretext_benchmark(
    text: &str,
    font: &str,
    prepare_iterations: usize,
    layout_iterations: usize,
) -> std::result::Result<String, JsValue> {
    let run = || -> Result<String> {
        if prepare_iterations == 0
            || layout_iterations == 0
            || prepare_iterations > 1000
            || layout_iterations > 100_000
        {
            return Err(LayoutError("invalid benchmark iterations".into()));
        }
        let clock = web_sys::window()
            .and_then(|w| w.performance())
            .ok_or_else(|| LayoutError("performance clock unavailable".into()))?;
        let mut engine =
            TextEngine::new(browser::BrowserBackend::new()?, browser::browser_profile());
        engine.set_locale(Some("en"));
        let start = clock.now();
        for _ in 0..prepare_iterations {
            engine.clear_cache();
            std::hint::black_box(engine.prepare_with_segments(
                text,
                font,
                PrepareOptions::default(),
            )?);
        }
        let preparation_ms = (clock.now() - start) / prepare_iterations as f64;
        let prepared = engine.prepare_with_segments(text, font, PrepareOptions::default())?;
        let mut checksum = 0;
        let start = clock.now();
        for i in 0..layout_iterations {
            checksum +=
                measure_line_stats(&prepared.geometry, 160.0 + (i % 320) as f64)?.line_count;
        }
        let warm_geometry_ms = (clock.now() - start) / layout_iterations as f64;
        let start = clock.now();
        for i in 0..layout_iterations {
            checksum += layout_with_lines(&prepared, 160.0 + (i % 320) as f64, 24.0)?.line_count;
        }
        let warm_lines_ms = (clock.now() - start) / layout_iterations as f64;
        Ok(json!({"preparationMs":preparation_ms,"warmGeometryMs":warm_geometry_ms,"warmLinesMs":warm_lines_ms,"checksum":checksum}).to_string())
    };
    run().map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn pretext_validate(
    text: &str,
    font: &str,
    options: &str,
    widths: &str,
) -> std::result::Result<String, JsValue> {
    let run = || -> Result<String> {
        let options: PrepareOptions =
            serde_json::from_str(options).map_err(|e| LayoutError(e.to_string()))?;
        let widths: Vec<f64> =
            serde_json::from_str(widths).map_err(|e| LayoutError(e.to_string()))?;
        let mut engine =
            TextEngine::new(browser::BrowserBackend::new()?, browser::browser_profile());
        engine.set_locale(Some("en"));
        let p = engine.prepare_with_segments(text, font, options)?;
        let m = &p.geometry.measured;
        let snapshot = json!({
            "segments":p.segments,"kinds":p.kinds,"segLevels":p.segment_levels,
            "widths":m.iter().map(|s|s.width).collect::<Vec<_>>(),
            "lineEndFitAdvances":m.iter().map(|s|s.fit).collect::<Vec<_>>(),
            "lineEndPaintAdvances":m.iter().map(|s|s.paint).collect::<Vec<_>>(),
            "breakableFitAdvances":m.iter().map(|s|&s.advances).collect::<Vec<_>>(),
            "breakablePreferredBreaks":m.iter().map(|s|if s.preferred.is_empty(){None}else{Some(&s.preferred)}).collect::<Vec<_>>(),
            "spacingGraphemeCounts":if options.letter_spacing==0.0 { Vec::<usize>::new() } else { m.iter().map(|s|s.spacing_count).collect() },
            "discretionaryHyphenWidth":p.geometry.hyphen_width,"tabStopAdvance":p.geometry.tab_stop,
        });
        let mut layouts = Vec::new();
        for width in widths {
            layouts.push(json!({"width":width,"batch":layout_with_lines(&p,width,24.0)?,"stats":measure_line_stats(&p.geometry,width)?,"layout":layout(&p.geometry,width,24.0)?}));
        }
        let natural = measure_natural_width(&p.geometry)?;
        // Cache disposal cannot mutate already prepared handles.
        engine.clear_cache();
        engine.set_locale(Some("en"));
        let after = engine.prepare_with_segments(text, font, options)?;
        if p.segments != after.segments || natural != measure_natural_width(&after.geometry)? {
            return Err(LayoutError("cache invalidation changed preparation".into()));
        }
        Ok(json!({"prepared":snapshot,"layouts":layouts,"naturalWidth":natural}).to_string())
    };
    run().map_err(|e| JsValue::from_str(&e.to_string()))
}
