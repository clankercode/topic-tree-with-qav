# topic-tree-with-qav — workflow index.
# Read CLAUDE.md first. Plan lives in .plan/2026-05-24-amber-falcon/.
# >5-line recipe bodies belong in scripts/<name>.sh and get called from here.

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments
export PATH := env_var("HOME") + "/.local/bin:" + env_var("PATH")

# threads cap: matches user system policy (see ~/CLAUDE.md "limit to 2 threads")
CARGO_BUILD_JOBS := "2"
CARGO_TEST_THREADS := "2"

# default ports
WEB_PORT := "5173"
SERVER_PORT := "3000"

# ──────────────────────────────────────────────────────────────────────────────
# meta
# ──────────────────────────────────────────────────────────────────────────────

# list all recipes
default:
    @just --list --unsorted

# show every recipe with its description
help:
    @just --list --unsorted

# print the plan index location
plan:
    @echo ".plan/2026-05-24-amber-falcon/index.md"
    @ls .plan/2026-05-24-amber-falcon/

# ──────────────────────────────────────────────────────────────────────────────
# setup
# ──────────────────────────────────────────────────────────────────────────────

# install all deps (web + cargo registry warmup)
setup:
    bash scripts/ensure-pnpm.sh
    pnpm install --frozen-lockfile
    cargo fetch --manifest-path server/Cargo.toml

# install Playwright browsers (chromium only)
setup-playwright:
    pnpm -C e2e exec playwright install --with-deps chromium

# ──────────────────────────────────────────────────────────────────────────────
# dev
# ──────────────────────────────────────────────────────────────────────────────

# concurrent vite dev + cargo run (run in two terminals if you want logs separate)
dev:
    bash scripts/dev.sh

# vite dev server only
dev-web:
    pnpm -C web dev --host --port {{WEB_PORT}}

# rust server only, dev mode (no embedded assets; expects vite proxy on {{WEB_PORT}})
dev-server:
    RUST_LOG=info,server=debug DATABASE_PATH=./dev.db PORT={{SERVER_PORT}} \
        cargo run --manifest-path server/Cargo.toml -j {{CARGO_BUILD_JOBS}}

# ──────────────────────────────────────────────────────────────────────────────
# build
# ──────────────────────────────────────────────────────────────────────────────

# full release build: web → embed → cargo release
build:
    pnpm -C web build
    cargo build --release --manifest-path server/Cargo.toml -j {{CARGO_BUILD_JOBS}}

# clean all build artifacts
clean:
    rm -rf web/dist e2e/test-results e2e/playwright-report .review/_tmp
    cargo clean --manifest-path server/Cargo.toml

# ──────────────────────────────────────────────────────────────────────────────
# run
# ──────────────────────────────────────────────────────────────────────────────

# run the release binary against the dev DB
serve:
    RUST_LOG=info DATABASE_PATH=./dev.db PORT={{SERVER_PORT}} \
        ./server/target/release/server

# release binary with a per-invocation temp-file DB, debug logging, random port (used by Playwright)
# (we don't use :memory: because r2d2 connection-pooling would split connections across
# independent in-memory DBs — see .plan/.../data-model.md "Read/write split" + risks.md)
serve-test:
    bash scripts/serve-test.sh

# ──────────────────────────────────────────────────────────────────────────────
# test
# ──────────────────────────────────────────────────────────────────────────────

# run every test layer
test: test-web test-server test-e2e

# vitest
test-web:
    pnpm -C web test --run

# cargo test (single-threaded inside each binary for log clarity; capped overall)
test-server:
    cargo test --manifest-path server/Cargo.toml -j {{CARGO_BUILD_JOBS}} -- --test-threads {{CARGO_TEST_THREADS}}

# playwright (boots release binary via webServer in playwright.config.ts)
test-e2e: build
    pnpm -C e2e test

# playwright on one file
test-e2e-only file:
    pnpm -C e2e test {{file}}

# accept current screenshots as the new baseline
snapshot-update: build
    pnpm -C e2e test --update-snapshots

# regenerate curated review screenshots into .review/_baseline/
snapshot-baseline: build
    bash scripts/snapshot-baseline.sh

