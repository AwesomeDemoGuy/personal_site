use super::{
    super::{Result, WordSegment},
    error,
};
use js_sys::{
    Array, Function,
    Intl::{SegmentData, Segmenter},
    Object, Reflect,
};
use wasm_bindgen::{JsCast, JsValue};

pub(crate) fn segmenter(locale: Option<&str>, granularity: &str) -> Result<Segmenter> {
    let locales = Array::new();
    if let Some(locale) = locale {
        locales.push(&JsValue::from_str(locale));
    }
    let options = Object::new();
    Reflect::set(&options, &"granularity".into(), &granularity.into()).map_err(error)?;
    let intl = Reflect::get(&js_sys::global(), &"Intl".into()).map_err(error)?;
    let constructor: Function = Reflect::get(&intl, &"Segmenter".into())
        .map_err(error)?
        .dyn_into()
        .map_err(error)?;
    let args = Array::new();
    args.push(&locales);
    args.push(&options);
    Reflect::construct(&constructor, &args)
        .map(|v| v.unchecked_into())
        .map_err(error)
}
pub(crate) fn segments(segmenter: &Segmenter, text: &str) -> Result<Vec<WordSegment>> {
    let items = segmenter.segment(text);
    let iterator = js_sys::try_iter(&items)
        .map_err(error)?
        .ok_or_else(|| super::super::LayoutError("segmenter is not iterable".into()))?;
    iterator
        .map(|value| {
            let data: SegmentData = value.map_err(error)?.unchecked_into();
            Ok(WordSegment {
                text: String::from(data.segment()),
                word_like: data.is_word_like().unwrap_or(false),
            })
        })
        .collect()
}
