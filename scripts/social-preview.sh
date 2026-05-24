#!/usr/bin/env bash
# Renders a 1280x640 social-preview.png using Playwright.
# Uploads via GitHub API (requires GH_TOKEN env var) or documents manual fallback.
set -euo pipefail

OUT="${OUT:-social-preview.png}"
REPO="${REPO:-clankercode/topic-tree-with-qav}"
PORT="${PORT:-4173}"
BASE_URL="http://127.0.0.1:${PORT}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_DIR="$(cd "$SCRIPT_DIR/.." && pwd)/e2e"

# Start the server in the background.
echo "[social-preview] starting server..."
bash "$SCRIPT_DIR/serve-test.sh" &
SERVER_PID=$!
cleanup() {
  echo "[social-preview] stopping server (pid $SERVER_PID)..."
  kill $SERVER_PID 2>/dev/null || true
}
trap cleanup EXIT

# Wait for the server to be ready.
for i in $(seq 1 30); do
  if curl -sf "$BASE_URL/healthz" > /dev/null 2>&1; then
    echo "[social-preview] server ready after ${i}s"
    break
  fi
  sleep 1
done

# Run the Playwright screenshot script.
echo "[social-preview] rendering 1280x640 screenshot..."
node --input-type=module <<'EOF'
import { chromium } from "@playwright/test";

const BASE_URL = process.env.BASE_URL ?? "http://127.0.0.1:4173";
const OUT = process.env.OUT ?? "social-preview.png";

const browser = await chromium.launch();
const page = await browser.newPage();
await page.setViewportSize({ width: 1280, height: 640 });

// Navigate to the app root (landing page or room redirect).
await page.goto(BASE_URL, { waitUntil: "networkidle" });

// Give the page a moment to render.
await page.waitForTimeout(1000);

await page.screenshot({ path: OUT, fullPage: false });
await browser.close();

console.log("[social-preview] wrote " + OUT);
EOF

# Upload via GitHub API if GH_TOKEN is set.
if [[ -n "${GH_TOKEN:-}" ]]; then
  echo "[social-preview] uploading to GitHub..."
  RESPONSE=$(gh api -X POST "/repos/$REPO/releases" \
    -F tag_name="social-preview" \
    -F name="Social Preview" \
    -F draft=true \
    --input - <<'JSON'
{
  "body": "Social preview image for GitHub repo metadata. Manually download and set via repo settings."
}
JSON
  )
  UPLOAD_URL=$(echo "$RESPONSE" | node -e "process.stdin.resume();let d='';process.stdin.on('data',c=>d+=c);process.stdin.on('end',()=>console.log(JSON.parse(d).upload_url||''))" 2>/dev/null || true)
  if [[ -n "$UPLOAD_URL" ]]; then
    UPLOAD_URL="${UPLOAD_URL%\{*}"
    gh api -X POST "$UPLOAD_URL?name=social-preview.png" \
      -H "Content-Type: image/png" \
      --input "$OUT"
    echo "[social-preview] uploaded. Release draft created at https://github.com/$REPO/releases"
  else
    echo "[social-preview] release draft created but upload URL not found. Manual upload required."
  fi
else
  echo "[social-preview] GH_TOKEN not set — skipping upload."
  echo "[social-preview] Manual fallback: upload social-preview.png via https://github.com/$REPO/settings/pages"
fi

echo "[social-preview] done"