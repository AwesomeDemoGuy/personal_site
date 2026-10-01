# personal_site

A minimalistic, dark-mode personal website built with Rust and
[Leptos](https://leptos.dev) (full-stack SSR + hydration), backed by SQLite and
served with Podman.

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
| Containerized  | Podman / podman-compose                 |

## Project layout

```
src/
  main.rs              Server entry: Axum + DB init + Leptos routes
  lib.rs               Hydration entry + module wiring
  app.rs               Router, document shell, header (photo + tabs), footer
  models.rs            Blog post, index summary, and rendered HTML types
  db.rs                SQLite pool + schema migration (server-only)
  markdown.rs          Markdown rendering, sanitization, and code highlighting
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

## Running with Podman

```bash
podman-compose -p personal_site -f compose.yaml up -d --build
```

The site is served at http://localhost:31337. SQLite data persists in the
`personal_site_db-data` volume. Use the same Compose project name for subsequent
updates to keep that database.

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

Drag the photo over this paragraph to see the text flow around it.'
);
```

Images can use paths under `public/assets`, referenced as `/assets/...` in the
Markdown. Standalone images become figures; images within prose are separated
into figures by the browser adapter. Figures start centered on their own line
and can be dragged with a mouse. On touchscreens, press and hold an image for
450 ms until its outline appears, then drag it. Ordinary swipes over images
scroll the page. This also applies to the profile photo and certificate icons.
The page stays locked in place during a held drag and resumes scrolling on release.
Pretext wraps text around their
rectangular bounds when moved into prose, and around any draggable photos.
The layout also updates when an image loads or the viewport changes.
Dragging an image out of its original row collapses that space, allowing the
following content to move up. Drop it near the center of its original row to snap
it back into place. Navigating also restores images to their original rows.

Dragging applies the latest pointer position once per animation frame. Only text
whose wrapping changes rebuilds its visible fragments; distant blocks reuse
their layout. Obstacle bounds are measured once per frame, image sizes update
through `ResizeObserver`, and generated text fragments do not trigger article
rescans. Markdown parsing and highlighting remain on the server, while pointer
movement and text wrapping stay in the browser for immediate feedback.

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
into the existing `public/js/pretext.js` file. The container build does this
automatically.
