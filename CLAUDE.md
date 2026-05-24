# topic-tree-with-qav — agent orientation

A real-time host-audience interaction web app: topic tree, Q&A with voting, smooth-drawing pen whiteboards + Excalidraw boards, live cursors + click pings. Single Rust binary serves both the built React app and the WebSocket / HTTP API.

> **Start here:** `.plan/2026-05-24-amber-falcon/index.md`. The plan tree is the source of truth for architecture, protocol, data model, phases, and review workflow. Read the relevant sections before changing code.

## 1. Stack

- **Frontend**: Vite + React + TypeScript + Tailwind + shadcn/ui + Zustand. Drawing via `perfect-freehand`. Excalidraw via `@excalidraw/excalidraw`. Built static assets are *embedded* into the Rust binary (`rust-embed`).
- **Backend**: Rust + Axum on Tokio (multi-threaded runtime). `tokio-tungstenite` for ws. `rusqlite` (bundled) + `r2d2` + `refinery` for migrations. `tracing` for logs. Single binary, single process, multi-threaded.
- **Storage**: SQLite (WAL) at `$DATABASE_PATH` (default `/data/app.db` in prod, `./dev.db` in dev). Single-writer, fine for single-host design.
- **Realtime**: raw WebSockets, JSON envelopes. See `.plan/2026-05-24-amber-falcon/protocol.md` for every message.
- **Identity**: host = random `adminToken` (argon2-hashed server-side) stored in browser IndexedDB. Guests = self-issued `guestId` (UUIDv4 in localStorage) + chosen display name.
- **Hosting**: Railway (team `clankercode`, project `topic-tree-with-qav`), one service, one volume at `/data`.

## 2. Repo layout

```
topic-tree-with-qav/
├── CLAUDE.md            # this file
├── justfile             # the index — every workflow has a `just <recipe>`
├── scripts/             # any just recipe longer than ~5 lines lives here
├── web/                 # Vite + React app
├── server/              # Rust crate (axum binary + integration tests)
├── e2e/                 # Playwright suite (multi-context scenarios + visual regression)
├── .plan/               # planning tree
│   └── 2026-05-24-amber-falcon/   # current plan
├── .review/             # per-phase per-round review artifacts (gitignored content, kept folder)
├── .github/workflows/
├── Dockerfile
└── railway.toml
```

## 3. Conventions

- **TDD by default**. Red test → minimum code → green → refactor → commit. Skill: `superpowers:test-driven-development` (or `ultra-test-driven-development`).
- **Justfile-first**. Every repeatable command is a `just <recipe>`. Don't invent ad-hoc one-liners; add a recipe instead.
- **Scripts over inline**. Any recipe body longer than ~5 lines goes in `scripts/<name>.sh` and the recipe just calls it.
- **No comments unless they explain WHY** something non-obvious is being done. Don't restate the code in English.
- **Commit style**: `feat(scope): ...`, `fix(scope): ...`, `test(scope): ...`, `refactor(scope): ...`, `chore(scope): ...`. Scopes: `topictree`, `qa`, `whiteboard-pen`, `whiteboard-exc`, `cursors`, `mod`, `raise-hand`, `ws`, `db`, `ui`, `theme`, `deploy`, `ci`.
- **Single source of truth for protocol types**: Rust structs in `server/src/proto.rs` are canonical; `ts-rs` generates `web/src/proto/generated.ts`. CI fails on drift.
- **Migrations are forward-only and additive**. No destructive DDL. Two-phase for column drops.
- **Light + dark mode parity** is enforced by paired visual-regression snapshots.

## 4. Running the app

```
just dev          # concurrent: vite dev :5173 + cargo run :3000 with /api+/ws proxy
just build        # web build → cargo release build
just serve        # run the release binary against dev DB
just serve-test   # release binary, in-memory DB, debug logging, random port
```

## 5. Testing

