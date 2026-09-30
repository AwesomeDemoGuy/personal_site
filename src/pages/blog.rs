use crate::components::flow_text::FlowText;
use leptos::prelude::*;

/// Blog tab. Post content will be loaded from SQLite via a server function
/// later; for now this renders the framework with a placeholder.
#[component]
pub fn BlogPage() -> impl IntoView {
    view! {
        <section class="page blog">
            <h1><FlowText text="Blog".to_string()/></h1>
            <p class="empty"><FlowText text="Posts coming soon.".to_string()/></p>
        </section>
    }
}
