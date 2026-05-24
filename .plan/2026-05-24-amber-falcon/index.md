# topic-tree-with-qav — plan

**Repo**: `clankercode/topic-tree-with-qav` (to be created)
**Plan ID**: `2026-05-24-amber-falcon`
**Date**: 2026-05-24

## 1. Product summary

A web app for **host-led, audience-interactive sessions**.

- Host creates a room, gets a join link + an admin link. Admin credentials persist in browser IndexedDB so the host can resume any room they ever created.
- Guests open the join link, enter a display name, and join — no auth.
- All state syncs over WebSockets to all clients in a room in real time.
- Three feature pillars:
  - **Topic tree**: pre-authored agenda the host walks during the session; active/done state visible to all.
  - **Q&A**: guests ask questions (optionally anonymous), vote others up, host marks answered.
  - **Whiteboards**: host draws (smooth strokes + text); also can create Excalidraw boards. Guests follow the focused board or pick any board to view. Click-pings + cursor positions visible.

## 2. Locked decisions

| Area | Decision |
|---|---|
| Frontend | **Vite + React + TypeScript + Tailwind + shadcn/ui** |
| Backend | **Rust + Axum + tokio-tungstenite + rusqlite** (single binary). OCaml/Dream considered and rejected — weaker ws + sqlite story, smaller pool of reviewable code, slower iteration for the JSON/ws-heavy surface. |
| Persistence | **SQLite (rusqlite, WAL mode) on Railway volume** at `/data/app.db` |
| Real-time transport | **Raw WebSockets**, length-delimited JSON frames, one connection per client |
| Drawing | **Custom canvas + `perfect-freehand`** for pen strokes; plain text layer overlaid via positioned `<div>`s |
| Excalidraw | **Excalidraw embedded** (`@excalidraw/excalidraw`); collab broadcast handled by our own Rust relay over our ws (no separate Node sidecar) |
| Auth | Admin = random 32-byte `adminToken` issued at room creation, stored in browser IndexedDB, sent on each admin action. Guests = self-issued `guestId` (UUIDv4 in localStorage) + chosen display name. |
| Theming | Tailwind class strategy + `prefers-color-scheme` default + manual toggle |
| Hosting | **Railway**, single service, Dockerfile multi-stage build, persistent volume mounted at `/data` |
| Test stack | Vitest (frontend unit), `cargo test` + axum-test (backend), Playwright (e2e + visual regression) |
| Review loops | **Two parallel tracks per phase.** Track A (visual): opus-4.7 subagent + `gpt-pro-run-review-dc` (ChatGPT Pro decisive-criticism). Track B (code): `ccc-review-cx` (Codex) + `kimi --print` (Moonshot). Synthesized punch list handed to an opus-4.7 `review-and-fix` subagent. |
| Repo entrypoint | `justfile` + `scripts/` for anything >5 lines |

## 3. Document map

| Doc | Purpose |
|---|---|
| [`architecture.md`](./architecture.md) | System diagram, processes, threads, deployment topology |
| [`protocol.md`](./protocol.md) | WebSocket message protocol — every message type, shape, direction |
| [`data-model.md`](./data-model.md) | SQLite schema + IndexedDB client-side schema |
| [`frontend.md`](./frontend.md) | Routes, components, design language, theming, key UX details |
| [`whiteboards.md`](./whiteboards.md) | Both whiteboard types — drawing pipeline, excalidraw relay, cursors |
| [`testing.md`](./testing.md) | Full test strategy: TDD layers, e2e patterns, screenshot regression, visual review |
| [`phases.md`](./phases.md) | Phase-by-phase implementation breakdown with TDD task lists |
| [`agents-workflow.md`](./agents-workflow.md) | Which skills + subagents drive each phase; review-and-fix + postcommit-status-and-continue rhythm |
| [`deployment.md`](./deployment.md) | Dockerfile, railway.toml, env, volume, CI, first-time setup |
| [`risks.md`](./risks.md) | Opinionated callouts of things that could go wrong + open questions to confirm |

