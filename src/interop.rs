//! JavaScript interop layer via `wasm-bindgen`.
//!
//! Temporary pointer-drag bridge. Text preparation and flow run in Rust.
//! Component cleanup releases listeners; Leptos handles navigation resets.

#[cfg(feature = "hydrate")]
mod bindings {
    use wasm_bindgen::prelude::*;

    // Local interop module: see `public/js/interop.js`.
    #[wasm_bindgen(module = "/public/js/interop.js")]
    extern "C" {
        /// Make the given element draggable by pointer. Dragging dispatches a
        /// `photomove` event that the text-flow layout listens for.
        #[wasm_bindgen(js_name = makeDraggable)]
        pub fn make_draggable(element: &web_sys::HtmlElement);

        #[wasm_bindgen(js_name = resetAllDraggables)]
        pub fn reset_draggables();

        #[wasm_bindgen(js_name = disposeDraggable)]
        pub fn dispose_draggable(element: &web_sys::HtmlElement);
    }
}

#[cfg(feature = "hydrate")]
pub use bindings::*;
