pub mod app;
pub mod components;
pub mod interop;
pub mod pages;

#[cfg(feature = "ssr")]
pub mod db;

pub mod models;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
    // Flow all prose text on every page around the draggable photo. Safe to
    // call here: the manager defers its first scan to an animation frame and
    // watches the content region for route changes.
    crate::interop::setup_all_text_flow();
}
