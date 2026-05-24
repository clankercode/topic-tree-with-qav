#!/usr/bin/env bash
# Run Moonshot Kimi over the current branch diff vs main, write the punch list
# to .review/phase-<phase>/round-<round>/kimi.md.
set -euo pipefail

PHASE="${1:-${PHASE:-unknown}}"
ROUND="${2:-${ROUND:-1}}"

OUT_DIR=".review/phase-${PHASE}/round-${ROUND}"
mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/kimi.md"

BASE_REF="${BASE_REF:-origin/main}"
if ! git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
  BASE_REF="main"
fi

DIFF="$(git diff --no-color "$BASE_REF"...HEAD || true)"
if [[ -z "$DIFF" ]]; then
  echo "[kimi-review] no diff against $BASE_REF — nothing to review." | tee "$OUT"
  exit 0
fi

PROMPT=$(cat <<'EOF'
Review the diff on the current branch against main for:
  - Correctness bugs (race conditions, off-by-one, lifetime issues, broken assumptions)
  - Error handling gaps at trust boundaries (user input, network)
  - Accessibility regressions (semantic HTML, focus, contrast, ARIA)
  - Dead code
  - API misuse of: axum, tokio, rusqlite, perfect-freehand, @excalidraw/excalidraw, zustand
  - Test coverage gaps for changed behavior

Reference docs:
  - .plan/2026-05-24-amber-falcon/index.md
  - .plan/2026-05-24-amber-falcon/protocol.md
  - .plan/2026-05-24-amber-falcon/data-model.md

Output as a punch list. Each item: severity, file:line, issue, suggested fix.
Skip nits unless they cluster.

DIFF:
EOF
)

if ! command -v kimi >/dev/null 2>&1; then
  {
    echo "# kimi-review skipped"
    echo
    echo "\`kimi\` CLI not on PATH. Install it or run review via your shell."
    echo "Phase=$PHASE round=$ROUND base=$BASE_REF"
  } > "$OUT"
  echo "[kimi-review] kimi CLI missing; wrote stub to $OUT"
  exit 0
fi

printf '%s\n\n```diff\n%s\n```\n' "$PROMPT" "$DIFF" \
  | kimi --print --yolo --thinking -p "$(cat)" > "$OUT" 2>&1 || true

echo "[kimi-review] wrote $OUT"
