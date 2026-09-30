use super::super::{LayoutError, Result, TextBackend, WordSegment};
use super::{error, segmentation::*};
use js_sys::Intl::Segmenter;
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlElement};

pub struct BrowserBackend {
    canvas: CanvasRenderingContext2d,
    word_segmenters: HashMap<Option<String>, Segmenter>,
    grapheme_segmenter: Segmenter,
    grapheme_cache: HashMap<String, Vec<String>>,
}
impl BrowserBackend {
    pub fn new() -> Result<Self> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| LayoutError("document unavailable".into()))?;
        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(error)?
            .dyn_into()
            .map_err(error)?;
        let ctx: CanvasRenderingContext2d = canvas
            .get_context("2d")
            .map_err(error)?
            .ok_or_else(|| LayoutError("Canvas 2D unavailable".into()))?
            .dyn_into()
            .map_err(error)?;
        Ok(Self {
            canvas: ctx,
            word_segmenters: HashMap::new(),
            grapheme_segmenter: segmenter(None, "grapheme")?,
            grapheme_cache: HashMap::new(),
        })
    }
}
impl TextBackend for BrowserBackend {
    fn words(&mut self, text: &str, locale: Option<&str>) -> Result<Vec<WordSegment>> {
        let key = locale.map(str::to_owned);
        if !self.word_segmenters.contains_key(&key) {
            self.word_segmenters
                .insert(key.clone(), segmenter(locale, "word")?);
        }
        segments(&self.word_segmenters[&key], text)
    }
    fn graphemes(&mut self, text: &str) -> Result<Vec<String>> {
        if let Some(cached) = self.grapheme_cache.get(text) {
            return Ok(cached.clone());
        }
        let graphemes: Vec<String> = segments(&self.grapheme_segmenter, text)?
            .into_iter()
            .map(|s| s.text)
            .collect();
        if self.grapheme_cache.len() >= 8192 {
            self.grapheme_cache.clear();
        }
        self.grapheme_cache
            .insert(text.to_owned(), graphemes.clone());
        Ok(graphemes)
    }
    fn measure(&mut self, font: &str, text: &str) -> Result<f64> {
        self.canvas.set_font(font);
        self.canvas
            .measure_text(text)
            .map(|m| m.width())
            .map_err(error)
    }
    fn emoji_correction(&mut self, font: &str) -> Result<f64> {
        // Canvas accepts CSS shorthand such as "16px/24px Arial" too.
        let size = font
            .find("px")
            .and_then(|end| {
                let number = font[..end]
                    .trim_end()
                    .rsplit(|c: char| !c.is_ascii_digit() && c != '.')
                    .next()?;
                number.parse::<f64>().ok()
            })
            .unwrap_or(16.0);
        let canvas_width = self.measure(font, "😀")?;
        if canvas_width <= size + 0.5 {
            return Ok(0.0);
        }
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| LayoutError("document unavailable".into()))?;
        let Some(body) = document.body() else {
            return Ok(0.0);
        };
        let span: HtmlElement = document
            .create_element("span")
            .map_err(error)?
            .dyn_into()
            .map_err(error)?;
        span.set_text_content(Some("😀"));
        let style = span.style();
        style.set_property("font", font).map_err(error)?;
        style
            .set_property("display", "inline-block")
            .map_err(error)?;
        style.set_property("visibility", "hidden").map_err(error)?;
        style.set_property("position", "absolute").map_err(error)?;
        body.append_child(&span).map_err(error)?;
        let dom_width = span.get_bounding_client_rect().width();
        span.remove();
        Ok(if canvas_width - dom_width > 0.5 {
            canvas_width - dom_width
        } else {
            0.0
        })
    }
    fn clear_cache(&mut self) {
        self.word_segmenters.clear();
        self.grapheme_cache.clear();
    }
}
