use leptos::prelude::*;

/// A photo placed near the top-center of the page that the visitor can drag
/// around. The drag behavior is implemented in JavaScript and attached via
/// `wasm-bindgen` interop once the node is mounted in the browser.
#[component]
pub fn DraggablePhoto(
    /// Image source path (served from the `public/assets` dir).
    #[prop(into)]
    src: String,
    /// Accessible alt text.
    #[prop(into)]
    alt: String,
) -> impl IntoView {
    let node_ref = NodeRef::<leptos::html::Img>::new();

    // After hydration, hand the element to the JS interop layer so it becomes
    // draggable. This only runs in the browser (the `hydrate` build).
    #[cfg(feature = "hydrate")]
    {
        use leptos::wasm_bindgen::JsCast;
        node_ref.on_load(move |el| {
            let element: web_sys::HtmlElement = el.unchecked_into();
            crate::interop::make_draggable(&element);
        });
    }

    view! {
        <div class="photo-wrap">
            <img
                node_ref=node_ref
                class="profile-photo"
                src=src
                alt=alt
                draggable="false"
            />
        </div>
    }
}
