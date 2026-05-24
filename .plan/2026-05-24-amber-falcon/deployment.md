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
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/
RUN pnpm install --frozen-lockfile --filter ./web...
COPY web/ web/
RUN pnpm -C web build

# --- Stage 2: rust build
FROM rust:1-bookworm AS server-build
WORKDIR /repo
COPY server/Cargo.toml server/Cargo.lock ./server/
COPY server/src/ ./server/src/
COPY server/migrations/ ./server/migrations/
COPY server/build.rs ./server/build.rs
COPY --from=web-build /repo/web/dist/ ./web/dist/
WORKDIR /repo/server
RUN cargo build --release --locked

# --- Stage 3: runtime (debian-slim, NOT distroless-nonroot)
# Why slim+root over distroless+nonroot: Railway volumes mount as root:root, and a non-root
# distroless image cannot write to /data without the Railway-specific RAILWAY_RUN_UID
# mechanism, which still requires the binary to be readable by that UID. Running as root
# inside an isolated Railway container is the path of least friction for v1.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=server-build /repo/server/target/release/server /usr/local/bin/server
ENV DATABASE_PATH=/data/app.db
ENV RUST_LOG=info
# Note: PORT is injected by Railway at runtime; locally defaults to 3000 via the binary.
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/server"]
```

The binary binds `0.0.0.0:${PORT:-3000}`. Locally `just serve` exports `PORT=3000`; in Railway, `$PORT` is provided by the platform and the binary picks it up.

## 3. `railway.toml`

```toml
[build]
builder = "DOCKERFILE"
dockerfilePath = "Dockerfile"

[deploy]
healthcheckPath = "/healthz"
healthcheckTimeout = 30
restartPolicyType = "ON_FAILURE"
restartPolicyMaxRetries = 5
```

`healthcheckTimeout = 30` because cold-start of the Rust binary + first-time SQLite migrations on a freshly-mounted volume can exceed 5s on a small Railway instance. `/healthz` itself returns immediately (no DB call) once the server has bound.

## 4. Volume + env

| Env var | Value (prod) | Why |
|---|---|---|
| `PORT` | injected by Railway | Binary reads `$PORT` and binds `0.0.0.0:$PORT`. Local default is 3000 via the binary, not the Dockerfile. |
| `DATABASE_PATH` | `/data/app.db` | Persistent volume mount. |
| `RUST_LOG` | `info,server=debug` | Structured logs. |
| `PUBLIC_ORIGIN` | `https://<domain>` | For absolute join URLs in admin banner. Optional; if unset, server derives from `Host` header. |
| `RAILWAY_RUN_UID` | (Railway-provided) | If we later switch to non-root, this is the documented mechanism. v1 runs as root in-container. |

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

All of these are wrapped in `scripts/railway-init.sh` for idempotent re-runs (`just railway-init`).

```
# from machine with railway + gh auth set up

gh auth status                                                  # confirm clankercode push access
gh repo create clankercode/topic-tree-with-qav --public --source . --remote origin --push

railway login                                                   # if not already
railway init --name topic-tree-with-qav                         # in project root
                                                                # (Railway CLI prompts will offer team selection;
                                                                #  pick or create "clankercode")
railway volume add                                              # interactive: name=data, mount=/data, size=1GB
railway variables --set DATABASE_PATH=/data/app.db --set RUST_LOG=info
railway up                                                      # initial deploy
railway domain                                                  # capture <slug>.up.railway.app
gh secret set RAILWAY_TOKEN -R clankercode/topic-tree-with-qav  # for CI deploy
```

(The exact Railway CLI verbs have shifted across versions — `railway volume add` is the current shape. `scripts/railway-init.sh` should `--help`-check the local CLI on first run and fall back to a printed instruction with a link to the Railway docs if the verbs have moved.)

## 8. Observability in prod

- Railway's built-in logs + metrics suffice for v1.
- `tracing-subscriber` with `EnvFilter` reading `RUST_LOG`. JSON formatter in prod (`tracing-subscriber::fmt().json()`), pretty in dev.
- Optional later: ship logs to an external collector (Axiom, Logflare). Out of v1 scope.

## 9. GitHub Pages (docs)

`docs/` is a separate static site (Vitepress) deployed to GitHub Pages via Actions. See `phases.md` Phase 9.5 for the full task list. Workflow:

```yaml
# .github/workflows/pages.yml
name: pages
on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: extractions/setup-just@v2
      - uses: pnpm/action-setup@v3
      - uses: actions/setup-node@v4
        with: { node-version: 20, cache: pnpm }
      - uses: dtolnay/rust-toolchain@stable
      - run: pnpm install --frozen-lockfile
      - run: pnpm -C e2e exec playwright install --with-deps chromium
      - run: just build
      - run: just test-e2e-only docs-screenshots.spec.ts
      - run: just docs-build
      - uses: actions/upload-pages-artifact@v3
        with:
          path: docs/.vitepress/dist/

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

Pages source is set to "GitHub Actions" (one-time, via `scripts/gh-repo-meta.sh` / `gh api`).

## 10. Rollback

- Railway keeps prior deployments addressable; "rollback to previous" via CLI: `railway redeploy <deployment-id>`.
- DB migrations are forward-only by default. For roll-forward safety, every migration is *additive* (no destructive DDL). If a column needs to be dropped, two-phase: deprecate-and-stop-writing in N, drop in N+1.
