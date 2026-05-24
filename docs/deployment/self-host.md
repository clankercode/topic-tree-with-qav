# Self-Hosting

## Requirements

- Rust 1.75+ (stable)
- Node.js 20+ and pnpm 10+
- SQLite (bundled via `rusqlite`; no system SQLite required)

## Building

```bash
just build
```

This produces a release binary at `server/target/release/server`.

## Running

```bash
DATABASE_PATH=/data/app.db PORT=3000 RUST_LOG=info ./server/target/release/server
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_PATH` | `./dev.db` | SQLite database path |
| `PORT` | `3000` | HTTP server port |
| `RUST_LOG` | `info` | Logging level (`server=debug` for verbose) |

## Database

SQLite with WAL mode is used. The database file is created automatically on first run.

### Backup

```bash
just db-dump backup.tar.gz
```

### Restore

```bash
tar -xzf backup.tar.gz
sqlite3 app.db < schema.sql  # requires extracting and running migrations
```

## Docker

A `Dockerfile` is provided for containerized deployment:

```bash
docker build -t topic-tree-with-qav .
docker run -p 3000:3000 -v /data:/data topic-tree-with-qav
```

Or with environment variables:

```bash
docker run -p 3000:3000 \
  -e DATABASE_PATH=/data/app.db \
  -e RUST_LOG=info \
  -v /path/to/data:/data \
  topic-tree-with-qav
```

## Reverse Proxy

For production, run behind a reverse proxy (nginx, Caddy, etc.) that handles TLS termination and WebSocket upgrades.

Example nginx location block:

```nginx
location / {
    proxy_pass http://127.0.0.1:3000;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
}
```

## Data Retention

Rooms are retained indefinitely. There is no automatic cleanup. To delete old rooms, manually connect to the SQLite database and remove rows from the `rooms` table.

## Security Notes

- The admin token is stored in browser IndexedDB. Clearing site data revokes host access.
- Per-IP rate limits are enforced but can be bypassed by clearing localStorage.
- No persistent guest identity — guests can rejoin with a new `guestId` by clearing localStorage.
