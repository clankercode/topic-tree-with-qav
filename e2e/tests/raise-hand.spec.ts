import { expect, test } from "@playwright/test";

test.describe("raise hand", () => {
  test("guest can raise hand with a topic", async ({ browser }) => {
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
    await guestPage.getByLabel("Your name").fill("Test Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    const raiseButton = guestPage.getByRole("button", { name: "Raise hand" });
    await expect(raiseButton).toBeVisible();

    await raiseButton.click();

    const dialog = guestPage.locator(".fixed.inset-0.z-50");
    await expect(dialog).toBeVisible();
    await dialog.locator('input[type="text"]').fill("Can you explain closures?");
    await dialog.getByRole("button", { name: "Raise hand" }).click();

    await expect(dialog).not.toBeVisible();
    await expect(guestPage.getByRole("button", { name: "Lower hand" })).toBeVisible();

    await hostContext.close();
    await guestContext.close();
  });

  test("guest can update their raised hand topic", async ({ browser }) => {
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
    await guestPage.getByLabel("Your name").fill("Test Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    await guestPage.getByRole("button", { name: "Raise hand" }).click();
    const dialog = guestPage.locator(".fixed.inset-0.z-50");
    await dialog.locator('input[type="text"]').fill("Original question");
    await dialog.getByRole("button", { name: "Raise hand" }).click();

    await guestPage.getByRole("button", { name: "Lower hand" }).click();
    await expect(dialog).toBeVisible();
    await dialog.locator('input[type="text"]').fill("Updated question");
    await dialog.getByRole("button", { name: "Update" }).click();

    await expect(guestPage.getByRole("button", { name: "Lower hand" })).toBeVisible();

    await hostContext.close();
    await guestContext.close();
  });

  test("guest can lower their hand", async ({ browser }) => {
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
    await guestPage.getByLabel("Your name").fill("Test Guest");
    await guestPage.getByRole("button", { name: "Join" }).click();
    await guestPage.waitForURL(`/r/${roomId}/guest`);

    await guestPage.getByRole("button", { name: "Raise hand" }).click();
    const dialog = guestPage.locator(".fixed.inset-0.z-50");
    await dialog.locator('input[type="text"]').fill("My question");
    await dialog.getByRole("button", { name: "Raise hand" }).click();

    await expect(guestPage.getByRole("button", { name: "Lower hand" })).toBeVisible();

    await guestPage.getByRole("button", { name: "Lower hand" }).click();
    await expect(dialog).not.toBeVisible({ timeout: 5000 });
    await expect(guestPage.getByRole("button", { name: "Raise hand" })).toBeVisible();

    await hostContext.close();
    await guestContext.close();
  });
});