# ──────────────────────────────────────────────────────────────────────────────
# lint + format
# ──────────────────────────────────────────────────────────────────────────────

lint: lint-web lint-server

lint-web:
    pnpm -C web typecheck
    pnpm -C web lint

lint-server:
    cargo fmt --manifest-path server/Cargo.toml -- --check
    cargo clippy --manifest-path server/Cargo.toml --all-targets -j {{CARGO_BUILD_JOBS}} -- -D warnings

fmt:
    pnpm -C web format
    cargo fmt --manifest-path server/Cargo.toml

# ──────────────────────────────────────────────────────────────────────────────
# review loops (see .plan/.../testing.md §6 and agents-workflow.md §4)
# ──────────────────────────────────────────────────────────────────────────────

# orchestrate the full per-phase quad-review: visual + code, both tracks parallel.
# This recipe only runs Track B (code). Track A (visual) is dispatched by the leader
# agent via the Agent tool, since it requires Claude subagent + ChatGPT Pro skills.
review-code phase round:
    bash scripts/review-code.sh "{{phase}}" "{{round}}"

# kimi code review against current diff
kimi-review phase="" round="":
    bash scripts/kimi-review.sh "{{phase}}" "{{round}}"

# clean review artifacts older than 30 days
review-prune:
    find .review -type d -name 'phase-*' -mtime +30 -print -exec rm -rf {} +

# ──────────────────────────────────────────────────────────────────────────────
# database (SQLite)
# ──────────────────────────────────────────────────────────────────────────────

# write a tarball backup of the dev DB
db-dump out="dev-db-backup.tar.gz":
    tar -czf {{out}} dev.db dev.db-wal dev.db-shm 2>/dev/null || tar -czf {{out}} dev.db
    @echo "wrote {{out}}"

# print the current migration list
db-migrations:
    @ls server/migrations/ 2>/dev/null || echo "no migrations dir yet"

# open the dev DB in sqlite3 REPL
db-shell:
    sqlite3 ./dev.db

# ──────────────────────────────────────────────────────────────────────────────
# docs site (GitHub Pages, Vitepress)
# ──────────────────────────────────────────────────────────────────────────────

# dev-serve the docs site
docs-dev:
    pnpm -C docs dev

# build the docs site (copies fresh screenshots from e2e/screenshots/_docs/ first)
docs-build:
    bash scripts/docs-build.sh

# regenerate docs screenshots via the dedicated Playwright suite
docs-screenshots: build
    pnpm -C e2e test docs-screenshots.spec.ts

# ──────────────────────────────────────────────────────────────────────────────
# GitHub repo metadata (description / homepage / topics / pages source)
# ──────────────────────────────────────────────────────────────────────────────

# apply description + homepage + topics + Pages source to the GitHub repo (idempotent)
gh-meta:
    bash scripts/gh-repo-meta.sh

# show what gh-meta would set (no-op)
gh-meta-dry:
    bash scripts/gh-repo-meta.sh --dry-run

# ──────────────────────────────────────────────────────────────────────────────
# deploy (Railway)
# ──────────────────────────────────────────────────────────────────────────────

# one-time: create team + project + volume + env. Re-runnable (idempotent intent).
railway-init:
    bash scripts/railway-init.sh

# deploy current commit to Railway
railway-deploy:
    railway up --detach

# tail prod logs
railway-logs:
    railway logs --service topic-tree-with-qav

# open the prod URL in a browser
railway-open:
    railway open

# ──────────────────────────────────────────────────────────────────────────────
# CI mirrors
# ──────────────────────────────────────────────────────────────────────────────

# what CI runs on every PR
ci: lint test

# what CI runs to gate a deploy
ci-deploy: ci build

# e2e + visual diff in CI environment
ci-e2e:
    bash scripts/ci-e2e.sh

# ──────────────────────────────────────────────────────────────────────────────
# convenience
# ──────────────────────────────────────────────────────────────────────────────

# regenerate ts proto types from rust serde definitions
proto-gen:
    cargo test --manifest-path server/Cargo.toml -j {{CARGO_BUILD_JOBS}} --features ts-gen proto_export -- --nocapture

# pop the .plan tree into less
plan-read:
    less .plan/2026-05-24-amber-falcon/index.md
