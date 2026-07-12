# syntax=docker/dockerfile:1

# NOTE ON ARCHITECTURE / QEMU
# Build this image for the SAME CPU architecture as where it will run. Compiling
# Rust/LLVM under QEMU emulation (i.e. building for a foreign --platform) is slow
# and often crashes with "qemu: ... signal: aborted (core dumped)".
#   * amd64 server  -> build on amd64:  docker build --platform linux/amd64 .
#   * arm64 server  -> build on arm64 hardware (the server itself or an arm64
#                      CI/cloud builder). Do NOT cross-build via QEMU.

ARG RUST_VERSION=1.94.1

################################################################################
# Chef base stage: the shared toolchain (Rust + build deps + cargo-leptos +
# cargo-chef). Both the planner and the builder derive from this, so the
# expensive toolchain install is done once and cached.
FROM rust:${RUST_VERSION}-bookworm AS chef
WORKDIR /app

# Build dependencies. cargo-leptos uses dart-sass + wasm; clang/lld speed up linking.
RUN apt-get update && apt-get install -y --no-install-recommends \
        clang lld pkg-config libssl-dev curl ca-certificates gnupg \
    && rm -rf /var/lib/apt/lists/*

# Node.js (for bundling the pretext JS library). Used only at build time.
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# WASM target for the client/hydrate bundle.
RUN rustup target add wasm32-unknown-unknown

# Install build tooling via cargo-binstall, which fetches prebuilt binaries
# instead of compiling from source. This is much faster and avoids heavy LLVM
# compiles (especially fragile/slow under QEMU emulation for a non-native arch).
#
# cargo-chef caches dependency compilation across source-only changes.
#
# NOTE: we do NOT install wasm-bindgen-cli here. cargo-leptos 0.3+ detects the
# wasm-bindgen version from this project's Cargo.lock and downloads the matching
# CLI automatically, guaranteeing the bindgen schema matches the crate.
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
        https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y --locked cargo-leptos@0.3.6 cargo-chef

################################################################################
# Planner stage: distill the dependency graph into recipe.json. This depends
# only on the Cargo manifests/lockfile (not the rest of the source), so the
# recipe — and therefore the cook layer below — only changes when dependencies
# change.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

################################################################################
# Build stage: cook dependencies from the recipe (cached until deps change),
# then compile the app. Editing source invalidates only the layers from the
# source COPY onward — the dependency compilation above stays cached.
FROM chef AS build
COPY --from=planner /app/recipe.json recipe.json

# Pre-compile dependencies for BOTH targets cargo-leptos builds, matching its
# feature sets and profiles so the cooked artifacts are actually reused (a
# profile/feature mismatch would force a full recompile and defeat the cache):
#   * server binary: `release` profile, `ssr` features (native target)
#   * wasm hydrate:  `wasm-release` profile, `hydrate` features, wasm32 target
#
# The `--mount=type=cache` mounts persist Cargo's registry/git downloads and the
# compiled `target/` across builds (not just within one build's layer cache), so
# an evicted layer cache or a dependency bump still reuses everything unchanged.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo chef cook --release --no-default-features --features ssr \
        --recipe-path recipe.json \
 && cargo chef cook --profile wasm-release --target wasm32-unknown-unknown \
        --no-default-features --features hydrate --recipe-path recipe.json

# Copy the full source (cargo-leptos needs Cargo.toml, src, style, public).
COPY . .

# Vendor the real pretext library into public/js/pretext.js as a single ESM
# bundle (overwrites the fallback stub). Runs before the Leptos build so the
# bundled module is picked up by wasm-bindgen.
RUN ./scripts/vendor-pretext.sh

# Produce an optimized SSR binary + hashed site assets under target/site. The
# same cache mounts as the cook step above are reused here, so the cooked
# dependencies are already compiled and only our own crate rebuilds. Because
# `target/` is a cache mount (its contents are NOT part of the image layer), the
# artifacts are copied out to /app/out so the runtime stage can COPY them.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo leptos build --release \
 && mkdir -p /app/out \
 && cp target/release/personal_site /app/out/personal_site \
 && cp -r target/site /app/out/site

################################################################################
# Runtime stage: minimal image with the server binary and site assets.
FROM debian:bookworm-slim AS final
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user.
ARG UID=10001
RUN adduser --disabled-password --gecos "" --uid "${UID}" appuser

# Server binary and the generated site (JS/WASM/CSS/assets). These come from
# /app/out because the build stage keeps target/ in a cache mount (not the
# image layer) and copies the finished artifacts here.
COPY --from=build /app/out/personal_site /usr/local/bin/personal_site
COPY --from=build /app/out/site /app/site

# Directory for the SQLite database file (mounted as a volume in compose).
RUN mkdir -p /data && chown appuser:appuser /data
USER appuser

# Leptos runtime configuration. These must match the build-time metadata.
ENV LEPTOS_OUTPUT_NAME=personal_site \
    LEPTOS_SITE_ROOT=/app/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:31337 \
    DATABASE_URL=sqlite:///data/personal_site.db

EXPOSE 31337

CMD ["personal_site"]
