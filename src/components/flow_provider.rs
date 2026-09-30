use leptos::prelude::*;
#[component]
pub fn FlowProvider(children: Children) -> impl IntoView {
    #[cfg(feature = "hydrate")]
    {
        let context = crate::text_flow::controller::FlowContext::provide();
        let location = leptos_router::hooks::use_location();
        Effect::new(move |_| {
            let _ = location.pathname.get();
            crate::interop::reset_draggables();
            context.schedule();
        });
    }
    children()
}
