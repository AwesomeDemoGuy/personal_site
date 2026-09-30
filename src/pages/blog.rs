use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{components::A, hooks::use_params_map};

use crate::models::{BlogPostSummary, RenderedBlogPost};

#[server]
pub async fn get_blog_posts() -> Result<Vec<BlogPostSummary>, ServerFnError> {
    let pool = use_context::<sqlx::SqlitePool>()
        .ok_or_else(|| ServerFnError::new("Database unavailable"))?;
    crate::db::list_blog_posts(&pool).await.map_err(|error| {
        leptos::logging::error!("listing blog posts: {error}");
        ServerFnError::new("Could not load posts")
    })
}

#[server]
pub async fn get_blog_post(slug: String) -> Result<Option<RenderedBlogPost>, ServerFnError> {
    let pool = use_context::<sqlx::SqlitePool>()
        .ok_or_else(|| ServerFnError::new("Database unavailable"))?;
    crate::db::find_blog_post(&pool, &slug)
        .await
        .map(|post| {
            post.map(|post| RenderedBlogPost {
                html: format!(
                    "<h1>{}</h1><time class=\"published-at\" datetime=\"{}\">{}</time>{}",
                    ammonia::clean_text(&post.title),
                    ammonia::clean_text(&post.published_at.replace(' ', "T")),
                    ammonia::clean_text(
                        post.published_at
                            .split(['T', ' '])
                            .next()
                            .unwrap_or(&post.published_at)
                    ),
                    crate::markdown::render(&post.body),
                ),
                title: post.title,
            })
        })
        .map_err(|error| {
            leptos::logging::error!("loading blog post: {error}");
            ServerFnError::new("Could not load post")
        })
}

#[component]
pub fn BlogPage() -> impl IntoView {
    let posts = Resource::new(|| (), |_| get_blog_posts());
    view! {
        <section class="page blog">
            <Title text="Blog · Sebastian Ashkar"/>
            <h1>"Blog"</h1>
            <Suspense fallback=|| view! { <p>"Loading posts…"</p> }>
                {move || posts.get().map(|result| match result {
                    Ok(posts) if posts.is_empty() => view! {
                        <p class="empty">"Posts coming soon."</p>
                    }.into_any(),
                    Ok(posts) => view! {
                        <ul class="blog-posts">
                            {posts.into_iter().map(|post| {
                                let url = post.url();
                                view! {
                                    <li>
                                        <A href=url>{post.title}</A>
                                        <PublishedAt value=post.published_at/>
                                    </li>
                                }
                            }).collect_view()}
                        </ul>
                    }.into_any(),
                    Err(_) => view! { <p role="alert">"Could not load posts. Please try again."</p> }.into_any(),
                })}
            </Suspense>
        </section>
    }
}

#[component]
pub fn BlogPostPage() -> impl IntoView {
    let params = use_params_map();
    let post = Resource::new(
        move || params.read().get("slug").unwrap_or_default(),
        get_blog_post,
    );
    view! {
        <section class="page blog">
            <A href="/blog" attr:class="blog-back">"← All posts"</A>
            <Suspense fallback=|| view! { <p>"Loading post…"</p> }>
                {move || post.get().map(|result| match result {
                    Ok(Some(post)) => view! {
                        <Title text=format!("{} · Sebastian Ashkar", post.title)/>
                        <article class="markdown-body" inner_html=post.html/>
                    }.into_any(),
                    Ok(None) => {
                        #[cfg(feature = "ssr")]
                        if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
                            response.set_status(axum::http::StatusCode::NOT_FOUND);
                        }
                        view! {
                            <Title text="Post not found · Sebastian Ashkar"/>
                            <h1>"Post not found"</h1>
                            <p>"That blog post does not exist."</p>
                        }.into_any()
                    },
                    Err(_) => view! { <p role="alert">"Could not load this post. Please try again."</p> }.into_any(),
                })}
            </Suspense>
        </section>
    }
}

#[component]
fn PublishedAt(value: String) -> impl IntoView {
    let date = value.split(['T', ' ']).next().unwrap_or(&value).to_string();
    view! { <time class="published-at" datetime=value.replace(' ', "T")>{date}</time> }
}
