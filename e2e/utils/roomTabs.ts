import type { Page } from "@playwright/test";

export async function goToWhiteboardsTab(page: Page) {
  await page.getByTestId("room-tab-whiteboards").click();
}
