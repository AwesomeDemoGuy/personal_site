use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes, A},
    StaticSegment,
};

use crate::components::photo::DraggablePhoto;
use crate::pages::{about::AboutPage, blog::BlogPage, gpg::GpgPage, projects::ProjectsPage};

/// Shared application state passed to Axum and Leptos route handlers.
///
/// Only meaningful on the server (`ssr`), where it carries the database pool
/// alongside the Leptos configuration.
#[cfg(feature = "ssr")]
#[derive(Clone, axum::extract::FromRef)]
pub struct AppState {
    pub leptos_options: leptos::config::LeptosOptions,
    pub pool: sqlx::SqlitePool,
}

/// The HTML document shell rendered by the server for every request.
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

/// Root application component: meta, theme stylesheet, and the router.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        // The compiled dark-mode stylesheet (see style/main.scss).
        <Stylesheet id="leptos" href="/pkg/personal_site.css"/>
        <Title text="Personal Site"/>

        <Router>
            <div class="app-shell">
                <Header/>
                <main class="content">
                    <Routes fallback=|| view! { <NotFound/> }>
                        <Route path=StaticSegment("") view=AboutPage/>
                        <Route path=StaticSegment("about") view=AboutPage/>
                        <Route path=StaticSegment("gpg") view=GpgPage/>
                        <Route path=StaticSegment("blog") view=BlogPage/>
                        <Route path=StaticSegment("projects") view=ProjectsPage/>
                    </Routes>
                </main>
                <Footer/>
            </div>
        </Router>
    }
}

/// Top section: draggable photo + tab navigation.
#[component]
fn Header() -> impl IntoView {
    view! {
        <header class="site-header">
            <DraggablePhoto src="/assets/me.jpg" alt="Photo of me"/>
            <nav class="tabs">
                <A href="/about" attr:class="tab">"About"</A>
                <A href="/blog" attr:class="tab">"Blog"</A>
                <A href="/projects" attr:class="tab">"Projects"</A>
            </nav>
        </header>
    }
}

#[component]
fn Footer() -> impl IntoView {
    let year = 2026;
    view! {
        <footer class="site-footer">
            <span>"© "{year}" Sebastian Ashkar \u{00B7} Built with Rust + Leptos"</span>
        </footer>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="not-found">
            <h1>"Error 404"</h1>
            <p>"That page does not exist."</p>
            <A href="/about" attr:class="tab">"Back to About Page"</A>
        </div>
    }
}
