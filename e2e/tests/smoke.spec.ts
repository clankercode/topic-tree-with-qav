import { expect, test } from "@playwright/test";

test("homepage renders the H1", async ({ page }) => {
  await page.goto("/");
  const h1 = page.getByRole("heading", { level: 1 });
  await expect(h1).toHaveText("topic-tree-with-qav");
});
