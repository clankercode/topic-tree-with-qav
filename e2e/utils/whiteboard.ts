import type { Page } from "@playwright/test";
import { goToWhiteboardsTab } from "./roomTabs";

export async function enableE2eTestHooks(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem("ttq-e2e", "1");
  });
}

export async function readPenStrokePointCount(
  page: Page,
  boardId?: string,
): Promise<number> {
  return page.evaluate((bid) => {
    const w = window as Window & {
      __ttqSessionStore?: {
        getState: () => {
          penBoards: Map<
            string,
            { strokes: Array<{ points: unknown[] }> }
          >;
          penInProgressStrokes: Map<
            string,
            { points: unknown[] }
          >;
        };
      };
    };
    const store = w.__ttqSessionStore?.getState();
    if (!store) return 0;

    let total = 0;
    store.penInProgressStrokes.forEach((stroke, key) => {
      if (!bid || key.startsWith(`${bid}:`)) {
        total += stroke.points.length;
      }
    });
    for (const [id, board] of store.penBoards.entries()) {
      if (bid && id !== bid) continue;
      for (const stroke of board.strokes) {
        total += stroke.points.length;
      }
    }
    return total;
  }, boardId);
}

export async function createPenBoard(page: Page) {
  await goToWhiteboardsTab(page);
  await page.getByRole("button", { name: "Create Board" }).click();
  await page.getByRole("button", { name: "Pen" }).click();
  await page.getByRole("button", { name: "Create", exact: true }).click();
  return page.getByTestId("room-panel-whiteboards");
}

export async function drawPenStroke(
  page: Page,
  boardPanel: ReturnType<Page["locator"]>,
  opts: { steps?: number; distance?: number } = {},
) {
  const steps = opts.steps ?? 40;
  const distance = opts.distance ?? 120;
  const canvas = boardPanel.locator("canvas");
  const box = await canvas.boundingBox();
  if (!box) throw new Error("canvas missing bounding box");

  const startX = box.width / 2;
  const startY = box.height / 2;
  await canvas.hover({ position: { x: startX, y: startY } });
  await page.mouse.down();
  for (let i = 1; i <= steps; i += 1) {
    await page.mouse.move(
      startX + (distance * i) / steps,
      startY + (distance * i) / steps,
    );
  }
  await page.mouse.up();
}
