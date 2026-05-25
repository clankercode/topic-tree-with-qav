export function readPenCanvasBg(_isDark: boolean): string {
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue("--pen-canvas-bg")
    .trim();
  if (!raw) return _isDark ? "#18181b" : "#ffffff";
  const parts = raw.split(/\s+/).map(Number);
  if (parts.length !== 3 || parts.some(Number.isNaN)) {
    return _isDark ? "#18181b" : "#ffffff";
  }
  return `rgb(${parts.join(", ")})`;
}
