# Architecture

## 1. Topology

```
┌──────────────────────────────── Railway service (1 container) ──────────────────────────────┐
│                                                                                              │
│   ┌─────────────────────────  Rust binary (axum) ─────────────────────────┐                  │
│   │                                                                       │                  │
│   │  ┌─────────────┐   ┌─────────────┐   ┌─────────────────────────────┐  │                  │
│   │  │ HTTP routes │   │ /ws handler │   │ static asset serve (embed)  │  │                  │
│   │  └─────┬───────┘   └──────┬──────┘   └───────────┬─────────────────┘  │                  │
│   │        │                  │                      │                    │                  │
│   │        ▼                  ▼                      ▼                    │                  │
│   │  ┌─────────────────────  AppState (Arc) ──────────────────────────┐   │                  │
│   │  │  rooms: DashMap<RoomId, Arc<RoomActor>>                        │   │                  │
│   │  │  db:    Pool<SqliteConnectionManager>                          │   │                  │
│   │  └────────────────────────────────────────────────────────────────┘   │                  │
│   │                                                                       │                  │
│   │  Per-room tokio task ("RoomActor"):                                   │                  │
│   │    - owns broadcast::Sender<ServerMsg> for all clients                │                  │
│   │    - applies inbound events, validates auth, persists, fans out      │                  │
│   │    - drives throttle for cursor/stroke broadcasts                     │                  │
│   │                                                                       │                  │
│   └───────────────────────────────────────────────────────────────────────┘                  │
│                                                                                              │
│   ┌──────────────── Railway volume mounted at /data ──────────────────────────┐              │
│   │    /data/app.db          (SQLite, WAL mode)                                │              │
│   │    /data/app.db-wal      (write-ahead log)                                 │              │
│   │    /data/app.db-shm                                                        │              │
│   └────────────────────────────────────────────────────────────────────────────┘              │
└──────────────────────────────────────────────────────────────────────────────────────────────┘

         ▲   ▲   ▲
    HTTPS│   │   │ WSS
         │   │   │
   ┌─────┴───┴───┴──────┐
   │ Browsers (React)    │  Host (admin) + N guests per room
   └─────────────────────┘
```

## 2. Why one binary

- Static React build is embedded via `rust-embed` or `axum-embed` and served on `GET /*`.
- All API routes live under `/api/*`.
- WebSocket endpoint: `GET /ws?room=<id>` (upgrade).
- A single executable + a single volume = trivial Railway deployment.
- No CORS, no separate origins, no SPA-vs-API auth weirdness.

## 3. Concurrency model

- **One tokio task per WebSocket connection** for read; another for write (split sink/stream).
- **One "RoomActor" task per room**, held in `DashMap<RoomId, Arc<RoomActor>>`. Lazily spawned on first connect; reaped after N minutes idle if zero connections.
- Inbound client messages → routed to room actor via an mpsc channel.
- Outbound broadcasts → `tokio::sync::broadcast::Sender<ServerMsg>` cloned to each connection writer.
- DB writes batched/serialized through a single tokio task with a small mpsc queue to keep SQLite single-writer happy (WAL gets us concurrent reads).

## 4. Authoritative state

- Server is the source of truth. Clients send **intents** (`SubmitQuestion`, `AddStroke`, `SetActiveTopic`, ...). Server validates, applies, broadcasts the **outcome** event to all subscribers, persists where appropriate.
- Optimistic UI on client is fine for low-stakes events (strokes, cursor moves); server confirmation overrides.
- Cursor + stroke-point streams are *not* persisted; only stroke completions and full snapshots are.

## 5. Throttling + backpressure

- **Cursor moves**: client throttles to 20Hz before send.
- **Stroke points**: client sends raw pointer events but batches each frame's points into one message at requestAnimationFrame cadence.
- **Server fanout**: `broadcast` channel with capacity 256. If a slow client lags, drop oldest with a warning; client refetches snapshot on reconnect.
- **Snapshot strategy**: on connect or after a dropped frame, client requests `GetRoomSnapshot` and gets the canonical state.

## 6. Asset pipeline

- `pnpm` workspace at repo root with `web/` (the React app).
- `cargo build --release` runs after `pnpm -C web build` (handled by `just build`).
- Production binary statically embeds `web/dist/`.
- Dev mode: `vite dev` on `:5173` + `cargo run` on `:3000` with a Vite proxy for `/api` and `/ws`.

## 7. Logging + observability

- `tracing` + `tracing-subscriber` (JSON in prod, pretty in dev).
- Per-request span with `room_id`, `client_id`.
- `/healthz` endpoint for Railway healthcheck.
- `/metrics` (Prometheus text format) — connection count, room count, msg throughput. Defer scrape integration to v1.1.

## 8. Repository layout

```
topic-tree-with-qav/
├── justfile
├── scripts/                # any just recipe >5 lines
│   ├── dev.sh
│   ├── ci-e2e.sh
│   └── visual-review.sh
├── server/                 # Rust crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── http.rs
│   │   ├── ws.rs
│   │   ├── room.rs
│   │   ├── db.rs
│   │   ├── proto.rs       # serde types — single source of truth
│   │   ├── auth.rs
│   │   └── moderation.rs
│   └── tests/
│       └── integration_*.rs
├── web/                    # Vite + React app
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── tailwind.config.ts
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── routes/
│   │   ├── components/
│   │   ├── ws/             # ws client + reconnect
│   │   ├── store/          # zustand stores
│   │   ├── proto/          # generated/checked-in proto types
│   │   ├── whiteboard/
│   │   ├── topictree/
│   │   ├── qa/
│   │   └── theme/
│   └── tests/
│       └── *.spec.ts       # vitest
├── e2e/
│   ├── playwright.config.ts
│   ├── tests/
│   │   ├── room-lifecycle.spec.ts
│   │   ├── topic-tree.spec.ts
│   │   ├── qa.spec.ts
│   │   ├── whiteboard-pen.spec.ts
│   │   ├── whiteboard-excalidraw.spec.ts
│   │   └── moderation.spec.ts
│   └── screenshots/        # baselines, ignored on first run
├── .plan/
│   └── 2026-05-24-amber-falcon/
├── .github/workflows/
│   └── ci.yml
├── Dockerfile
├── railway.toml
└── README.md
```

## 9. Proto types: single source of truth

- Rust `serde` structs in `server/src/proto.rs` are canonical.
- `ts-rs` crate generates `web/src/proto/generated.ts` at build time (via `cargo test` in CI to verify drift).
- All client/server message shapes derive from these.

## 10. Build + deploy artifact

- Multi-stage Dockerfile:
  1. `node:20-alpine` — `pnpm install --frozen-lockfile && pnpm -C web build`.
  2. `rust:1-bookworm` — `cargo build --release` with `web/dist/` copied in for embed.
  3. `gcr.io/distroless/cc-debian12` — final image, single `server` binary, ~20MB.
- Railway service uses the Dockerfile, mounts `/data` volume, sets `PORT`, `DATABASE_PATH=/data/app.db`, `RUST_LOG=info`.
