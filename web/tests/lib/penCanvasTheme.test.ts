import { describe, expect, it } from "vitest";
import { readPenCanvasBg } from "../../src/lib/penCanvasTheme";

describe("penCanvasTheme", () => {
  it("reads light and dark canvas backgrounds from CSS variables", () => {
    document.documentElement.style.setProperty("--pen-canvas-bg", "255 255 255");
    expect(readPenCanvasBg(false)).toBe("rgb(255, 255, 255)");

    document.documentElement.style.setProperty("--pen-canvas-bg", "24 24 27");
    expect(readPenCanvasBg(true)).toBe("rgb(24, 24, 27)");
  });
});
