import { expect, test } from "@playwright/test";
import { goToWhiteboardsTab } from "../utils/roomTabs";

test.describe("pen whiteboard", () => {
  test("host can create a pen board and draw a stroke", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await goToWhiteboardsTab(page);
    await page.getByRole("button", { name: "Create Board" }).click();
    await page.getByRole("button", { name: "Pen" }).click();
    await page.getByRole("button", { name: "Create", exact: true }).click();

    const boardPanel = page.getByTestId("room-panel-whiteboards");
    await expect(boardPanel.locator("canvas")).toBeVisible();

    const canvas = boardPanel.locator("canvas");
    const box = await canvas.boundingBox();
    expect(box).not.toBeNull();

    await canvas.hover({ position: { x: box!.width / 2, y: box!.height / 2 } });
    await page.mouse.down();
    await page.mouse.move(box!.width / 2 + 50, box!.height / 2 + 50);
    await page.mouse.up();

    await expect(boardPanel.locator("canvas")).toBeVisible();
  });

  test("guest can see the pen board created by host", async ({ browser }) => {
    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();

    await hostPage.goto("/");
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);

    const url = hostPage.url();
    const roomId = url.match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    await goToWhiteboardsTab(hostPage);
    await hostPage.getByRole("button", { name: "Create Board" }).click();
    await hostPage.getByRole("button", { name: "Pen" }).click();
    await hostPage.getByRole("button", { name: "Create", exact: true }).click();

    const guestContext = await browser.newContext();
    const guestPage = await guestContext.newPage();

    await guestPage.goto(`/r/${roomId}`);
    await guestPage.getByLabel("Your name").fill("Test Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    await goToWhiteboardsTab(guestPage);
    const boardPanel = guestPage.getByTestId("room-panel-whiteboards");
    await expect(boardPanel.locator("canvas")).toBeVisible({ timeout: 10_000 });

    await hostContext.close();
    await guestContext.close();
  });
});
