# syntax=docker/dockerfile:1
# The SAT Dojo worker. Build from the repository root:
#   fly deploy . --config dojo/fly.toml --dockerfile dojo/Dockerfile

FROM rust:1.99-trixie AS builder
WORKDIR /app
COPY . .
RUN rustup toolchain install
RUN cargo build --locked -p dojo --features worker --release

FROM debian:trixie-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/dojo /usr/local/bin/dojo
# Submissions run sandboxed in wasmtime; on top of that the worker doesn't run as root. Fly mounts
# the cache volume as root, so the entry point hands it over and then drops privileges.
RUN useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin dojo
CMD ["sh", "-c", "chown dojo /data 2>/dev/null; exec setpriv --reuid=dojo --regid=dojo --init-groups --inh-caps=-all dojo serve"]
