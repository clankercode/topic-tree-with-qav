#!/usr/bin/env bash
# Boot the release server against a per-invocation tempfile SQLite DB on a free port.
# Used by Playwright's webServer (BASE_URL is derived from the chosen port).
#
# NOTE: we deliberately use a tempfile DB rather than :memory: because r2d2 pools
# independent connections, and an in-memory SQLite has a per-connection DB —
# writes from one connection would be invisible to another. See data-model.md.
set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

BIN="./server/target/release/server"
if [[ ! -x "$BIN" ]]; then
  echo "[serve-test] building release binary..." >&2
  cargo build --release --manifest-path server/Cargo.toml -j 2
fi

TMPDIR_DB="$(mktemp -d -t ttq-serve-test-XXXXXX)"
DB_PATH="$TMPDIR_DB/app.db"

# pick a free port unless PORT is explicitly set
if [[ -z "${PORT:-}" ]]; then
  PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
fi
export PORT

server_pid=""
cleanup() {
  trap - INT TERM EXIT
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill -TERM "$server_pid" 2>/dev/null || true
    sleep 1
    kill -KILL "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$TMPDIR_DB"
}
trap cleanup INT TERM EXIT

echo "[serve-test] DB=$DB_PATH PORT=$PORT"
echo "[serve-test] BASE_URL=http://127.0.0.1:$PORT"

RUST_LOG="${RUST_LOG:-debug}" \
DATABASE_PATH="$DB_PATH" \
  "$BIN" &
server_pid="$!"

wait "$server_pid"
