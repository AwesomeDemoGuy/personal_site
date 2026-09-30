use leptos::prelude::*;

/// Semantic parents own this inline-compatible subtree on SSR and hydration.
#[component]
pub fn FlowText(
    #[prop(into)] text: Signal<String>,
    #[prop(default = false)] break_anywhere: bool,
) -> impl IntoView {
    let node = NodeRef::<leptos::html::Span>::new();
    let output = RwSignal::new(None::<crate::text_flow::FlowLayout>);
    #[cfg(feature = "hydrate")]
    {
        use crate::{
            pretext::{PrepareOptions, WhiteSpace},
            text_flow::controller::FlowContext,
        };
        use wasm_bindgen::JsCast;
        if let Some(context) = use_context::<FlowContext>() {
            let id = StoredValue::new(None::<u64>);
            Effect::new(move |_| {
                let source = text.get();
                let Some(element) = node.get() else {
                    return;
                };
                let options = PrepareOptions {
                    white_space: WhiteSpace::PreWrap,
                    ..Default::default()
                };
                if let Some(id) = id.get_value() {
                    context.update_text(id, source, options);
                } else {
                    let new_id = context.register_text(
                        element.unchecked_into(),
                        source,
                        options,
                        break_anywhere,
                        output,
                    );
                    id.set_value(Some(new_id));
                }
            });
            on_cleanup(move || {
                if let Some(id) = id.get_value() {
                    context.unregister(id);
                }
            });
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = break_anywhere;
    view! {
        <span class="flow-text" node_ref=node style=move ||output.with(|v|v.as_ref().map(|l|format!("height:{}px",l.height)).unwrap_or_default())>
            <Show when=move ||output.with(|v|v.is_some()) fallback=move ||text.get()>
                <For each=move ||output.with(|v|(0..v.as_ref().map_or(0,|l|l.lines.len())).collect::<Vec<_>>()) key=|i|*i children=move |i| {
                    view! { <span class="flow-line" style=move ||output.with(|v|v.as_ref().and_then(|l|l.lines.get(i).map(|line|format!("left:{}px;top:{}px;line-height:{}px",line.x,line.y,l.line_height))).unwrap_or_default())>{move ||output.with(|v|v.as_ref().and_then(|l|l.lines.get(i)).map(|line|line.text.clone()).unwrap_or_default())}</span> }
                }/>
            </Show>
        </span>
    }.into_any()
}
