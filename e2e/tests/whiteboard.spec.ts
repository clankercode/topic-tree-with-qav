import { expect, test } from "@playwright/test";
import {
  createPenBoard,
  drawPenStroke,
  enableE2eTestHooks,
  readPenStrokePointCount,
} from "../utils/whiteboard";
import { awaitAppReady, expectThemedScreenshot } from "../utils/snapshot";

test.describe("pen whiteboard", () => {
  test.beforeEach(async ({ page }) => {
    await enableE2eTestHooks(page);
  });

  test("host can create a pen board and draw a stroke", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);
    await awaitAppReady(page, { requireConnection: true });

    const boardPanel = await createPenBoard(page);
    await expect(boardPanel.locator("canvas")).toBeVisible();
    await drawPenStroke(page, boardPanel);
    await expect(boardPanel.locator("canvas")).toBeVisible();
  });

  test("guest receives dense pen stroke samples from fast host drag", async ({
    browser,
  }) => {
    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();
    await enableE2eTestHooks(hostPage);

    await hostPage.goto("/");
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);
    await awaitAppReady(hostPage, { requireConnection: true });

    const url = hostPage.url();
    const roomId = url.match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    const boardPanel = await createPenBoard(hostPage);

    const guestContext = await browser.newContext();
    const guestPage = await guestContext.newPage();
    await enableE2eTestHooks(guestPage);

    await guestPage.goto(`/r/${roomId}`);
    await guestPage.getByLabel("Your name").fill("Test Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);
    await awaitAppReady(guestPage, { requireConnection: true });

    await drawPenStroke(hostPage, boardPanel, { steps: 50, distance: 160 });

    await expect
      .poll(async () => readPenStrokePointCount(guestPage), {
        timeout: 5000,
      })
      .toBeGreaterThanOrEqual(30);
  });

  test("host does not double-play pen stroke points from server echo", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);
    await awaitAppReady(page, { requireConnection: true });

    const boardPanel = await createPenBoard(page);
    await drawPenStroke(page, boardPanel, { steps: 25, distance: 100 });

    await expect
      .poll(async () => readPenStrokePointCount(page), { timeout: 5000 })
      .toBeGreaterThanOrEqual(20);

    const hostPoints = await readPenStrokePointCount(page);
    await page.waitForTimeout(300);
    const hostPointsAfterEcho = await readPenStrokePointCount(page);
    expect(hostPointsAfterEcho).toBe(hostPoints);
  });

  test("guest can see the pen board created by host", async ({ browser }) => {
    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();
    await enableE2eTestHooks(hostPage);

    await hostPage.goto("/");
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);

    const url = hostPage.url();
    const roomId = url.match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    await createPenBoard(hostPage);

    const guestContext = await browser.newContext();
    const guestPage = await guestContext.newPage();
    await enableE2eTestHooks(guestPage);

    await guestPage.goto(`/r/${roomId}`);
    await guestPage.getByLabel("Your name").fill("Test Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    const boardPanel = guestPage.locator(".flex.flex-col.h-full").last();
    await expect(boardPanel.locator("canvas")).toBeVisible({ timeout: 10_000 });

    await hostContext.close();
    await guestContext.close();
  });
});

test.describe("whiteboard theme baselines @baseline", () => {
  test.beforeEach(async ({ page }) => {
    await enableE2eTestHooks(page);
  });

  test("pen board light and dark snapshots", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);
    await awaitAppReady(page, { requireConnection: true });

    const boardPanel = await createPenBoard(page);
    await drawPenStroke(page, boardPanel, { steps: 20, distance: 80 });

    await expectThemedScreenshot(page, "pen-board-host", {
      clip: await boardPanel.boundingBox() ?? undefined,
    });
  });

  test("excalidraw board light and dark snapshots", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);
    await awaitAppReady(page, { requireConnection: true });

    await page.getByRole("button", { name: "Create Board" }).click();
    await page.getByRole("button", { name: "Excalidraw" }).click();
    await page.getByRole("button", { name: "Create", exact: true }).click();

    const boardPanel = page.locator(".flex.flex-col.h-full").last();
    await expect(boardPanel).toBeVisible();

    await expectThemedScreenshot(page, "excalidraw-board-host", {
      clip: await boardPanel.boundingBox() ?? undefined,
    });
  });
});
