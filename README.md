# personal_site

A minimalistic, dark-mode personal website built with Rust and
[Leptos](https://leptos.dev) (full-stack SSR + hydration), backed by SQLite and
served from Docker.

## Features

- Three tabs: **About** (default), **Blog**, **Projects**
- Dark-mode theme (dark grey background, white text)
- Draggable photos (pointer-drag via JS interop)
- [pretext](https://github.com/chenglou/pretext) text-layout library wired in
  through `wasm-bindgen` JS interop
- SQLite persistence via SQLx
- Blog posts rendered from Markdown in Rust, with formatted text flowing around images

## Tech stack

| Concern        | Choice                                  |
| -------------- | --------------------------------------- |
| Language       | Rust                                    |
| Web framework  | Leptos 0.8 (SSR + hydration) + Axum     |
| Database       | SQLite (SQLx)                           |
| JS interop     | wasm-bindgen                            |
| Build tool     | cargo-leptos                            |
| Containerized  | Docker / Docker Compose                 |

## Project layout

```
src/
  main.rs              Server entry: Axum + DB init + Leptos routes
  lib.rs               Hydration entry + module wiring
  app.rs               Router, document shell, header (photo + tabs), footer
  models.rs            Shared types: BlogPost, Project, Certificate, Technology
  db.rs                SQLite pool + schema migration (server-only)
  interop.rs           wasm-bindgen bindings (draggable photo + pretext)
  components/photo.rs  DraggablePhoto component
  pages/               about.rs (default), blog.rs, projects.rs
public/
  js/interop.js        Drag-to-move logic
  js/pretext.js        Vendored Pretext layout and rich-inline APIs
  assets/me.jpg        Profile photo
style/main.scss        Dark-mode theme
scripts/
  vendor-pretext.sh    Bundles the real pretext library into public/js/pretext.js
```

## Running with Docker (recommended)

```bash
docker compose up --build
```

The site is served at http://localhost:31337. SQLite data persists in the
`db-data` Docker volume.

## Blog posts

`/blog` lists posts from SQLite, newest first. `/blog/{slug}` renders a post's
`body` as Markdown on the server. The parser and sanitizer are server-only Rust
dependencies; Pretext is the browser's text-layout dependency.

Add a row to `blog_posts` in the configured database, keeping `body` as raw
Markdown. For example:

```sql
INSERT INTO blog_posts (title, slug, body) VALUES (
  'My first post',
  'my-first-post',
  'A paragraph with **bold**, *emphasis*, [a link](/about), and `inline code`.

![Photo](/assets/me.jpg)

This paragraph flows beside the photo after the page hydrates.'
);
```

Images can use paths under `public/assets`, referenced as `/assets/...` in the
Markdown. Standalone images become figures; images within prose are separated
into figures by the browser adapter. Figures start centered on their own line
and can be dragged with a mouse or touch. Pretext wraps text around their
rectangular bounds when moved into prose, and around any draggable photos.
The layout also updates when an image loads or the viewport changes.

Set an image width in pixels by appending `{width=629}` (or `{width=629px}`):

```markdown
![Screenshot](/assets/blog/my-post/screenshot.png){width=629}
```

Widths must be positive whole numbers. Images keep their aspect ratio and shrink
to fit the article on smaller screens. This also works for linked images and
images mixed with prose. Images without a width retain the default size.

Headings receive unique anchor IDs: `## 1. Initial observations` becomes
`#1-initial-observations`, so chapter links can use
`[Initial observations](#1-initial-observations)`.

Links to local files under `/assets/` are marked as downloads so the browser
handles them directly. Use root-relative URLs such as
`[Download](/assets/blog/my-post/file.zip)` to work on both test and public sites.

CommonMark headings, lists, quotes, links, emphasis, images, hard line breaks,
code blocks, and strikethrough are supported. Fenced code blocks with a language
label (for example, `python`, `c`, or `rust`) receive server-side syntax
highlighting using Syntect. Unlabelled blocks, `text`, and unknown languages stay
plain. Code retains whitespace, token colors, and wrapping around obstacles.
Each code block has a Copy button that copies its original text, including
indentation and line breaks.
Raw HTML is displayed as text, and unsafe link/image URL
schemes are removed. The original semantic HTML stays accessible while Pretext
renders the visible lines, including keyboard focus for links. With JavaScript
disabled, posts remain readable using their server-rendered HTML.

After changing the Pretext bundle entrypoints, run `scripts/vendor-pretext.sh`
before building. It bundles the pinned package's regular and rich-inline APIs
into the existing `public/js/pretext.js` file. Docker does this automatically.
