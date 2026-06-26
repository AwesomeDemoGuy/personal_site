//! JavaScript interop layer via `wasm-bindgen`.
//!
//! All browser-side behavior for the profile photo lives in
//! `public/js/interop.js`:
//!   1. `make_draggable` — drag-to-move behavior for the profile photo.
//!   2. `setup_all_text_flow` — uses the pretext layout library to reflow ALL
//!      prose text on every page around the photo's circular shape, so the text
//!      is displaced as the photo is dragged (pretext use case #2). It
//!      auto-discovers text blocks under the main content region and re-scans
//!      on client-side route changes.
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

        /// Flow all prose text on every page around the photo using pretext,
        /// re-running on `photomove`, resize, and route changes. Call once.
        #[wasm_bindgen(js_name = setupAllTextFlow)]
        pub fn setup_all_text_flow();
    }
}

#[cfg(feature = "hydrate")]
pub use bindings::*;
