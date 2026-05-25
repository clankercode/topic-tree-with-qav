import { expect, test } from "@playwright/test";

async function addRootTopic(page: import("@playwright/test").Page, title: string) {
  const treeSection = page
    .locator("section")
    .filter({ has: page.locator("text=Topics") });
  await treeSection.getByRole("button", { name: "Add topic" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByPlaceholder("Topic title").fill(title);
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(treeSection.getByText(title)).toBeVisible();
}

test.describe("topic tree", () => {
  test("guest can vote on a topic", async ({ browser }) => {
    const hostContext = await browser.newContext();
    const hostPage = await hostContext.newPage();

    await hostPage.goto("/");
    await hostPage.getByRole("button", { name: "Create room" }).click();
    await hostPage.waitForURL(/\/r\/.*\/host/);

    const roomId = hostPage.url().match(/\/r\/([^/]+)\/host/)?.[1];
    expect(roomId).toBeDefined();

    await addRootTopic(hostPage, "Vote target");

    const guestContext = await browser.newContext();
    const guestPage = await guestContext.newPage();
    await guestPage.goto(`/r/${roomId}`);
    await guestPage.getByLabel("Your name").fill("Voter");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    const treeSection = guestPage
      .locator("section")
      .filter({ has: guestPage.locator("text=Topics") });
    await expect(treeSection.getByText("Vote target")).toBeVisible();

    const topicRow = treeSection.locator("li").filter({ hasText: "Vote target" });
    const voteBtn = topicRow.getByRole("button", { name: "Upvote" });
    await expect(voteBtn).toBeVisible({ timeout: 10000 });
    await voteBtn.click();
    await expect(
      topicRow.getByRole("button", { name: "Remove vote" }),
    ).toContainText("1", { timeout: 10000 });

    await hostContext.close();
    await guestContext.close();
  });

  test("host can add nested subtopics", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await addRootTopic(page, "Parent");

    const treeSection = page
      .locator("section")
      .filter({ has: page.locator("text=Topics") });
    const parentRow = treeSection.locator("li").filter({ hasText: "Parent" });
    await parentRow.getByRole("button", { name: "Add subtopic" }).click();
    await parentRow.getByPlaceholder("Subtopic title").fill("Child");
    await parentRow.getByRole("button", { name: "Add", exact: true }).click();
    await expect(treeSection.getByText("Child")).toBeVisible();
  });

  test("collapse hides subtopics locally", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Create room" }).click();
    await page.waitForURL(/\/r\/.*\/host/);

    await addRootTopic(page, "Collapse parent");

    const treeSection = page
      .locator("section")
      .filter({ has: page.locator("text=Topics") });
    const parentRow = treeSection.locator("li").filter({ hasText: "Collapse parent" });
    await parentRow.getByRole("button", { name: "Add subtopic" }).click();
    await parentRow.getByPlaceholder("Subtopic title").fill("Hidden child");
    await parentRow.getByRole("button", { name: "Add", exact: true }).click();
    await expect(treeSection.getByText("Hidden child")).toBeVisible();

    await parentRow.getByRole("button", { name: "Collapse subtopics" }).click();
    await expect(treeSection.getByText("Hidden child")).not.toBeVisible();
    await expect(parentRow.getByText("1 subtopic")).toBeVisible();
  });
});
