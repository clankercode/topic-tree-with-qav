#!/usr/bin/env bash
# Assert every Playwright snapshot has a matching light+dark pair.
# Pattern: e2e/screenshots/<spec>/<step>-{light,dark}.png  (see testing.md §3)
set -euo pipefail

ROOT="${1:-e2e/screenshots}"

if [[ ! -d "$ROOT" ]]; then
  echo "[check-snapshot-pairs] no $ROOT directory — nothing to check"
  exit 0
fi

missing=0

# every -light.png needs a -dark.png and vice versa
while IFS= read -r -d '' f; do
  pair="${f%-light.png}-dark.png"
  if [[ ! -f "$pair" ]]; then
    echo "missing dark pair for: $f"
    missing=$((missing + 1))
  fi
done < <(find "$ROOT" -type f -name '*-light.png' -print0)

while IFS= read -r -d '' f; do
  pair="${f%-dark.png}-light.png"
  if [[ ! -f "$pair" ]]; then
    echo "missing light pair for: $f"
    missing=$((missing + 1))
  fi
done < <(find "$ROOT" -type f -name '*-dark.png' -print0)

# Flag stray PNGs that don't follow the -light/-dark convention.
while IFS= read -r -d '' f; do
  case "$f" in
    *-light.png|*-dark.png) ;;
    *)
      echo "snapshot does not follow <name>-{light,dark}.png convention: $f"
      missing=$((missing + 1))
      ;;
  esac
done < <(find "$ROOT" -type f -name '*.png' -print0)

if [[ "$missing" -gt 0 ]]; then
  echo "[check-snapshot-pairs] FAIL: $missing issue(s)"
  exit 1
fi

echo "[check-snapshot-pairs] OK"
