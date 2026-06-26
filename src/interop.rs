//! JavaScript interop layer via `wasm-bindgen`.
//!
//! All browser-side behavior for the profile photo lives in
//! `public/js/interop.js`:
//!   1. `make_draggable` — drag-to-move behavior for the profile photo.
//!   2. `setup_text_flow` — uses the pretext layout library to reflow a
//!      paragraph around the photo's current rectangle, so the text is
//!      displaced as the photo is dragged (pretext use case #2).
//!
//! `interop.js` loads pretext itself via a dynamic `import('/js/pretext.js')`,
//! so the (per-drag-frame) layout hot path stays in JS and never crosses the
//! wasm boundary.
//!
//! Both are no-ops on the server build; the bindings only compile under the
//! `hydrate` feature where wasm-bindgen and web-sys are available.

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

        /// Flow the text content of `element` around the photo using pretext,
        /// re-running on `photomove` and window resize.
        #[wasm_bindgen(js_name = setupTextFlow)]
        pub fn setup_text_flow(element: &web_sys::HtmlElement);
    }
}

#[cfg(feature = "hydrate")]
pub use bindings::*;
