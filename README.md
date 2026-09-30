# personal_site

A dark-mode personal website built with Rust, [Leptos](https://leptos.dev)
SSR and hydration, Axum, and SQLite.

## Features

- About, Blog, Projects, and GPG pages.
- Text and atomic chips flow around draggable photos and certificate icons.
- Rust port of the checked-in [Pretext](https://github.com/chenglou/pretext)
  0.0.8 engine: Unicode analysis, measurement caches, incremental line breaking,
  materialization, statistics, and browser compatibility profiles.
- Leptos owns text rendering, source updates, registration, and cleanup.
- Canvas and `Intl.Segmenter` supply browser font metrics and segmentation.
- Readable server markup before hydration or with JavaScript disabled.
- Weather cache and SQLite persistence through SQLx.
- `/gpg` serves the original importable key to clients requesting plain text.

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
  main.rs                 Axum server, database, GPG content negotiation
  app.rs                  Document shell, router, shared flow provider
  pretext/                Framework-independent Rust layout engine
    browser/              Canvas, Intl segmentation, browser profiles
  text_flow/              Circle geometry, chip packing, browser controller
  components/
    flow_text.rs          Reactive text source and Leptos-rendered lines
    flow_chips.rs         Atomic chip registration
    certificate_icon.rs   Rust pointer handling and Leptos-owned portal
    photo.rs              Photo and obstacle registration
  pages/                  About, Blog, Projects, GPG
public/js/interop.js       Temporary photo pointer-drag bridge
style/main.scss           Site styles
tests/pretext_parity.rs    Native comparisons against recorded oracle outputs
tests/fixtures/pretext/    Frozen JavaScript oracle, metrics, attribution
scripts/pretext-*.mjs      Development-only reference and browser checks
```

The production build does not use Node or a vendored JavaScript layout engine.
The remaining photo drag bridge dispatches `photomove`; Rust performs layout.

## Run

```bash
docker compose up --build
```

The site is served at http://localhost:31337. SQLite data persists in the
`db-data` Docker volume.

For local development, install Rust, the `wasm32-unknown-unknown` target, and
`cargo-leptos` 0.3.6, then use `cargo leptos watch`. Server settings are listed
in [.env.example](.env.example). A production build uses `cargo leptos build --release`.

## Validate the Rust migration

```bash
cargo test --lib --test pretext_parity --no-default-features --features ssr
cargo check --no-default-features --features ssr
cargo check --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
cargo leptos build --release
```

For differential browser testing, build with
`cargo leptos build --release --lib-features hydrate,pretext-validation`, start
the server, and run `node scripts/pretext-browser-check.mjs`. These optional
tests require Node and Playwright with its browser binaries. Set
`PRETEXT_TEST_URL` for the server URL and `PRETEXT_BROWSER` to `chromium`,
`firefox`, or `webkit`. Set `PRETEXT_TEST_FONT` to a local TTF file to include
delayed-font and changed-chip-size checks. `PRETEXT_SKIP_PARITY=1` runs the site
checks against a normal production build. Reports and screenshots are written
to `target/pretext-validation/`.

`node scripts/pretext-reference.mjs` regenerates deterministic fixtures and
the original bidi/punctuation tables. To regenerate Unicode property tables,
run `cargo run --quiet --example pretext_unicode > /tmp/pretext-unicode.rs`,
then copy the completed output to `src/pretext/unicode.rs`.
Regex and Unicode segmentation crates used by these tools are development-only.

See [migration results and API details](docs/pretext-rust-migration-results.md)
and the [implementation plan](docs/pretext-rust-migration-plan.md). The frozen
oracle and translated algorithms retain the [upstream MIT notice](tests/fixtures/pretext/LICENSE).
