#!/usr/bin/env bash
# Idempotent first-time Railway setup: project, volume, env vars.
# Re-runnable; safe to invoke multiple times.
#
# Pre-reqs:
#   - `railway` CLI on PATH
#   - `railway login` already completed (or RAILWAY_TOKEN env set)
set -euo pipefail

PROJECT_NAME="${PROJECT_NAME:-topic-tree-with-qav}"
PROJECT_ID="${PROJECT_ID:-}"
TEAM_NAME="${TEAM_NAME:-clankercode}"
VOLUME_NAME="${VOLUME_NAME:-data}"
VOLUME_MOUNT="${VOLUME_MOUNT:-/data}"
export PROJECT_NAME PROJECT_ID TEAM_NAME VOLUME_NAME VOLUME_MOUNT

if ! command -v railway >/dev/null 2>&1; then
  echo "railway CLI not on PATH. Install: npm i -g @railway/cli" >&2
  exit 1
fi

# version check + help-probe (Railway CLI verbs have shifted across versions)
railway --version || true

echo "[railway-init] project=$PROJECT_NAME team=$TEAM_NAME volume=$VOLUME_NAME mount=$VOLUME_MOUNT"

current_project="$(
  railway status --json 2>/dev/null \
    | python3 -c 'import json,sys
data=json.load(sys.stdin)
print(data.get("name", ""))
' 2>/dev/null || true
)"
current_workspace="$(
  railway status --json 2>/dev/null \
    | python3 -c 'import json,sys
data=json.load(sys.stdin)
print(data.get("workspace", {}).get("name", ""))
' 2>/dev/null || true
)"

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
export workspace_id

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

if [[ "$current_project" == "$PROJECT_NAME" && "$current_workspace" == "$TEAM_NAME" ]]; then
  echo "[railway-init] already linked to $PROJECT_NAME on $TEAM_NAME — skipping init"
