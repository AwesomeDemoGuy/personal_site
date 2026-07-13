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
  js/pretext.js        pretext wrapper (fallback stub; real lib vendored in Docker)
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