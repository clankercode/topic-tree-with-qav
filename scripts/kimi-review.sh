#!/usr/bin/env bash
# Run Moonshot Kimi over the current branch diff, write the punch list
# to .review/phase-<phase>/round-<round>/kimi.md.
set -euo pipefail

PHASE="${1:-${PHASE:-unknown}}"
ROUND="${2:-${ROUND:-1}}"

OUT_DIR=".review/phase-${PHASE}/round-${ROUND}"
mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/kimi.md"

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
BASE_COMMIT="$(git merge-base "$BASE_REF" HEAD)"
DIFF_FILE="$OUT_DIR/diff.patch"
DIFF_PATHS=(. ':(exclude)pnpm-lock.yaml' ':(exclude)server/Cargo.lock' ':(exclude)web/src/proto/generated.ts')

{
  git diff --no-color "$BASE_COMMIT" HEAD -- "${DIFF_PATHS[@]}"
  git diff --cached --no-color -- "${DIFF_PATHS[@]}"
  git diff --no-color -- "${DIFF_PATHS[@]}"
  while IFS= read -r file; do
    [[ -f "$file" ]] || continue
    case "$file" in
      pnpm-lock.yaml|server/Cargo.lock|web/src/proto/generated.ts) continue ;;
    esac
    git diff --no-color --no-index /dev/null "$file" || true
  done < <(git ls-files --others --exclude-standard)
} > "$DIFF_FILE"

if [[ ! -s "$DIFF_FILE" ]]; then
  echo "[kimi-review] no diff against $BASE_REF — nothing to review." | tee "$OUT"
  exit 0
fi

PROMPT=$(cat <<EOF
Review the diff on the current branch against $BASE_REF for:
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

The full diff, including committed changes plus staged/unstaged/untracked working-tree changes, is saved at:
$DIFF_FILE

Large lock/generated files are intentionally omitted from that patch for reviewer throughput:
pnpm-lock.yaml, server/Cargo.lock, web/src/proto/generated.ts.
Inspect those files directly only if package/proto consistency is relevant to an issue.

Read that file and inspect referenced source files as needed. Output PASS if there are no actionable issues.
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

run_with_timeout() {
  local seconds="$1"
  shift
  if command -v timeout >/dev/null 2>&1; then
    timeout "$seconds" "$@"
  else
    "$@"
  fi
}

run_with_timeout "${KIMI_TIMEOUT_SECONDS:-600}" \
  kimi --print --final-message-only --yolo --thinking -p "$PROMPT" > "$OUT" 2>&1 || true

if [[ ! -s "$OUT" ]]; then
  run_with_timeout "${KIMI_FALLBACK_TIMEOUT_SECONDS:-180}" \
    kimi --print --final-message-only --yolo --no-thinking -p "$PROMPT" > "$OUT" 2>&1 || true
fi

if [[ ! -s "$OUT" ]]; then
  {
    echo "# kimi-review inconclusive"
    echo
    echo "Kimi produced no final message before timeout or exit."
    echo "Phase=$PHASE round=$ROUND base=$BASE_REF diff=$DIFF_FILE"
  } > "$OUT"
fi

echo "[kimi-review] wrote $OUT"
