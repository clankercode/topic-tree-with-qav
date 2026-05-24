# Deployment

## 1. Hosting target

**Railway**, single service per env, persistent volume. The `railway` CLI is available.

- Railway team: `clankercode` (created if not present).
- Railway project: `topic-tree-with-qav`.

Two environments planned:
- `production` — `main` branch auto-deploys
- `preview` — manual deploys via `railway up` for branch testing

**Domain**: Railway's `*.up.railway.app` to start. Optionally CNAME a subdomain under `xk.io` (e.g. `topics.xk.io`) at end of Phase 9. Custom domain setup is a Railway dashboard action + DNS record; no code change.

## 2. Dockerfile

```dockerfile
# syntax=docker/dockerfile:1.7

# --- Stage 1: web build
FROM node:20-bookworm-slim AS web-build
WORKDIR /repo
RUN corepack enable
COPY pnpm-lock.yaml package.json pnpm-workspace.yaml ./
COPY web/package.json web/
RUN pnpm install --frozen-lockfile
COPY web/ web/
RUN pnpm -C web build

# --- Stage 2: rust build
FROM rust:1-bookworm AS server-build
WORKDIR /repo
COPY server/Cargo.toml server/Cargo.lock server/
COPY server/src/ server/src/
COPY server/migrations/ server/migrations/
# Bring in built static assets for rust-embed.
COPY --from=web-build /repo/web/dist/ web/dist/
WORKDIR /repo/server
RUN cargo build --release --locked

# --- Stage 3: runtime
FROM gcr.io/distroless/cc-debian12
COPY --from=server-build /repo/server/target/release/server /usr/local/bin/server
ENV PORT=3000
ENV DATABASE_PATH=/data/app.db
ENV RUST_LOG=info
EXPOSE 3000
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/server"]
```

## 3. `railway.toml`

```toml
[build]
builder = "DOCKERFILE"
dockerfilePath = "Dockerfile"

[deploy]
healthcheckPath = "/healthz"
healthcheckTimeout = 5
restartPolicyType = "ON_FAILURE"
restartPolicyMaxRetries = 5
```

## 4. Volume + env

| Env var | Value (prod) | Why |
|---|---|---|
| `PORT` | injected by Railway | Axum binds to this. |
| `DATABASE_PATH` | `/data/app.db` | Persistent volume mount. |
| `RUST_LOG` | `info,server=debug` | Structured logs. |
| `PUBLIC_ORIGIN` | `https://<domain>` | For absolute join URLs in admin banner. Optional; if unset, server derives from `Host` header. |

Volume: 1GB at `/data` (Railway default storage). SQLite plus WAL stays comfortably below this for thousands of rooms with realistic data.

## 5. Single-instance constraint

SQLite + a Railway volume = **horizontal scaling not supported** without architectural change. The plan accepts this:
- One instance handles the spec's audience size (≤ 50 per room, hundreds of concurrent rooms).
- If we outgrow, the upgrade path is documented in `risks.md` (move to Postgres + Redis-backed pubsub).

## 6. CI → deploy

`.github/workflows/ci.yml`:

```yaml
name: ci
on:
  pull_request:
  push:
    branches: [main]

jobs:
  lint-and-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v3
      - uses: actions/setup-node@v4
        with: { node-version: 20, cache: pnpm }
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy, rustfmt }
      - run: pnpm install --frozen-lockfile
      - run: just lint
      - run: just test-web
      - run: just test-server
      - run: just test-e2e
```

`deploy.yml` (separate, on push to `main`):

```yaml
name: deploy
on:
  push:
    branches: [main]

jobs:
  railway:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm i -g @railway/cli
      - run: railway up --service topic-tree-with-qav --detach
        env:
          RAILWAY_TOKEN: ${{ secrets.RAILWAY_TOKEN }}
```

## 7. First-time setup checklist

```
# from machine with railway + gh auth set up

gh auth status                              # confirm clankercode push access
gh repo create clankercode/topic-tree-with-qav --public --source . --remote origin --push

railway login                               # if not already
railway init                                # in project root; name: topic-tree-with-qav
railway volume create data /data            # 1GB
railway variables set PORT=3000 DATABASE_PATH=/data/app.db RUST_LOG=info
railway up                                  # initial deploy
railway domain                              # capture <slug>.up.railway.app
gh secret set RAILWAY_TOKEN -R clankercode/topic-tree-with-qav   # for CI deploy
```

## 8. Observability in prod

- Railway's built-in logs + metrics suffice for v1.
- `tracing-subscriber` with `EnvFilter` reading `RUST_LOG`. JSON formatter in prod (`tracing-subscriber::fmt().json()`), pretty in dev.
- Optional later: ship logs to an external collector (Axiom, Logflare). Out of v1 scope.

## 9. Rollback

- Railway keeps prior deployments addressable; "rollback to previous" via CLI: `railway redeploy <deployment-id>`.
- DB migrations are forward-only by default. For roll-forward safety, every migration is *additive* (no destructive DDL). If a column needs to be dropped, two-phase: deprecate-and-stop-writing in N, drop in N+1.
