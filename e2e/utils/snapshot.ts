// Visual-regression helpers.
//
// `awaitAppReady` blocks until `<div data-testid="app-ready">` has
// fired its `data-state="ready"` marker (one painted frame past
// initial mount). For room-bound screenshots, pass
// `requireConnection: true` so the helper additionally waits for
// `data-connection="connected"` — i.e. Welcome has been applied to
// the session store.
//
// `expectThemedScreenshot` asserts paired light + dark Playwright
// snapshots. Baselines live in Playwright's tracked `*-snapshots/`
// directories instead of the ignored `e2e/screenshots/` output folder.
// It switches themes by toggling the root `dark` class and the
// persisted `theme` localStorage key, awaits the next paint, hides
// noisy elements via `.snapshot-mode`, asserts, and restores.

import { expect, type Page } from "@playwright/test";

type ScreenshotClip = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type ThemedScreenshotOptions = {
  fullPage?: boolean;
  clip?: ScreenshotClip;
};

export async function awaitAppReady(
  page: Page,
  opts: { requireConnection?: boolean; timeout?: number } = {},
) {
  const timeout = opts.timeout ?? 10_000;
  await expect(page.getByTestId("app-ready")).toHaveAttribute(
    "data-state",
    "ready",
    { timeout },
  );
  if (opts.requireConnection) {
    await expect(page.getByTestId("app-ready")).toHaveAttribute(
      "data-connection",
      "connected",
      { timeout },
    );
  }
}

async function setTheme(page: Page, theme: "light" | "dark") {
  await page.evaluate((t) => {
    localStorage.setItem("ttq-e2e", "1");
    localStorage.setItem("theme", t);
    const w = window as Window & {
      __ttqThemeStore?: {
        getState: () => { setMode: (mode: "light" | "dark") => void };
      };
    };
    if (w.__ttqThemeStore) {
      w.__ttqThemeStore.getState().setMode(t);
    } else {
      const r = document.documentElement;
      if (t === "dark") r.classList.add("dark");
      else r.classList.remove("dark");
    }
  }, theme);
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        window.requestAnimationFrame(() => resolve()),
      ),
  );
}

export async function expectThemedScreenshot(
  page: Page,
  name: string,
  opts: ThemedScreenshotOptions = {},
) {
  await page.evaluate(() =>
    document.documentElement.classList.add("snapshot-mode"),
  );
  try {
    // Light first — most stylesheets default to it; restoring at the
    // end leaves the page in light mode for any subsequent assertions.
    for (const theme of ["dark", "light"] as const) {
      await setTheme(page, theme);
      await expect(page).toHaveScreenshot(`${name}-${theme}.png`, {
        fullPage: opts.fullPage ?? false,
        clip: opts.clip,
        animations: "disabled",
      });
    }
  } finally {
    await page.evaluate(() =>
      document.documentElement.classList.remove("snapshot-mode"),
    );
  }
}
