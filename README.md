# topic-tree-with-qav

Real-time host-audience interaction: topic tree, Q&A with voting, collaborative whiteboards (pen + Excalidraw), live cursors, and click pings. Single Rust binary serves both the built React app and the WebSocket/HTTP API.

## Quickstart

```bash
# install dependencies
just setup

# run in dev mode (vite + rust server, concurrent)
just dev
```

Open `http://localhost:5173` in your browser. Create a room as a host, share the guest link with your audience.

For more dev commands:

| Command | Description |
|---------|-------------|
| `just dev-web` | Vite dev server only |
| `just dev-server` | Rust server only (no embedded assets) |
| `just serve` | Run release binary against dev DB |
| `just serve-test` | Release binary with temp DB + debug logging |

## Tech Stack

- **Frontend**: Vite + React + TypeScript + Tailwind + shadcn/ui + Zustand
- **Backend**: Rust + Axum + Tokio + SQLite (rusqlite)
- **Realtime**: raw WebSockets, JSON envelopes
- **Whiteboards**: `perfect-freehand` (pen), `@excalidraw/excalidraw` (Excalidraw boards)
- **Embedding**: `rust-embed` — built static assets are embedded in the Rust binary

## Project Structure

```
topic-tree-with-qav/
├── web/                 # Vite + React app
├── server/              # Rust crate (axum binary + integration tests)
├── e2e/                 # Playwright suite
├── justfile             # Workflow index (run `just` to see all recipes)
├── CLAUDE.md            # Agent orientation
└── .plan/               # Planning tree
```

## Testing

```bash
just test          # all layers: vitest + cargo test + playwright
just test-web      # vitest only
just test-server   # cargo test only
just test-e2e      # playwright e2e suite
```

## Linting

```bash
just lint          # tsc + eslint + clippy + rustfmt
just fmt           # format code (prettier + rustfmt)
```

## Deployment (Railway)

The app is deployed on Railway with a persistent volume at `/data`.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3000` | HTTP server port |
| `DATABASE_PATH` | `/data/app.db` | SQLite database path |
| `RUST_LOG` | `info` | Logging level |

### Commands

```bash
just railway-init      # one-time: create team + project + volume
just railway-deploy     # deploy current commit
just railway-logs       # tail production logs
just railway-open       # open production URL
```

### Backup

```bash
# local dev DB
just db-dump

# production DB (from Railway)
just db-dump-railway > backups/prod-$(date +%Y%m%d).db.tar.gz
```

## Contributing

1. Run `just setup` to install dependencies
2. Run `just dev` to start in dev mode
3. Make changes, run `just lint` to check
4. Run `just test` to verify all tests pass
5. Commit with conventional format: `feat(scope): ...`, `fix(scope): ...`, etc.

See `CLAUDE.md` for agent orientation and `.plan/` for the full project plan.
