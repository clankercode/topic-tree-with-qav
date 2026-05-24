#!/usr/bin/env bash
# Run Track B (code) reviews for a phase round: ccc + kimi.
# Track A (visual) is dispatched by the leader agent, not this script.
set -euo pipefail

PHASE="${1:-${PHASE:-unknown}}"
ROUND="${2:-${ROUND:-1}}"

OUT_DIR=".review/phase-${PHASE}/round-${ROUND}"
mkdir -p "$OUT_DIR"

echo "[review-code] phase=$PHASE round=$ROUND → $OUT_DIR"

# kimi runs in the foreground here (the justfile recipe for background use is
# `just kimi-review` invoked separately by the leader).
bash scripts/kimi-review.sh "$PHASE" "$ROUND" || true

CCC_OUT="$OUT_DIR/ccc.md"
if command -v ccc >/dev/null 2>&1; then
  ccc review --base "${BASE_REF:-origin/main}" --head HEAD > "$CCC_OUT" 2>&1 || true
  echo "[review-code] wrote $CCC_OUT"
else
  {
    echo "# ccc-review skipped"
    echo
    echo "\`ccc\` CLI not on PATH. Dispatch the ccc-review-cx skill manually."
  } > "$CCC_OUT"
  echo "[review-code] ccc CLI missing; wrote stub to $CCC_OUT"
fi
