/**
 * Render a pixel map onto a canvas element.
 */
import type { PixelMap } from "./types.js";

export function renderPixels(
  canvasEl: HTMLCanvasElement,
  pixelMap: PixelMap,
  cellSize: number
): void {
  const ctx = canvasEl.getContext("2d")!;
  ctx.clearRect(0, 0, canvasEl.width, canvasEl.height);
  for (const [key, color] of pixelMap) {
    const [px, py] = key.split(":").map(Number);
    ctx.fillStyle = color;
    ctx.fillRect(px * cellSize, py * cellSize, cellSize, cellSize);
  }
}
