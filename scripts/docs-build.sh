#!/usr/bin/env bash
# Build the Vitepress docs site. Copies fresh screenshots from the e2e
# docs-screenshots run (if present) before building.
set -euo pipefail

export PATH="$HOME/.local/bin:$PATH"

SRC="e2e/.docs-snapshots"
DST="docs/public/screenshots"

if [[ -d "$SRC" ]]; then
  mkdir -p "$DST"
  rsync -a --delete "$SRC/" "$DST/"
  echo "[docs-build] copied screenshots from $SRC → $DST"
else
  echo "[docs-build] no $SRC — skipping screenshot sync"
fi

if [[ ! -f docs/package.json ]]; then
  echo "[docs-build] docs/package.json missing — docs site not scaffolded yet, exiting clean."
  exit 0
fi

pnpm -C docs build
