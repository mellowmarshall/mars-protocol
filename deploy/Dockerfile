# ============================================================================
# mesh-hub — Multi-stage Docker build
# ============================================================================
# Build:  docker build -t mesh-hub -f deploy/Dockerfile .
# Run:    docker run -v ./data:/app/data -p 4433:4433/udp -p 8080:8080 mesh-hub

FROM rust:1.87-bookworm AS builder

WORKDIR /build
COPY . .

RUN cargo build --release --bin mesh-hub \
    && strip target/release/mesh-hub

# ── Runtime ──────────────────────────────────────────────────────────────────

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /usr/sbin/nologin mesh

WORKDIR /app
COPY --from=builder /build/target/release/mesh-hub /usr/local/bin/mesh-hub

RUN mkdir -p /app/data && chown mesh:mesh /app/data

USER mesh

EXPOSE 4433/udp
EXPOSE 8080/tcp

ENTRYPOINT ["mesh-hub"]
CMD ["--config", "/app/mesh-hub.toml"]
