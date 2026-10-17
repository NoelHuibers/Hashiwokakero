# syntax=docker/dockerfile:1

# Tailwind v4 (the CLI and tw-animate-css) comes from npm, cargo-leptos runs node_modules/.bin/tailwindcss
FROM node:26-trixie-slim AS css-deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts

FROM rust:1.99-trixie AS builder

# cargo-binstall makes it easier to install cargo extensions like cargo-leptos
RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo binstall -y cargo-leptos@0.3.11

WORKDIR /app
COPY --from=css-deps /usr/local/bin/node /usr/local/bin/node
COPY . .
COPY --from=css-deps /app/node_modules ./node_modules

# Installs the toolchain from rust-toolchain.toml (stable + wasm32-unknown-unknown)
RUN rustup toolchain install

RUN cargo leptos build --release -vv

FROM debian:trixie-slim AS runner
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
# The server binary, the hashes of the JS/WASM/CSS file names (read next to the binary) and the
# site (JS/WASM/CSS and public assets)
COPY --from=builder /app/target/release/hashi /app/target/release/hash.txt /app/
COPY --from=builder /app/target/site /app/site

ENV RUST_LOG="info"
ENV LEPTOS_OUTPUT_NAME="hashi"
ENV LEPTOS_SITE_ADDR="0.0.0.0:8080"
ENV LEPTOS_SITE_ROOT="site"
ENV LEPTOS_SITE_PKG_DIR="pkg"
ENV LEPTOS_HASH_FILES="true"
ENV LEPTOS_ENV="PROD"
EXPOSE 8080

CMD ["/app/hashi"]
