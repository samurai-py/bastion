# syntax=docker/dockerfile:1
# Bastion v3 — Multi-stage static build, scratch final image (PKG-01, PKG-03, D-06)
# Stage 1: builder — rust:alpine + musl toolchain
# Stage 2: scratch image — zero OS overhead, static binary only
#
# Replaces the v2 `FROM ghcr.io/openclaw/openclaw` image (200MB+ Node runtime)
# with a single static Rust binary in a scratch container.

# ── Stage 1: builder ──────────────────────────────────────────────────────────
FROM rust:alpine AS builder

# musl-dev: musl libc headers; gcc: C compiler for rusqlite's bundled SQLite;
# ca-certificates: SSL bundle copied into the scratch stage.
# NOTE: `musl-tools` is a Debian package and does NOT exist on Alpine — on
# rust:alpine the musl target is built with `musl-dev` + `gcc`.
RUN apk add --no-cache musl-dev gcc ca-certificates

# Register the musl target (static linking target).
RUN rustup target add x86_64-unknown-linux-musl

# Force a fully static binary (crt-static): rusqlite's bundled C otherwise links
# dynamically against the host C runtime, which breaks the scratch image.
ENV RUSTFLAGS="-C target-feature=+crt-static"

WORKDIR /build

# Copy manifests first — this layer is cached if no deps change.
COPY Cargo.toml Cargo.lock ./

# Stub src for dependency pre-cache (avoids re-compiling deps on code-only changes).
RUN mkdir src && echo 'fn main(){}' > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl 2>/dev/null || true; \
    rm -rf src

# Copy actual source.
COPY src ./src

# Force rebuild of src (cargo detects the manifest/source timestamp).
RUN touch src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl

# Verify static linking before shipping — fail the build if the binary is dynamic.
RUN OUTPUT=$(ldd target/x86_64-unknown-linux-musl/release/bastion 2>&1 || true); \
    echo "$OUTPUT"; \
    echo "$OUTPUT" | grep -q "not a dynamic executable" || \
    (echo "ERROR: binary is dynamically linked — musl static target required" && exit 1)

# ── Stage 2: scratch — zero OS layer ──────────────────────────────────────────
FROM scratch

# SSL certificates required for HTTPS (Anthropic, OpenAI, Telegram long-poll).
# The scratch image has no cert store — must be copied explicitly.
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# The static binary — the only executable in the image besides the cert bundle.
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/bastion /bastion

# Point reqwest/rustls at the cert bundle.
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

# Port for the /api/infer gateway (Phase 3, D-08).
EXPOSE 3000

# Volume ownership (PKG-08): the scratch image has no shell — no chown entrypoint script
# is possible. Permissions are resolved via docker-compose.yml
# `user: "${BASTION_UID:-1000}:${BASTION_GID:-1000}"`. Named volumes are initialized
# empty; the first write by the configured UID creates correct ownership — zero
# manual chmod needed.

ENTRYPOINT ["/bastion"]
# Default: daemon mode (long-running with channels active).
CMD ["daemon"]
