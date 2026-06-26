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
# Build stage: compile the Leptos full-stack app with cargo-leptos.
FROM rust:${RUST_VERSION}-bookworm AS build
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
# instead of compiling cargo-leptos from source. This is much faster and avoids
# heavy LLVM compiles (which are especially fragile/slow under QEMU emulation
# when building for a non-native architecture).
#
# NOTE: we do NOT install wasm-bindgen-cli here. cargo-leptos 0.3+ detects the
# wasm-bindgen version from this project's Cargo.lock and downloads the matching
# CLI automatically, guaranteeing the bindgen schema matches the crate.
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
        https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall -y --locked cargo-leptos@0.3.6

# Copy the full source (cargo-leptos needs Cargo.toml, src, style, public).
COPY . .

# Vendor the real pretext library into public/js/pretext.js as a single ESM
# bundle (overwrites the fallback stub). Runs before the Leptos build so the
# bundled module is picked up by wasm-bindgen.
RUN ./scripts/vendor-pretext.sh

# Produce an optimized SSR binary + hashed site assets under target/site.
RUN cargo leptos build --release

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

# Server binary and the generated site (JS/WASM/CSS/assets).
COPY --from=build /app/target/release/personal_site /usr/local/bin/personal_site
COPY --from=build /app/target/site /app/site

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
