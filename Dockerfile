# syntax=docker/dockerfile:1.7

# ═══════════════════════════════════════════════════════════════════
# Stage 1 : Build
# ═══════════════════════════════════════════════════════════════════
# Pin to a specific Rust minor + Debian trixie so builds are reproducible
# (rust:latest = anti-pattern for prod images).
FROM rust:1.98-slim-trixie AS builder

# curl is required by utoipa-swagger-ui's build.rs to fetch the Swagger UI
# zip from GitHub at compile time. Without it, the build panics with
# 'failed to download Swagger UI: curl command not found'.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies : copy manifests first so `cargo build` re-uses the
# compiled deps as long as Cargo.lock is stable.
COPY Cargo.toml Cargo.lock ./

# Dummy src so cargo builds only the deps ; discarded before the real build.
#
# `--features discord-bot` here as well as in the real build below, and that
# is the whole point of the layer. A feature set is part of what Cargo
# fingerprints, so warming the cache without it warmed a build nothing ever
# asked for: the real build then recompiled the entire dependency graph, four
# hundred crates from `proc-macro2` onward, on every image. The layer said
# CACHED and bought nothing.
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release --features discord-bot 2>/dev/null || true
RUN rm -rf src

COPY src/ src/
COPY migrations/ migrations/

# Embedded at compile time by `include_str!` / `include_bytes!`, so they have
# to exist in the builder even though nothing reads them at runtime: the
# translation catalogues behind `services::i18n`, and the two fonts the
# OpenGraph card rasterises with.
COPY locales/ locales/
COPY assets/ assets/

# `services::discord_roles` embeds `ops/discord/server.toml` with include_str!,
# so the declaration of the Discord server has to exist in the builder. Without
# it the build fails at compile time rather than at run time, which is the
# right end of the pipeline for a missing file.
COPY ops/ ops/

# Touch to force cargo to rebuild the (now real) sources.
# --features discord-bot pulls serenity in so the discord_bot binary
# is included in the image (feature-gated to keep test builds lean).
RUN touch src/main.rs src/lib.rs && cargo build --release --features discord-bot

# ═══════════════════════════════════════════════════════════════════
# Stage 2 : Runtime
# ═══════════════════════════════════════════════════════════════════
FROM debian:trixie-slim

# `upgrade` as well as `install`: the packages that ship inside the base
# image — util-linux among them — never got a security update otherwise, so
# the scan failed on fixes Debian had already published and this image had
# simply not taken. It costs a layer and makes the build non-reproducible
# across days, which is the point: a rebuild should pick up patches.
RUN apt-get update \
    && apt-get upgrade -y --no-install-recommends \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
        tini \
        procps \
    && rm -rf /var/lib/apt/lists/* \
    # Create a non-root user to run the app under. UID/GID pinned so
    # bind-mounted volumes have predictable ownership.
    && groupadd --system --gid 10001 skilluv \
    && useradd  --system --uid 10001 --gid skilluv --create-home skilluv

WORKDIR /app

# Long-running binaries — one per Coolify app. All shipped in /usr/local/bin
# so they're on PATH and `docker exec` works. Kept root-owned there (system
# location, non-writable by the app user by design).
#   skilluv-backend         — HTTP API (main)
#   skilluv-discord-bot     — SKI-116 gateway bot v2 (long-running)
#   skilluv-discord-notifier — SKI-34 webhook notifier v1 (kept for fallback)
COPY --from=builder /app/target/release/skilluv-backend          /usr/local/bin/skilluv-backend
COPY --from=builder /app/target/release/skilluv-discord-bot      /usr/local/bin/skilluv-discord-bot
COPY --from=builder /app/target/release/skilluv-discord-notifier /usr/local/bin/skilluv-discord-notifier

# Migrations bundled with the image so any long-running binary that
# runs sqlx::migrate! at boot finds them at /app/migrations.
COPY --from=builder --chown=skilluv:skilluv /app/migrations/ ./migrations/

# Auxiliary one-shot binaries — seed catalog data, provision admin,
# dump DB, ingest GitHub. Run via `docker exec` or a Coolify one-shot job.
COPY --from=builder /app/target/release/skilluv-seed          /usr/local/bin/skilluv-seed
COPY --from=builder /app/target/release/skilluv-seed-admin    /usr/local/bin/skilluv-seed-admin
COPY --from=builder /app/target/release/skilluv-seed-projects /usr/local/bin/skilluv-seed-projects
COPY --from=builder /app/target/release/skilluv-backup        /usr/local/bin/skilluv-backup
COPY --from=builder /app/target/release/skilluv-github-ingest /usr/local/bin/skilluv-github-ingest

USER skilluv:skilluv

EXPOSE 3001

# SKILLUV_BINARY picks which long-running binary this container runs.
# Defaults to the HTTP backend so the existing Coolify app keeps its
# behavior. A second Coolify app pulling the same image only has to set
# SKILLUV_BINARY=skilluv-discord-bot to run the Discord bot instead.
ENV HOST=0.0.0.0 \
    PORT=3001 \
    RUST_LOG=skilluv_backend=info,tower_http=info \
    SKILLUV_BINARY=skilluv-backend

# tini reaps zombies + forwards SIGTERM cleanly (axum shuts down gracefully
# only if it actually receives the signal — bash shell PIDs swallow them).
# `exec` chains the binary so it inherits PID 1's signal handling from tini.
ENTRYPOINT ["/usr/bin/tini", "--", "/bin/sh", "-c", "exec /usr/local/bin/${SKILLUV_BINARY}"]

# Container-level healthcheck — conditional on which binary this
# container runs. The HTTP backend serves /api/health on port 3001 ;
# every other binary (skilluv-discord-bot, skilluv-discord-notifier,
# skilluv-github-ingest as a long-running worker) has no HTTP surface.
# `pgrep` returns 0 as long as the target process is running, which is
# what we actually want to know for a non-HTTP container.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD if [ "$SKILLUV_BINARY" = "skilluv-backend" ]; then \
            curl -fsS http://127.0.0.1:3001/api/health || exit 1 ; \
        else \
            pgrep -f "$SKILLUV_BINARY" > /dev/null || exit 1 ; \
        fi

# OCI labels for image provenance (SBOM tools + registries pick these up).
LABEL org.opencontainers.image.source="https://github.com/Skilluv/skilluv-backend" \
      org.opencontainers.image.description="Skilluv backend API (axum, Rust)" \
      org.opencontainers.image.licenses="AGPL-3.0-or-later" \
      org.opencontainers.image.vendor="Skilluv"

CMD ["./skilluv-backend"]
