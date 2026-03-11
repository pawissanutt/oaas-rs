/**
 * PresenterMosaic — N×M tiled mosaic view for presenter / cloud.
 *
 * Renders a grid of canvas tiles, auto-refreshes via polling,
 * and supports click-to-edit on individual tiles.
 */

import { CANVAS_SIZE } from "./types.js";
import type { PixelMap } from "./types.js";
import { fetchCanvas, invokeGolStep } from "./api.js";
import type { FetchResult } from "./api.js";
import { renderPixels } from "./render.js";
import { AudienceCanvas } from "./canvas.js";

export class PresenterMosaic {
  private readonly gatewayBase: string;
  private readonly cols: number;
  private readonly rows: number;
  private readonly cellSize: number;

  /** pixelMaps[x][y] = Map<"px:py", color> */
  private readonly pixelMaps: PixelMap[][];

  private pollInterval: ReturnType<typeof setInterval> | null = null;
  private activeEditor: AudienceCanvas | null = null;
  private canvases: { el: HTMLCanvasElement; x: number; y: number }[] = [];

  private statusEl!: HTMLSpanElement;
  private editorEl!: HTMLDivElement;

  /** GoL auto-run state */
  private golRunning = false;
  private golTimer: ReturnType<typeof setTimeout> | null = null;
  private golGeneration = 0;
  private golBusy = false;

  constructor(
    container: HTMLElement,
    gatewayBase: string,
    cols: number,
    rows: number
  ) {
    this.gatewayBase = gatewayBase;
    this.cols = cols;
    this.rows = rows;

    // Compute tile display size so mosaic fits in ~700px
    this.cellSize = Math.max(
      1,
      Math.floor(Math.min(700 / cols, 700 / rows) / CANVAS_SIZE)
    );

    this.pixelMaps = Array.from({ length: cols }, () =>
      Array.from({ length: rows }, () => new Map<string, string>())
    );

    this.buildUI(container);
    this.fetchAll();
    this.startPolling();
  }

  private buildUI(container: HTMLElement): void {
    const tilePx = this.cellSize * CANVAS_SIZE;

    container.innerHTML = `
      <div class="presenter-mosaic">
        <div class="presenter-toolbar">
          <span class="js-mosaic-status status-indicator">● loading...</span>
          <span class="mosaic-info">${this.cols}×${this.rows} | ${this.gatewayBase}</span>
        </div>
        <div class="gol-controls">
          <button class="btn-small js-gol-step" title="Run one Game of Life step">⏩ Step</button>
          <button class="btn-small js-gol-toggle" title="Start/stop auto-run">▶ Run</button>
          <label class="gol-speed-label">
            Speed
            <input type="range" class="js-gol-speed" min="100" max="3000" value="500" step="100">
            <span class="js-gol-speed-val">500ms</span>
          </label>
          <span class="js-gol-info gol-info"></span>
        </div>
        <div class="js-mosaic-grid mosaic-grid"
          style="grid-template-columns:repeat(${this.cols}, ${tilePx}px)">
        </div>
        <div class="js-tile-editor" style="display:none"></div>
      </div>`;

    this.statusEl = container.querySelector(".js-mosaic-status")!;
    const gridEl = container.querySelector(".js-mosaic-grid")!;
    this.editorEl = container.querySelector(".js-tile-editor")! as HTMLDivElement;

    // GoL controls wiring
    const stepBtn = container.querySelector(".js-gol-step") as HTMLButtonElement;
    const toggleBtn = container.querySelector(".js-gol-toggle") as HTMLButtonElement;
    const speedSlider = container.querySelector(".js-gol-speed") as HTMLInputElement;
    const speedVal = container.querySelector(".js-gol-speed-val")!;
    const golInfo = container.querySelector(".js-gol-info")!;

    stepBtn.addEventListener("click", () => this.runOneStep(golInfo));
    toggleBtn.addEventListener("click", () => {
      this.golRunning = !this.golRunning;
      toggleBtn.textContent = this.golRunning ? "⏸ Pause" : "▶ Run";
      if (this.golRunning) {
        this.scheduleAutoStep(golInfo, parseInt(speedSlider.value, 10));
      } else {
        this.stopAutoRun();
      }
    });
    speedSlider.addEventListener("input", () => {
      const ms = parseInt(speedSlider.value, 10);
      speedVal.textContent = `${ms}ms`;
    });

    this.canvases = [];
    for (let y = 0; y < this.rows; y++) {
      for (let x = 0; x < this.cols; x++) {
        const el = document.createElement("canvas");
        el.width = tilePx;
        el.height = tilePx;
        el.title = `canvas-${x}-${y}`;
        el.className = "mosaic-tile";
        el.addEventListener("click", () => this.openEditor(x, y));
        gridEl.appendChild(el);
        this.canvases.push({ el, x, y });
      }
    }
  }

