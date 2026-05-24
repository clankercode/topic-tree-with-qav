#!/usr/bin/env bash
# CI entrypoint for the Playwright suite. Playwright's webServer
# boots `just serve-test` per playwright.config.ts.
set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"
export CI="${CI:-true}"

# Make sure the release binary exists so the webServer boot is cheap.
if [[ ! -x ./server/target/release/server ]]; then
  cargo build --release --manifest-path server/Cargo.toml -j 2
fi

# Ensure chromium present (no-op if installed).
pnpm -C e2e exec playwright install chromium >/dev/null 2>&1 || true

pnpm -C e2e test --reporter=line,html
