# syntax=docker/dockerfile:1.7

# ═══════════════════════════════════════════════════════════════════
# Stage 1 : Build
# ═══════════════════════════════════════════════════════════════════
# Pin to a specific Rust minor + Debian trixie so builds are reproducible
# (rust:latest = anti-pattern for prod images).
FROM rust:1.97-slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies : copy manifests first so `cargo build` re-uses the
# compiled deps as long as Cargo.lock is stable.
COPY Cargo.toml Cargo.lock ./

# Dummy src so cargo builds only the deps ; discarded before the real build.
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src

COPY src/ src/
COPY migrations/ migrations/

# Touch to force cargo to rebuild the (now real) sources.
RUN touch src/main.rs src/lib.rs && cargo build --release

# ═══════════════════════════════════════════════════════════════════
# Stage 2 : Runtime
# ═══════════════════════════════════════════════════════════════════
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
        tini \
    && rm -rf /var/lib/apt/lists/* \
    # Create a non-root user to run the app under. UID/GID pinned so
    # bind-mounted volumes have predictable ownership.
    && groupadd --system --gid 10001 skilluv \
    && useradd  --system --uid 10001 --gid skilluv --create-home skilluv

WORKDIR /app

# Main binary + migrations, ownership set to non-root user.
COPY --from=builder --chown=skilluv:skilluv /app/target/release/skilluv-backend ./skilluv-backend
COPY --from=builder --chown=skilluv:skilluv /app/migrations/ ./migrations/

# Auxiliary binaries — seed catalog data, provision admin, dump DB, ingest GitHub.
# Shipped in the same image so ops can `docker exec` any of them without
# rebuilding. Kept in /usr/local/bin so they're on PATH. Owned by root there
# (system location, non-writable by app user, as it should be).
COPY --from=builder /app/target/release/skilluv-seed         /usr/local/bin/skilluv-seed
COPY --from=builder /app/target/release/skilluv-seed-admin   /usr/local/bin/skilluv-seed-admin
COPY --from=builder /app/target/release/skilluv-backup       /usr/local/bin/skilluv-backup
COPY --from=builder /app/target/release/skilluv-github-ingest /usr/local/bin/skilluv-github-ingest

USER skilluv:skilluv

EXPOSE 3001

ENV HOST=0.0.0.0 \
    PORT=3001 \
    RUST_LOG=skilluv_backend=info,tower_http=info

# tini reaps zombies + forwards SIGTERM cleanly (axum shuts down gracefully
# only if it actually receives the signal — bash shell PIDs swallow them).
ENTRYPOINT ["/usr/bin/tini", "--"]

# Container-level healthcheck. Coolify / k8s / docker compose all honor it.
# `wget` is not installed to keep the image small ; curl is already there.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3001/api/health || exit 1

# OCI labels for image provenance (SBOM tools + registries pick these up).
LABEL org.opencontainers.image.source="https://github.com/Skilluv/skilluv-backend" \
      org.opencontainers.image.description="Skilluv backend API (axum, Rust)" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later" \
      org.opencontainers.image.vendor="Skilluv"

CMD ["./skilluv-backend"]
