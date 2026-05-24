# syntax=docker/dockerfile:1.7

# --- Stage 1: web build ---------------------------------------------------
FROM node:20-bookworm-slim AS web-build
WORKDIR /repo
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/
RUN pnpm install --frozen-lockfile --filter ./web...
COPY web/ web/
RUN pnpm -C web build

# --- Stage 2: rust build --------------------------------------------------
FROM rust:1-bookworm AS server-build
WORKDIR /repo
COPY server/Cargo.toml server/Cargo.lock ./server/
COPY server/src/ ./server/src/
COPY server/migrations/ ./server/migrations/
COPY server/build.rs ./server/build.rs
COPY --from=web-build /repo/web/dist/ ./web/dist/
WORKDIR /repo/server
RUN cargo build --release --locked

# --- Stage 3: runtime -----------------------------------------------------
# debian-slim + root (see .plan/.../deployment.md §2 for the rationale).
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=server-build /repo/server/target/release/server /usr/local/bin/server
ENV DATABASE_PATH=/data/app.db
ENV RUST_LOG=info
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/server"]
