use leptos::prelude::*;

/// The default landing page. Holds the intro, external links, certificates,
/// and technologies. Concrete copy/data is filled in later — placeholders for
/// now so the framework renders end-to-end.
#[component]
pub fn AboutPage() -> impl IntoView {
    view! {
        <section class="page about">
            <h1>"About Me"</h1>

            <p class="intro">
                // Placeholder bio — replaced with real copy later. Long enough
                // that dragging the photo over it visibly displaces the text.
                "Short introduction goes here. This paragraph is intentionally a \
                 few sentences long so that the pretext layout engine has real \
                 text to flow around the profile photo. Try grabbing the photo \
                 above and dragging it down across these lines: the text reflows \
                 to make room for it, shifting to whichever side has space and \
                 falling back to full width once the photo no longer overlaps a \
                 given line. None of this touches the browser's own layout for \
                 measurement — pretext computes the line breaks from cached glyph \
                 widths, which keeps the reflow smooth while you drag."
            </p>

            <div class="links">
                <a
                    class="ext-link"
                    href="https://www.linkedin.com/in/your-handle"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    "LinkedIn"
                </a>
                <a
                    class="ext-link"
                    href="https://github.com/your-handle"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    "GitHub"
                </a>
            </div>

            <CertificatesSection/>
            <TechnologiesSection/>
        </section>
    }
}

#[component]
fn CertificatesSection() -> impl IntoView {
    // Placeholder list; will be sourced from the database later.
    let placeholders = ["Certificate one", "Certificate two", "Certificate three"];

    view! {
        <div class="section certificates">
            <h2>"Certificates"</h2>
            <ul class="cert-list">
                {placeholders
                    .into_iter()
                    .map(|c| view! { <li class="cert-item">{c}</li> })
                    .collect_view()}
            </ul>
        </div>
    }
}

#[component]
fn TechnologiesSection() -> impl IntoView {
    // Placeholder tags; will be sourced from the database later.
    let placeholders = ["Rust", "Leptos", "SQLite", "Docker", "TypeScript"];

    view! {
        <div class="section technologies">
            <h2>"Technologies"</h2>
            <div class="tech-tags">
                {placeholders
                    .into_iter()
                    .map(|t| view! { <span class="tech-tag">{t}</span> })
                    .collect_view()}
            </div>
        </div>
    }
}
