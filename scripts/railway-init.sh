#!/usr/bin/env bash
# Idempotent first-time Railway setup: project, volume, env vars.
# Re-runnable; safe to invoke multiple times.
#
# Pre-reqs:
#   - `railway` CLI on PATH
#   - `railway login` already completed (or RAILWAY_TOKEN env set)
set -euo pipefail

PROJECT_NAME="${PROJECT_NAME:-topic-tree-with-qav}"
TEAM_NAME="${TEAM_NAME:-clankercode}"
VOLUME_NAME="${VOLUME_NAME:-data}"
VOLUME_MOUNT="${VOLUME_MOUNT:-/data}"

if ! command -v railway >/dev/null 2>&1; then
  echo "railway CLI not on PATH. Install: npm i -g @railway/cli" >&2
  exit 1
fi

# version check + help-probe (Railway CLI verbs have shifted across versions)
railway --version || true

echo "[railway-init] project=$PROJECT_NAME team=$TEAM_NAME volume=$VOLUME_NAME mount=$VOLUME_MOUNT"

workspace_id="$(
  railway whoami --json 2>/dev/null \
    | python3 -c 'import json,sys,os
target=os.environ["TEAM_NAME"]
data=json.load(sys.stdin)
for workspace in data.get("workspaces", []):
    if workspace.get("name") == target or workspace.get("id") == target:
        print(workspace["id"])
        break
' 2>/dev/null || true
)"

if [[ -z "$workspace_id" ]]; then
  echo "[railway-init] Railway workspace '$TEAM_NAME' is not available to this login." >&2
  echo "[railway-init] Create or join that workspace, then rerun this script." >&2
  echo "[railway-init] Available workspaces:" >&2
  railway whoami --json \
    | python3 -c 'import json,sys
for workspace in json.load(sys.stdin).get("workspaces", []):
    print("  - {} ({})".format(workspace.get("name"), workspace.get("id")))
' >&2
  exit 1
fi

if [[ ! -f .railway/project.json ]] && [[ ! -d .railway ]]; then
  echo "[railway-init] linking/creating project..."
  railway init --name "$PROJECT_NAME" --workspace "$workspace_id" || {
    echo "[railway-init] railway init failed — run \`railway link\` manually or check CLI version." >&2
    echo "Docs: https://docs.railway.app/reference/cli-api" >&2
    exit 1
  }
else
  echo "[railway-init] .railway/ already present — skipping init"
fi

# Volume — `railway volume add` is the current verb; fall back to a printed note.
if railway volume --help 2>&1 | grep -q 'add'; then
  railway volume add --name "$VOLUME_NAME" --mount-path "$VOLUME_MOUNT" 2>/dev/null \
    || echo "[railway-init] volume may already exist — continuing"
else
  echo "[railway-init] this railway CLI version does not expose \`volume add\`; create the volume in the Railway dashboard:"
  echo "  Name: $VOLUME_NAME  Mount: $VOLUME_MOUNT  Size: 1GB"
fi

# Env vars — `railway variables --set K=V` is idempotent.
railway variables \
  --set "DATABASE_PATH=$VOLUME_MOUNT/app.db" \
  --set "RUST_LOG=info,server=debug" \
  || {
    echo "[railway-init] failed to set required variables — check CLI auth and project link" >&2
    exit 1
  }

echo "[railway-init] done. Next: \`just railway-deploy\`"
