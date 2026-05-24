import { expect, test } from "@playwright/test";

const OUT_DIR = "screenshots/_docs";

test.describe("docs screenshots", () => {
  test("empty room state", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);
    await page.screenshot({ path: `${OUT_DIR}/empty-room.png` });
  });

  test("mid-session with topics", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await page.getByRole("button", { name: "Add Topic" }).click();
    await page.locator('input[placeholder*="topic" i], input[type="text"]').first().fill("Introduction");
    await page.keyboard.press("Enter");

    await page.getByRole("button", { name: "Add Topic" }).click();
    await page.locator('input[placeholder*="topic" i], input[type="text"]').first().fill("Deep Dive");
    await page.keyboard.press("Enter");

    await page.screenshot({ path: `${OUT_DIR}/mid-session-topics.png` });
  });

  test("Q&A active", async ({ browser }) => {
    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();

    await hostPage.goto("/");
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);

    const url = hostPage.url();
    const roomId = url.match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    const guestContext = await browser.newContext();
    const guestPage = await guestContext.newPage();

    await guestPage.goto(`/r/${roomId}`);
    await guestPage.getByLabel("Your name").fill("Alice");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    await guestPage.getByPlaceholder("Type your question").fill("What is the topic?");
    await guestPage.getByRole("button", { name: "Submit" }).click();
    await guestPage.waitForTimeout(500);

    await guestPage.getByPlaceholder("Type your question").fill("Can you elaborate?");
    await guestPage.getByRole("button", { name: "Submit" }).click();
    await guestPage.waitForTimeout(500);

    const voteBtn = guestPage.getByRole("button", { name: /vote/i }).first();
    await voteBtn.click();
    await guestPage.waitForTimeout(500);

    await hostPage.screenshot({ path: `${OUT_DIR}/qa-active.png` });

    await hostContext.close();
    await guestContext.close();
  });

  test("pen board with content", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await page.getByRole("button", { name: "Create Board" }).click();
    await page.getByRole("button", { name: "Pen" }).click();
    await page.getByRole("button", { name: "Create", exact: true }).click();

    const boardPanel = page.locator(".flex.flex-col.h-full").last();
    await expect(boardPanel.locator("canvas")).toBeVisible({ timeout: 10_000 });

    const canvas = boardPanel.locator("canvas");
    const box = await canvas.boundingBox();
    expect(box).not.toBeNull();

    await canvas.hover({ position: { x: box!.width / 2, y: box!.height / 2 } });
    await page.mouse.down();
    await page.mouse.move(box!.width / 2 + 80, box!.height / 2 + 30);
    await page.mouse.move(box!.width / 2 + 120, box!.height / 2 + 60);
    await page.mouse.up();
    await page.waitForTimeout(500);

    await page.screenshot({ path: `${OUT_DIR}/pen-board-content.png` });
  });

  test("excalidraw board", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await page.getByRole("button", { name: "Create Board" }).click();
    await page.getByRole("button", { name: "Excalidraw" }).click();
    await page.getByRole("button", { name: "Create", exact: true }).click();

    await page.waitForTimeout(3000);

    const excanvas = page.locator(".excalidraw-body").first();
    await expect(excanvas).toBeVisible({ timeout: 10_000 });

    await page.screenshot({ path: `${OUT_DIR}/excalidraw-board.png` });
  });

  test("raise hand queue", async ({ browser }) => {
    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();

    await hostPage.goto("/");
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);

    const url = hostPage.url();
    const roomId = url.match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    const guest1Context = await browser.newContext();
    const guest1Page = await guest1Context.newPage();
    await guest1Page.goto(`/r/${roomId}`);
    await guest1Page.getByLabel("Your name").fill("Alice");
    await guest1Page.getByRole("button", { name: "Join" }).click();
    await guest1Page.waitForURL(`/r/${roomId}/guest`);
    await guest1Page.getByRole("button", { name: "Raise hand" }).click();
    await guest1Page.locator(".fixed.inset-0.z-50 input[type='text']").fill("Question one");
    await guest1Page.getByRole("button", { name: "Raise hand" }).click();

    const guest2Context = await browser.newContext();
    const guest2Page = await guest2Context.newPage();
    await guest2Page.goto(`/r/${roomId}`);
    await guest2Page.getByLabel("Your name").fill("Bob");
    await guest2Page.getByRole("button", { name: "Join" }).click();
    await guest2Page.waitForURL(`/r/${roomId}/guest`);
    await guest2Page.getByRole("button", { name: "Raise hand" }).click();
    await guest2Page.locator(".fixed.inset-0.z-50 input[type='text']").fill("Question two");
    await guest2Page.getByRole("button", { name: "Raise hand" }).click();

    await hostPage.screenshot({ path: `${OUT_DIR}/raise-hand-queue.png` });

    await hostContext.close();
    await guest1Context.close();
    await guest2Context.close();
  });

  test("dark mode empty room", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => {
      localStorage.setItem("theme", "dark");
      document.documentElement.classList.add("dark");
    });
    await page.reload();
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);
    await page.screenshot({ path: `${OUT_DIR}/dark-empty-room.png` });
  });

  test("dark mode mid-session", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => {
      localStorage.setItem("theme", "dark");
      document.documentElement.classList.add("dark");
    });
    await page.reload();
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await page.getByRole("button", { name: "Add Topic" }).click();
    await page.locator('input[placeholder*="topic" i], input[type="text"]').first().fill("Dark Topic");
    await page.keyboard.press("Enter");

    await page.screenshot({ path: `${OUT_DIR}/dark-mid-session.png` });
  });

  test("dark mode Q&A", async ({ browser }) => {
    await page.goto("/");
    await page.evaluate(() => {
      localStorage.setItem("theme", "dark");
      document.documentElement.classList.add("dark");
    });
    await page.reload();

    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();

    await hostPage.goto("/");
    await hostPage.evaluate(() => {
      localStorage.setItem("theme", "dark");
      document.documentElement.classList.add("dark");
    });
    await hostPage.reload();
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);

    const url = hostPage.url();
    const roomId = url.match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    const guestContext = await browser.newContext();
    const guestPage = await guestContext.newPage();
    await guestPage.goto(`/r/${roomId}`);
    await guestPage.evaluate(() => {
      localStorage.setItem("theme", "dark");
      document.documentElement.classList.add("dark");
    });
    await guestPage.reload();
    await guestPage.getByLabel("Your name").fill("Dark Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    await guestPage.getByPlaceholder("Type your question").fill("Dark question");
    await guestPage.getByRole("button", { name: "Submit" }).click();
    await guestPage.waitForTimeout(500);

    await hostPage.screenshot({ path: `${OUT_DIR}/dark-qa-active.png` });

    await hostContext.close();
    await guestContext.close();
  });
});
