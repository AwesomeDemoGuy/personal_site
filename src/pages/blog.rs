use leptos::prelude::*;

/// Blog tab. Post content will be loaded from SQLite via a server function
/// later; for now this renders the framework with a placeholder.
#[component]
pub fn BlogPage() -> impl IntoView {
    view! {
        <section class="page blog">
            <h1>"Blog"</h1>
            <p class="empty">"Posts coming soon."</p>
        </section>
    }
}
