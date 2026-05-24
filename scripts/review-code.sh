#!/usr/bin/env bash
# Run Track B (code) reviews for a phase round: ccc + kimi.
# Track A (visual) is dispatched by the leader agent, not this script.
set -euo pipefail

PHASE="${1:-${PHASE:-unknown}}"
ROUND="${2:-${ROUND:-1}}"

OUT_DIR=".review/phase-${PHASE}/round-${ROUND}"
mkdir -p "$OUT_DIR"

echo "[review-code] phase=$PHASE round=$ROUND → $OUT_DIR"

resolve_base_ref() {
  if [[ -n "${BASE_REF:-}" ]] && git rev-parse --verify "$BASE_REF" >/dev/null 2>&1; then
    printf '%s\n' "$BASE_REF"
    return
  fi

  local origin_head
  origin_head="$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null || true)"
  for candidate in "$origin_head" origin/main origin/master main master; do
    [[ -n "$candidate" ]] || continue
    if git rev-parse --verify "$candidate" >/dev/null 2>&1; then
      printf '%s\n' "$candidate"
      return
    fi
  done

  echo "could not resolve a base ref; set BASE_REF=<ref>" >&2
  exit 2
}

BASE_REF="$(resolve_base_ref)"

# kimi runs in the foreground here (the justfile recipe for background use is
# `just kimi-review` invoked separately by the leader).
BASE_REF="$BASE_REF" bash scripts/kimi-review.sh "$PHASE" "$ROUND" || true

CCC_OUT="$OUT_DIR/ccc.md"
if command -v ccc >/dev/null 2>&1; then
  ccc --yolo --timeout-secs "${CCC_TIMEOUT_SECONDS:-300}" @cx-reviewer \
    "Review the current branch diff against $BASE_REF for correctness, deployment blockers, CI/build/test failures, security, accessibility regressions, dead code, API misuse, and test coverage gaps. Reference .plan/2026-05-24-amber-falcon/index.md, protocol.md, data-model.md, deployment.md, and testing.md. Output PASS if no actionable issues; otherwise list severity, file:line, issue, and suggested fix. Skip nits." \
    > "$CCC_OUT" 2>&1 || true
  echo "[review-code] wrote $CCC_OUT"
else
  {
    echo "# ccc-review skipped"
    echo
    echo "\`ccc\` CLI not on PATH. Dispatch the ccc-review-cx skill manually."
  } > "$CCC_OUT"
  echo "[review-code] ccc CLI missing; wrote stub to $CCC_OUT"
fi
