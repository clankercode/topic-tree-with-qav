import { expect, test } from "@playwright/test";

async function addTopic(page: import("@playwright/test").Page, title: string) {
  const treeSection = page
    .locator("section")
    .filter({ has: page.locator("text=Topics") });
  await treeSection.getByRole("button", { name: "Add topic" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByPlaceholder("Topic title").fill(title);
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
  await expect(dialog).toBeHidden();
}

test.describe("optimistic updates", () => {
  test("host add topic appears immediately in UI", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    const treeSection = page
      .locator("section")
      .filter({ has: page.locator("text=Topics") });
    await expect(treeSection).toBeVisible();

    await addTopic(page, "My Test Topic");
    await expect(treeSection.getByText("My Test Topic")).toBeVisible();
  });

  test("host rename topic updates immediately", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    const treeSection = page
      .locator("section")
      .filter({ has: page.locator("text=Topics") });

    await addTopic(page, "Original Title");
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

    const treeSection = page
      .locator("section")
      .filter({ has: page.locator("text=Topics") });

    await addTopic(page, "First Topic");
    await addTopic(page, "Second Topic");

    const topicList = treeSection.locator("ul");
    await expect(topicList.locator("li").first()).toContainText("First Topic");
    await expect(topicList.locator("li").nth(1)).toContainText("Second Topic");
  });
});
