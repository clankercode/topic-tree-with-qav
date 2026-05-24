import { fileURLToPath } from "node:url";
import { dirname } from "node:path";
import { defineConfig, devices } from "@playwright/test";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Deterministic port for Playwright's webServer.
//
// scripts/serve-test.sh picks a random free port unless $PORT is set. We set
// PORT explicitly here so Playwright's `baseURL` and `webServer` agree without
// post-hoc port discovery (Playwright resolves webServer URLs before any test
// hook runs, so dynamic ports would require a custom launcher). The script
// still uses a per-invocation mktemp SQLite file — never `:memory:` — to keep
// r2d2's multi-connection pool consistent (see Phase 0.7).
const PORT = Number(process.env.PORT ?? 4173);
const BASE_URL = `http://127.0.0.1:${PORT}`;

export default defineConfig({
  testDir: "./tests",
  outputDir: "./test-results",
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: [
    ["list"],
    ["html", { outputFolder: "playwright-report", open: "never" }],
  ],
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "bash scripts/serve-test.sh",
    url: `${BASE_URL}/healthz`,
    cwd: `${__dirname}/..`,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    env: {
      PORT: String(PORT),
    },
    stdout: "pipe",
    stderr: "pipe",
  },
});
