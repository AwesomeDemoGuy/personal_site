//! JavaScript interop layer via `wasm-bindgen`.
//!
//! Browser behavior for draggable photos, certificate icons, and blog images
//! lives in `public/js/interop.js`:
//!   1. `make_draggable` — drag-to-move behavior for the profile photo.
//!   2. `make_floating_draggable` — detaches nested images when dragged.
//!   3. `setup_all_text_flow` — uses Pretext to reflow prose and Markdown around
//!      circular photos and rectangular blog images. It discovers text blocks
//!      under the main content region and re-scans on client-side route changes.
//!
//! `interop.js` loads pretext itself via a dynamic `import('/js/pretext.js')`,
//! so the (per-drag-frame) layout hot path stays in JS and never crosses the
//! wasm boundary.
//!
//! The bindings only compile under the `hydrate` feature where wasm-bindgen
//! and web-sys are available.

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

        /// Like `make_draggable`, but for an element nested inside other content
        /// (e.g. the certificate icon inside its card's link). On the first real
        /// drag it detaches to `<body>` as a free, page-level element and stops
        /// behaving as a hyperlink; a plain click is left untouched.
        #[wasm_bindgen(js_name = makeFloatingDraggable)]
        pub fn make_floating_draggable(element: &web_sys::HtmlElement);

        /// Flow all prose text on every page around the photo using pretext,
        /// re-running on `photomove`, resize, and route changes. Call once.
        #[wasm_bindgen(js_name = setupAllTextFlow)]
        pub fn setup_all_text_flow();
    }
}

#[cfg(feature = "hydrate")]
pub use bindings::*;
