use leptos::prelude::*;

/// Projects tab. Project entries will be loaded from SQLite via a server
/// function later; for now this renders the framework with a placeholder.
#[component]
pub fn ProjectsPage() -> impl IntoView {
    view! {
        <section class="page projects">
            <h1>"Projects"</h1>
            <p class="empty">"Projects coming soon."</p>
        </section>
    }
}
