import { expect, test } from "@playwright/test";

test.describe("optimistic updates", () => {
  test("host add topic appears immediately in UI", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    const treeSection = page.locator("section").filter({ has: page.locator("text=Topics") });
    await expect(treeSection).toBeVisible();

    page.on("dialog", (dialog) => dialog.accept("My Test Topic"));

    await treeSection.getByRole("button", { name: "Add topic" }).click();

    await expect(treeSection.getByText("My Test Topic")).toBeVisible();
  });

  test("host rename topic updates immediately", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    const treeSection = page.locator("section").filter({ has: page.locator("text=Topics") });

    page.on("dialog", (dialog) => dialog.accept("Original Title"));
    await treeSection.getByRole("button", { name: "Add topic" }).click();
    await expect(treeSection.getByText("Original Title")).toBeVisible();

    await treeSection.getByText("Original Title").hover();
    await treeSection.getByRole("button", { name: "Rename" }).click();

    const renameInput = treeSection.locator("input[type='text']");
    await renameInput.fill("Renamed Title");
    await renameInput.press("Enter");

    await expect(treeSection.getByText("Renamed Title")).toBeVisible();
    await expect(treeSection.getByText("Original Title")).not.toBeVisible();
  });

  test("host can add multiple topics and they appear in order", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    const treeSection = page.locator("section").filter({ has: page.locator("text=Topics") });

    let dialogHandler: (dialog: import("@playwright/test").Dialog) => void;
    dialogHandler = (dialog) => dialog.accept("First Topic");
    page.on("dialog", dialogHandler);
    await treeSection.getByRole("button", { name: "Add topic" }).click();
    await page.off("dialog", dialogHandler);

    dialogHandler = (dialog) => dialog.accept("Second Topic");
    page.on("dialog", dialogHandler);
    await treeSection.getByRole("button", { name: "Add topic" }).click();
    await page.off("dialog", dialogHandler);

    const topicList = treeSection.locator("ul");
    await expect(topicList.locator("li").first()).toContainText("First Topic");
    await expect(topicList.locator("li").nth(1)).toContainText("Second Topic");
  });
});