else
  if [[ -z "$PROJECT_ID" ]]; then
    mapfile -t matching_projects < <(
      railway list --json 2>/dev/null \
        | python3 -c 'import json,os,sys
target_name=os.environ["PROJECT_NAME"]
target_workspace=os.environ["workspace_id"]
for project in json.load(sys.stdin):
    workspace=project.get("workspace", {})
    if project.get("deletedAt") is None and project.get("name") == target_name and (
        workspace.get("id") == target_workspace or workspace.get("name") == os.environ["TEAM_NAME"]
    ):
        print("{}\t{}".format(project.get("id"), project.get("createdAt", "")))
' 2>/dev/null \
        | sort -k2
    )
    if [[ ${#matching_projects[@]} -eq 1 ]]; then
      PROJECT_ID="${matching_projects[0]%%$'\t'*}"
    elif [[ ${#matching_projects[@]} -gt 1 ]]; then
      echo "[railway-init] multiple '$PROJECT_NAME' projects exist in workspace '$TEAM_NAME'." >&2
      echo "[railway-init] Set PROJECT_ID to the intended project and rerun. Candidates:" >&2
      printf '  - %s\n' "${matching_projects[@]}" >&2
      exit 1
    fi
  fi

  if [[ -n "$PROJECT_ID" ]]; then
    echo "[railway-init] linking existing project $PROJECT_ID..."
    railway link --project "$PROJECT_ID" --workspace "$workspace_id" || {
      echo "[railway-init] failed to link project $PROJECT_ID" >&2
      exit 1
    }
  elif [[ ! -f .railway/project.json ]] && [[ ! -d .railway ]]; then
    echo "[railway-init] creating project..."
    railway init --name "$PROJECT_NAME" --workspace "$workspace_id" || {
      echo "[railway-init] railway init failed — run \`railway link\` manually or check CLI version." >&2
      echo "Docs: https://docs.railway.app/reference/cli-api" >&2
      exit 1
    }
  else
    echo "[railway-init] .railway/ present but status does not match $PROJECT_NAME on $TEAM_NAME" >&2
    exit 1
  fi
fi

service_id="$(
  railway status --json 2>/dev/null \
    | python3 -c 'import json,os,sys
target=os.environ["PROJECT_NAME"]
data=json.load(sys.stdin)
for edge in data.get("services", {}).get("edges", []):
    service=edge.get("node", {})
    if service.get("name") == target:
        print(service.get("id", ""))
        break
' 2>/dev/null || true
)"

if [[ -z "$service_id" ]]; then
  echo "[railway-init] creating service $PROJECT_NAME..."
  service_add_out="$(mktemp)"
  printf '\n' | railway add --service "$PROJECT_NAME" --json >"$service_add_out" || true
  service_id="$(
    railway status --json 2>/dev/null \
      | python3 -c 'import json,os,sys
target=os.environ["PROJECT_NAME"]
data=json.load(sys.stdin)
for edge in data.get("services", {}).get("edges", []):
    service=edge.get("node", {})
    if service.get("name") == target:
        print(service.get("id", ""))
        break
' 2>/dev/null || true
  )"
  if [[ -z "$service_id" ]]; then
    echo "[railway-init] failed to create required service $PROJECT_NAME" >&2
    cat "$service_add_out" >&2 || true
    rm -f "$service_add_out"
    exit 1
  fi
  rm -f "$service_add_out"
fi

if [[ -z "$service_id" ]]; then
  echo "[railway-init] could not resolve required service id for $PROJECT_NAME" >&2
  exit 1
fi
export SERVICE_ID="$service_id"

railway service link "$service_id" >/dev/null || {
  echo "[railway-init] failed to link service $PROJECT_NAME ($service_id)" >&2
  exit 1
}

volume_exists() {
  railway volume list --json 2>/dev/null \
    | python3 -c 'import json,os,sys
target_mount = os.environ["VOLUME_MOUNT"]
target_service = os.environ["PROJECT_NAME"]
target_service_id = os.environ["SERVICE_ID"]
data = json.load(sys.stdin)
volumes = data if isinstance(data, list) else data.get("volumes", [])
for volume in volumes:
    mount = volume.get("mountPath") or volume.get("mount_path") or volume.get("mount")
    service_name = volume.get("serviceName") or volume.get("service_name")
    service_id = volume.get("serviceId") or volume.get("service_id")
    service_matches = service_name == target_service or service_id == target_service_id
    if mount == target_mount and service_matches:
        sys.exit(0)
sys.exit(1)
' 2>/dev/null
}

# Volume — `railway volume add` is the current verb; fall back to a printed note.
if railway volume --help 2>&1 | grep -q 'add'; then
  if volume_exists; then
    echo "[railway-init] volume at $VOLUME_MOUNT already exists — continuing"
  else
    volume_add_err="$(mktemp)"
    if ! printf '%s\n' "$VOLUME_MOUNT" | railway volume add --mount-path "$VOLUME_MOUNT" 2>"$volume_add_err"; then
      if volume_exists; then
        echo "[railway-init] volume at $VOLUME_MOUNT already exists — continuing"
      else
        echo "[railway-init] failed to create required volume at $VOLUME_MOUNT:" >&2
        cat "$volume_add_err" >&2
        rm -f "$volume_add_err"
        exit 1
      fi
    elif ! volume_exists; then
      echo "[railway-init] volume add completed, but no $PROJECT_NAME service volume was found at $VOLUME_MOUNT" >&2
      cat "$volume_add_err" >&2
      rm -f "$volume_add_err"
      exit 1
    fi
    rm -f "$volume_add_err"
  fi
else
  echo "[railway-init] this railway CLI version does not expose \`volume add\`; create the volume in the Railway dashboard:"
  echo "  Name: $VOLUME_NAME  Mount: $VOLUME_MOUNT  Size: 1GB"
  exit 1
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
