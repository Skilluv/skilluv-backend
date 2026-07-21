# Stage 1: Build
# Pin to a specific Rust minor + Debian trixie so builds are reproducible
# (rust:latest = anti-pattern for prod images).
FROM rust:1.97-slim-trixie AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies: copy manifests first
COPY Cargo.toml Cargo.lock ./

# Create dummy src to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src

# Copy actual source code
COPY src/ src/
COPY migrations/ migrations/

# Build the real binary
RUN touch src/main.rs src/lib.rs && cargo build --release

# Stage 2: Runtime
FROM debian:trixie-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and migrations
COPY --from=builder /app/target/release/skilluv-backend ./skilluv-backend
COPY --from=builder /app/migrations/ ./migrations/

EXPOSE 3001

ENV HOST=0.0.0.0
ENV PORT=3001
ENV RUST_LOG=skilluv_backend=info,tower_http=info

CMD ["./skilluv-backend"]
