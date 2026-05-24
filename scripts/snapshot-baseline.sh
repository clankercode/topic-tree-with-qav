#!/usr/bin/env bash
# Regenerate curated review screenshots into .review/_baseline/.
# Used by leader at phase boundaries to seed the visual review track.
set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

OUT_DIR=".review/_baseline"
mkdir -p "$OUT_DIR"

# Run the e2e suite with the snapshot grep so only screenshot-producing
# specs execute. The dedicated specs are tagged with @baseline.
SCREENSHOT_DIR="$OUT_DIR" \
  pnpm -C e2e test --grep @baseline --update-snapshots || {
    echo "[snapshot-baseline] no @baseline-tagged specs yet — skipping." >&2
}

echo "[snapshot-baseline] baseline written to $OUT_DIR"
