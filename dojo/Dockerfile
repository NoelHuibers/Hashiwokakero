# syntax=docker/dockerfile:1
# The SAT Dojo worker. Build from the repository root:
#   fly deploy . --config dojo/fly.toml --dockerfile dojo/Dockerfile

FROM rust:1.99-trixie AS builder
WORKDIR /app
COPY . .
RUN rustup toolchain install
RUN cargo build -p dojo --features worker --release

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/dojo /usr/local/bin/dojo
CMD ["dojo", "serve"]
