#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use axum::http::header::CACHE_CONTROL;
    use axum::http::HeaderValue;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use personal_site::app::*;
    use personal_site::db;
    use tower_http::services::ServeDir;
    use tower_http::set_header::SetResponseHeaderLayer;

    // Initialize the SQLite database (creates the file and runs migrations).
    let pool = db::init_pool()
        .await
        .expect("failed to initialize SQLite database");

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;

    // Generate the list of routes in the Leptos App.
    let routes = generate_route_list(App);

    // Application state shared with server functions / handlers.
    let app_state = AppState {
        leptos_options: leptos_options.clone(),
        pool,
    };

    // The compiled JS/WASM/CSS bundle lives under <site_root>/pkg. We serve it
    // through our own tower-http 0.7 `ServeDir` (rather than leptos_axum's
    // built-in handler, which links an older tower-http that emits no ETag).
    // tower-http 0.7 generates a strong ETag from file size + mtime and answers
    // `If-None-Match` with `304 Not Modified`, so a rebuilt bundle is fetched
    // fresh while an unchanged one costs only a tiny conditional request.
    let pkg_dir = format!("{}/pkg", leptos_options.site_root);
    let pkg_service = ServeDir::new(pkg_dir)
        .precompressed_gzip()
        .precompressed_br();

    let app = Router::new()
        .leptos_routes_with_context(
            &app_state,
            routes,
            {
                let app_state = app_state.clone();
                move || provide_context(app_state.pool.clone())
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        // Serve hashless static bundle assets with ETag/304 support.
        .nest_service("/pkg", pkg_service)
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        // Force browsers to revalidate cached assets on every load instead of
        // serving them from the heuristic cache. `no-cache` means "you may
        // store this, but you must revalidate before reuse" — so a rebuilt
        // JS/WASM bundle is always picked up immediately, while the ETag/304
        // path above keeps unchanged assets cheap. We deliberately do NOT use
        // content-hashed filenames (cargo-leptos `hash-files`), because that
        // renames the wasm-bindgen JS interop snippets and breaks the WASM
        // module's import paths (turns the app into a dead static page).
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        ))
        .with_state(app_state);

    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // No client-side main function.
    // The hydration entry point lives in lib.rs (`hydrate`).
}