  private canvasEl(x: number, y: number): HTMLCanvasElement | undefined {
    return this.canvases.find((c) => c.x === x && c.y === y)?.el;
  }

  private async fetchAll(): Promise<void> {
    const promises: Promise<{ x: number; y: number; result: FetchResult }>[] = [];
    let anyFailed = false;
    for (let x = 0; x < this.cols; x++) {
      for (let y = 0; y < this.rows; y++) {
        promises.push(
          fetchCanvas(this.gatewayBase, x, y).then((result) => ({ x, y, result }))
        );
      }
    }
    const results = await Promise.allSettled(promises);
    for (const r of results) {
      if (r.status !== "fulfilled") { anyFailed = true; continue; }
      const { x, y, result } = r.value;
      if (!result.ok) { anyFailed = true; continue; }
      this.pixelMaps[x][y] = result.pixels;
      const el = this.canvasEl(x, y);
      if (el) renderPixels(el, result.pixels, this.cellSize);
    }
    if (anyFailed) {
      this.statusEl.textContent = `● offline`;
      this.statusEl.style.color = "#ef4444";
    } else {
      this.statusEl.textContent = `● live — ${new Date().toLocaleTimeString()}`;
      this.statusEl.style.color = "#22c55e";
    }
  }

  private startPolling(): void {
    this.pollInterval = setInterval(() => this.fetchAll(), 1000);
  }

  private openEditor(x: number, y: number): void {
    if (this.activeEditor) {
      this.activeEditor.destroy();
      this.activeEditor = null;
    }
    this.editorEl.style.display = "block";
    this.editorEl.innerHTML = `
      <div class="tile-editor-panel">
        <div class="tile-editor-header">
          Editing canvas-${x}-${y}
          <button class="btn-small js-close-editor">✕ close</button>
        </div>
        <div class="js-editor-mount"></div>
      </div>`;
    this.editorEl
      .querySelector(".js-close-editor")!
      .addEventListener("click", () => {
        this.activeEditor?.destroy();
        this.activeEditor = null;
        this.editorEl.style.display = "none";
      });
    this.activeEditor = new AudienceCanvas(
      this.editorEl.querySelector(".js-editor-mount")!,
      this.gatewayBase,
      x,
      y
    );
  }

  /** Run a single GoL step and refresh the mosaic. */
  private async runOneStep(infoEl: Element): Promise<void> {
    if (this.golBusy) return;
    this.golBusy = true;
    infoEl.textContent = "computing...";
    const result = await invokeGolStep(this.gatewayBase, this.cols, this.rows);
    if (result) {
      this.golGeneration++;
      infoEl.textContent = `gen ${this.golGeneration} | +${result.births} −${result.deaths}`;
    } else {
      infoEl.textContent = "step failed";
    }
    await this.fetchAll();
    this.golBusy = false;
  }

  /** Schedule the next auto-run step after delay. */
  private scheduleAutoStep(infoEl: Element, delayMs: number): void {
    if (!this.golRunning) return;
    this.golTimer = setTimeout(async () => {
      await this.runOneStep(infoEl);
      if (this.golRunning) {
        // Re-read slider value each iteration so speed changes take effect
        const slider = document.querySelector(".js-gol-speed") as HTMLInputElement | null;
        const ms = slider ? parseInt(slider.value, 10) : delayMs;
        this.scheduleAutoStep(infoEl, ms);
      }
    }, delayMs);
  }

  /** Stop auto-run. */
  private stopAutoRun(): void {
    if (this.golTimer !== null) {
      clearTimeout(this.golTimer);
      this.golTimer = null;
    }
  }

  destroy(): void {
    if (this.pollInterval !== null) clearInterval(this.pollInterval);
    this.stopAutoRun();
    this.activeEditor?.destroy();
  }
}