## 4. Confirmed answers (was: open questions)

1. **GitHub org access**: clankercode is accessible — confirmed.
2. **Railway**: create a new Railway team and project. Team = `clankercode`, project = `topic-tree-with-qav`.
3. **Domain**: Railway subdomain to start; optionally swap to a subdomain under `xk.io` (e.g. `topics.xk.io` or a random `abcd.xk.io`) — can be added at the end of phase 9 without code changes.
4. **Excalidraw license**: MIT — confirmed.
5. **Raise-hand feature** *(was R12 §1)*: ADOPTED. Each raise-hand carries a 1–10 word topic. See `protocol.md` §raise-hand and `phases.md` Phase 7.
6. **Q&A ↔ topic-tree linking** *(was R12 §2)*: free-form `topic_id` on questions is overkill — dropped. Instead: a one-click **"promote question to topic"** button on each Q&A item creates a new topic-tree node from the question and removes it from the Q&A list. Questions otherwise live independently.
7. **Retention** *(was R12 §3)*: retain forever; only deletion is admin-driven. YAGNI on auto-archive.
8. **Multiple hosts** *(was R12 §4)*: NO. Single host enforced. SQLite single-writer fits. Webserver remains multi-threaded (Tokio worker pool) and serves both HTML and API — already in the design.

---

--- SUMMARY ---

- **What we are building**: a real-time host-audience web app with topic tree, Q&A, and two flavors of whiteboard (custom-canvas pen + Excalidraw), syncing over raw WebSockets, deployed as a single Rust binary + static React frontend on Railway.
- **Stack**: Vite/React/TS/Tailwind/shadcn front-end; Rust/Axum back-end serving both the API+ws and the built static assets; SQLite on a Railway volume; no Node runtime in production.
- **Identity model**: host gets a `roomId` + `adminToken` saved to browser IndexedDB so the host can re-enter any room they ever created from the same device; guests just enter a display name (a stable `guestId` is generated client-side for vote dedup and moderation).
- **Real-time model**: one ws per client; server is authoritative; admin actions require `adminToken`; broadcasts are room-scoped; cursor/click pings are throttled to ~20Hz and not persisted.
- **Whiteboards**: host can create either a *pen board* (custom canvas, `perfect-freehand` strokes + text layer) or an *Excalidraw board* (full Excalidraw widget, our Rust server relays scene operations and pointer updates between clients — no Node sidecar). Default permission: only host edits; guests view + click-ping + share cursor positions.
- **Testing**: TDD throughout, three layers (unit, integration, e2e); Playwright drives multi-client scenarios; `toHaveScreenshot` for visual regression. Every phase ends with a parallel **multi-agent review** with two tracks: Track A visual (opus-4.7 subagent + ChatGPT-Pro decisive-criticism via `gpt-pro-run-review-dc`) and Track B code (`/ccc-review-cx` Codex + Moonshot Kimi via `kimi --print --yolo --thinking`). Merged punch list goes to an opus-4.7 subagent running `review-and-fix` until clean.
- **Workflow per phase**: brainstorm if anything unknown → write tests → implement (subagent-driven, dispatched in parallel where independent) → e2e → multi-agent review (Tracks A + B in parallel) → opus-4.7 `review-and-fix` loop on merged punch list → commit → `postcommit-status-and-continue` decides whether to continue.
- **Deployment**: Dockerfile multi-stage (Node build → Rust build → distroless runtime); Railway with `/data` volume for SQLite; GitHub Actions runs tests on every PR; main auto-deploys.
- **Repo norms**: `justfile` is the index — every repeated workflow has a `just <name>` entry; logic >5 lines lives in `scripts/`.
- **Hidden risks called out** in `risks.md`: Excalidraw relay correctness, screenshot flakiness in Playwright, ws backpressure under high stroke rate, Railway volume single-instance constraint, adminToken in URL footgun on first share.
