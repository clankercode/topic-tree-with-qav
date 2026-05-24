#!/usr/bin/env bash
# Run vite dev (web) + cargo run (server) concurrently, with cleanup.
set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

WEB_PORT="${WEB_PORT:-5173}"
SERVER_PORT="${SERVER_PORT:-3000}"

pids=()

cleanup() {
  trap - INT TERM EXIT
  for pid in "${pids[@]:-}"; do
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -TERM "$pid" 2>/dev/null || true
    fi
  done
  # give them a moment, then force
  sleep 1
  for pid in "${pids[@]:-}"; do
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null; then
      kill -KILL "$pid" 2>/dev/null || true
    fi
  done
  wait 2>/dev/null || true
}
trap cleanup INT TERM EXIT

echo "[dev] starting vite on :${WEB_PORT}"
pnpm -C web dev --host --port "$WEB_PORT" &
pids+=("$!")

echo "[dev] starting cargo run on :${SERVER_PORT}"
RUST_LOG="${RUST_LOG:-info,server=debug}" \
DATABASE_PATH="${DATABASE_PATH:-./dev.db}" \
PORT="$SERVER_PORT" \
  cargo run --manifest-path server/Cargo.toml -j 2 &
pids+=("$!")

# Exit as soon as any child exits — cleanup trap kills the rest.
wait -n
