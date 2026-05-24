#!/usr/bin/env bash
# Idempotently apply description, homepage, topics, and Pages source to the
# clankercode/topic-tree-with-qav GitHub repo.
#
# Flags:
#   --dry-run            print intended actions, change nothing
#   --with-protection    additionally apply branch protection to the default branch
set -euo pipefail

REPO="${REPO:-clankercode/topic-tree-with-qav}"
DESCRIPTION="${DESCRIPTION:-Real-time host-audience interaction: topic tree, Q&A with voting, collaborative whiteboards, live cursors.}"
HOMEPAGE="${HOMEPAGE:-https://clankercode.github.io/topic-tree-with-qav/}"
TOPICS=(real-time websockets rust axum react typescript whiteboard q-and-a self-hosted)
DEFAULT_BRANCH="${DEFAULT_BRANCH:-}"

DRY_RUN=0
WITH_PROTECTION=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --with-protection) WITH_PROTECTION=1 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '[dry-run] %s\n' "$*"
  else
    "$@"
  fi
}

if ! command -v gh >/dev/null 2>&1; then
  echo "gh CLI not on PATH" >&2
  exit 1
fi

if [[ -z "$DEFAULT_BRANCH" ]]; then
  DEFAULT_BRANCH="$(git remote show origin 2>/dev/null | sed -n '/HEAD branch/s/.*: //p' || true)"
fi
DEFAULT_BRANCH="${DEFAULT_BRANCH:-main}"

echo "[gh-repo-meta] repo=$REPO dry_run=$DRY_RUN with_protection=$WITH_PROTECTION"

run gh repo edit "$REPO" \
  --description "$DESCRIPTION" \
  --homepage "$HOMEPAGE"

# topics: gh repo edit --add-topic is idempotent (no-op if already present).
for t in "${TOPICS[@]}"; do
  run gh repo edit "$REPO" --add-topic "$t"
done

# Pages source = GitHub Actions. PUT is idempotent.
run gh api -X PUT "/repos/$REPO/pages" \
  -f 'build_type=workflow' || true

if [[ "$WITH_PROTECTION" -eq 1 ]]; then
  echo "[gh-repo-meta] applying branch protection to $DEFAULT_BRANCH"
  run gh api -X PUT "/repos/$REPO/branches/$DEFAULT_BRANCH/protection" \
    --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["lint", "test-web", "test-server", "proto-drift", "snapshot-pairs", "test-e2e"]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": { "required_approving_review_count": 0 },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
fi

echo "[gh-repo-meta] done"
