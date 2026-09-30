use leptos::prelude::*;
#[derive(Clone, Copy, Default)]
pub enum ChipTag {
    #[default]
    Div,
    Paragraph,
}
#[component]
pub fn FlowChips(
    #[prop(into)] class: String,
    #[prop(default=ChipTag::Div)] tag: ChipTag,
    #[prop(default = false)] exclude_certificates: bool,
    children: Children,
) -> impl IntoView {
    #[cfg(not(feature = "hydrate"))]
    let _ = exclude_certificates;
    match tag {
        ChipTag::Div => {
            let node = NodeRef::<leptos::html::Div>::new();
            #[cfg(feature = "hydrate")]
            bind(node, exclude_certificates);
            view! { <div node_ref=node class=class>{children()}</div> }.into_any()
        }
        ChipTag::Paragraph => {
            let node = NodeRef::<leptos::html::P>::new();
            #[cfg(feature = "hydrate")]
            bind(node, exclude_certificates);
            view! { <p node_ref=node class=class>{children()}</p> }.into_any()
        }
    }
}
#[cfg(feature = "hydrate")]
fn bind<E>(node: NodeRef<E>, exclude: bool)
where
    E: leptos::html::ElementType + 'static,
    E::Output: wasm_bindgen::JsCast + Clone + 'static,
{
    use crate::text_flow::controller::FlowContext;
    use wasm_bindgen::JsCast;
    if let Some(context) = use_context::<FlowContext>() {
        let id = StoredValue::new(None::<u64>);
        node.on_load(move |el| {
            id.set_value(Some(context.register_chips(el.unchecked_into(), exclude)))
        });
        on_cleanup(move || {
            if let Some(id) = id.get_value() {
                context.unregister(id);
            }
        });
    }
}
