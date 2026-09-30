mod canvas;
mod profile;
mod segmentation;
pub use canvas::BrowserBackend;
pub use profile::browser_profile;

pub(crate) fn error(value: impl Into<wasm_bindgen::JsValue>) -> super::LayoutError {
    let value = value.into();
    super::LayoutError(format!("browser text API: {value:?}"))
}
