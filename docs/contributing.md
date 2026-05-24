# Contributing

## Dev Quickstart

```bash
git clone https://github.com/clankercode/topic-tree-with-qav
cd topic-tree-with-qav
just setup        # install deps
just dev          # start vite + rust server concurrently
```

Open http://localhost:5173.

## Stack

- **Frontend**: Vite + React + TypeScript + Tailwind + shadcn/ui + Zustand
- **Backend**: Rust + Axum + tokio-tungstenite + rusqlite
- **Testing**: vitest (unit), Playwright (e2e)
- **Docs**: VitePress

## Workflow

- **TDD by default**: red test → minimum code → green → refactor → commit.
- **Justfile-first**: every repeatable command is a `just <recipe>`. Recipes with bodies >5 lines go in `scripts/`.
- **Single source of truth**: Rust structs in `server/src/proto.rs` are canonical; `ts-rs` generates `web/src/proto/generated.ts`. CI fails on drift.

## Running Tests

```bash
just test           # all layers
just test-web      # vitest only
just test-server   # cargo test only
just test-e2e     # playwright (full suite)
just test-e2e-only docs-screenshots.spec.ts  # single file
```

## Building

```bash
just build          # web + rust release
just lint           # tsc + eslint + clippy -D warnings + fmt check
just fmt            # write formatted code
```

## Commit Style

Format: `feat(scope): ...`

Scopes: `topictree`, `qa`, `whiteboard-pen`, `whiteboard-exc`, `cursors`, `mod`, `raise-hand`, `ws`, `db`, `ui`, `theme`, `deploy`, `ci`

## Migrations

Migrations are forward-only and additive. No destructive DDL. Two-phase for column drops.

```bash
just db-migrations   # list migrations
just db-shell       # open sqlite3 REPL
```

## Protocol

All message types are defined in `server/src/proto.rs`. Run `just proto-gen` after changing proto definitions to regenerate TypeScript types.

## CI

```bash
just ci        # lint + test (what PRs run)
just ci-deploy # lint + test + build (what gates a deploy)
```

## Review

After each commit in an autonomous loop, run `just kimi-review` or dispatch the full quad-review per the CLAUDE.md §6.