```
just test         # everything
just test-web     # vitest only
just test-server  # cargo test only
just test-e2e     # playwright (boots binary, multi-context)
just lint         # tsc + eslint + clippy -D warnings + fmt check
just fmt          # rustfmt + prettier write
just snapshot-update     # accept current screenshots as baseline
just snapshot-baseline   # regenerate curated review screenshots into .review/
```

Skills required:
- `superpowers:test-driven-development` for every behavior change.
- `superpowers:verification-before-completion` before claiming any task done.

## 6. Review workflow (mandatory at phase boundaries)

Two tracks in parallel, then merge punch lists, then opus-4.7 `review-and-fix` subagent walks the list.

**Track A — visual** (UI-touching phases only). Dispatch both in parallel:
- An Opus 4.7 subagent — "review screenshots against design language".
- Skill `gpt-pro-run-review-dc` — ChatGPT Pro with decisive-criticism framing.

**Track B — code** (every phase). Dispatch both in parallel:
- Skill `ccc-review-cx` — Codex via the `ccc` tool reviews the diff.
- `just kimi-review` — Moonshot Kimi reviews the diff (background).

Merge punch lists with provenance (items raised by 2+ reviewers are top priority). Strike pure-taste items. Dispatch an Opus 4.7 subagent running `review-and-fix` on the merged list. Loop until exit (all PASS, or two rounds with no new items, or user override).

After every commit: invoke `postcommit-status-and-continue`.

See `.plan/2026-05-24-amber-falcon/agents-workflow.md` for prompt skeletons and `.plan/2026-05-24-amber-falcon/testing.md` §6 for the loop spec.

## 7. Skills to invoke

| When | Skill |
|---|---|
| Start of every task | `superpowers:test-driven-development` |
| Implementing a phase | `superpowers:subagent-driven-development` (preferred) or `use-subagents-impl` or `ultra-implementing-team` |
| Before claiming "done" | `superpowers:verification-before-completion` |
| Visual polish | `frontend-design` |
| After each commit | `postcommit-status-and-continue` (autonomous-continuation cadence) |
| Long-running work | `heartbeat-monitor` to keep cache warm |
| Review at phase end | dispatch the four reviewers per §6 |
| Operating in a continuous-work loop | `repeat-via-checklist-after-commit` |

## 8. Constraints + non-goals

- **Single host per room** (single SQLite writer). Multi-host is explicitly out of scope.
- **Single Railway instance** (volume = single-instance constraint). Out-of-scope to scale horizontally; documented upgrade path in `risks.md` R4.
- **No voice/video**. Click pings + cursors + raise-hand are the only live presence affordances.
- **No retention policy**. Rooms live until admin deletes them.
- **Anonymous Q is per-question, not per-session**. Presence still shows display name even when a user posts anonymously.

## 9. Defense-in-depth reminders

- `viewModeEnabled` on Excalidraw is *display-only* enforcement. Server still rejects non-admin `ExcalidrawUpdate`. **Don't drop the server check.**
- Admin token never appears in any client→client payload. It's only in the host's IndexedDB + the initial `?admin=` URL (stripped within 50ms of load).
- Rate-limit *every* client→server message type. See `protocol.md` §rate-limits.
- Anonymous questions still store `guest_id` server-side (not surfaced to clients) so moderation works.

## 10. When you're stuck

- Re-read the relevant section of the plan tree.
- Run `just test-<scope>` to confirm the failing surface.
- If a design call needs human input: don't guess — surface the question and stop.
- If a fix would require changing the plan: update the plan doc *in the same commit* as the code change. Plan + docs + code stay in sync.

## 11. Repo norms (from user CLAUDE.md)

- `justfile` is the index. Add recipes liberally; >5-line bodies go in `scripts/`.
- Limit builds/tests to 2 threads where parallelism is configurable (`cargo build -j 2`, `cargo test -j 2`, `pnpm -C web test --threads 2`). Recipes already pass these flags.
- `attn` CLI is available to notify the user — use sparingly, only on blockers or when finished after a long-running task. Spoken-language friendly: no symbols, no jargon.
