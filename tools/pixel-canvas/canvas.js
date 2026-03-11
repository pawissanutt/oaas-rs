// canvas.js — Pixel Canvas OaaS Demo
// ES module, no build step required.
//
// Two exported classes:
//   AudienceCanvas  — 32×32 drawable canvas for audience / edge
//   PresenterMosaic — N×M tiled mosaic for presenter / cloud

const CANVAS_SIZE = 32;      // pixels per dimension
const CELL_PX    = 10;       // display pixels per canvas pixel (audience)
const CLASS_NAME = "pixel-canvas";
const PARTITION  = 0;

// ---------------------------------------------------------------------------
// Gateway API helpers
// ---------------------------------------------------------------------------

/** Encode a CSS color string to base64 (utf-8 aware). */
function encodeColor(color) {
  return btoa(unescape(encodeURIComponent(color)));
}

/** Decode base64 entry data back to a CSS color string. */
function decodeColor(b64) {
  try {
    return decodeURIComponent(escape(atob(b64)));
  } catch {
    return "#ffffff";
  }
}

function objectUrl(gatewayBase, gridX, gridY) {
  return `${gatewayBase}/api/class/${CLASS_NAME}/${PARTITION}/objects/canvas-${gridX}-${gridY}`;
}

/**
 * Fetch a canvas object.
 * Returns a Map<"px:py", "#rrggbb"> — empty map if missing.
 */
export async function fetchCanvas(gatewayBase, gridX, gridY) {
  const url = objectUrl(gatewayBase, gridX, gridY);
  let res;
  try {
    res = await fetch(url, { headers: { Accept: "application/json" } });
  } catch (e) {
    console.warn("fetchCanvas network error:", e);
    return new Map();
  }
  if (res.status === 404) return new Map();
  if (!res.ok) { console.warn("fetchCanvas error", res.status); return new Map(); }

  const obj = await res.json();
  const entries = obj.entries ?? {};
  const pixels = new Map();
  for (const [key, val] of Object.entries(entries)) {
    pixels.set(key, decodeColor(val.data));
  }
  return pixels;
}

/**
 * Save a full pixel map to the gateway via PUT.
 * pixelMap: Map<"px:py", "#rrggbb">
 */
export async function saveCanvas(gatewayBase, gridX, gridY, pixelMap) {
  const entries = {};
  for (const [key, color] of pixelMap) {
    entries[key] = { data: encodeColor(color), type: "BYTE" };
  }
  const body = {
    metadata: { cls_id: CLASS_NAME, partition_id: PARTITION, object_id: `canvas-${gridX}-${gridY}` },
    entries,
  };
  const url = objectUrl(gatewayBase, gridX, gridY);
  try {
    const res = await fetch(url, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) console.warn("saveCanvas PUT failed:", res.status);
  } catch (e) {
    console.warn("saveCanvas network error:", e);
  }
}

// ---------------------------------------------------------------------------
// Shared canvas rendering
// ---------------------------------------------------------------------------

/** Render a pixel map onto a canvas element. */
export function renderPixels(canvasEl, pixelMap, cellSize) {
  const ctx = canvasEl.getContext("2d");
  ctx.clearRect(0, 0, canvasEl.width, canvasEl.height);
  for (const [key, color] of pixelMap) {
    const [px, py] = key.split(":").map(Number);
    ctx.fillStyle = color;
    ctx.fillRect(px * cellSize, py * cellSize, cellSize, cellSize);
  }
}

// ---------------------------------------------------------------------------
// AudienceCanvas
// ---------------------------------------------------------------------------

/**
 * Manages a 32×32 drawable canvas for an audience member.
 *
 * @param {HTMLElement} container  - Element to mount into.
 * @param {string}      gatewayBase - e.g. "http://edge-gateway:8080"
 * @param {number}      gridX       - Mosaic column index.
 * @param {number}      gridY       - Mosaic row index.
 */
export class AudienceCanvas {
  constructor(container, gatewayBase, gridX, gridY) {
    this.gatewayBase = gatewayBase;
    this.gridX = gridX;
    this.gridY = gridY;
    this.cellSize = CELL_PX;

    // Local pixel state
    this.pixels = new Map();
    this.dirty = new Set();
    this.isDrawing = false;
    this.currentColor = "#000000";
    this._flushTimer = null;
    this._pollInterval = null;

    this._buildUI(container);
    this._attachEvents();
    this._startPolling();
    this._fetchAndRender();   // initial load
  }

  _buildUI(container) {
    const size = CANVAS_SIZE * this.cellSize;

    container.innerHTML = `
      <div style="display:flex;flex-direction:column;align-items:center;gap:8px;font-family:sans-serif">
        <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap;justify-content:center">
          <label style="display:flex;align-items:center;gap:4px">
            Color <input type="color" id="color-picker" value="#000000" style="width:36px;height:28px;cursor:pointer">
          </label>
          <button id="clear-btn" style="padding:4px 10px;cursor:pointer">Clear</button>
          <span id="status" style="font-size:13px;color:#666">●</span>
        </div>
        <canvas id="draw-canvas"
          width="${size}" height="${size}"
          style="border:1px solid #ccc;cursor:crosshair;touch-action:none;image-rendering:pixelated">
        </canvas>
        <span style="font-size:12px;color:#999">canvas-${this.gridX}-${this.gridY}</span>
      </div>`;

    this.canvasEl   = container.querySelector("#draw-canvas");
    this.colorPicker = container.querySelector("#color-picker");
    this.statusEl   = container.querySelector("#status");
    this.clearBtn   = container.querySelector("#clear-btn");
  }

  _attachEvents() {
    const canvas = this.canvasEl;

    this.colorPicker.addEventListener("input", (e) => {
      this.currentColor = e.target.value;
    });

    this.clearBtn.addEventListener("click", () => {
      this.pixels.clear();
      for (let x = 0; x < CANVAS_SIZE; x++)
        for (let y = 0; y < CANVAS_SIZE; y++)
          this.pixels.set(`${x}:${y}`, "#ffffff");
      this.dirty = new Set(this.pixels.keys());
      this._render();
      this._scheduleSave();
    });

    const paint = (e) => {
      if (!this.isDrawing) return;
      const rect  = canvas.getBoundingClientRect();
      const scaleX = CANVAS_SIZE * this.cellSize / rect.width;
      const scaleY = CANVAS_SIZE * this.cellSize / rect.height;
      const cx = e.clientX ?? e.touches?.[0]?.clientX;
      const cy = e.clientY ?? e.touches?.[0]?.clientY;
      if (cx === undefined) return;
      const px = Math.floor(((cx - rect.left) * scaleX) / this.cellSize);
      const py = Math.floor(((cy - rect.top)  * scaleY) / this.cellSize);
      if (px < 0 || px >= CANVAS_SIZE || py < 0 || py >= CANVAS_SIZE) return;
      const key = `${px}:${py}`;
      if (this.pixels.get(key) === this.currentColor) return;
      this.pixels.set(key, this.currentColor);
      this.dirty.add(key);
      this._render();
      this._scheduleSave();
    };

    canvas.addEventListener("pointerdown",  (e) => { this.isDrawing = true; canvas.setPointerCapture(e.pointerId); paint(e); });
    canvas.addEventListener("pointermove",  paint);
    canvas.addEventListener("pointerup",    () => { this.isDrawing = false; });
    canvas.addEventListener("pointercancel",() => { this.isDrawing = false; });
  }

  _render() {
    renderPixels(this.canvasEl, this.pixels, this.cellSize);
  }

  _setStatus(ok, text) {
    this.statusEl.textContent = `● ${text}`;
    this.statusEl.style.color = ok ? "#22c55e" : "#ef4444";
  }

  _scheduleSave() {
    clearTimeout(this._flushTimer);
    this._flushTimer = setTimeout(() => this._flush(), 300);
  }

  async _flush() {
    if (this.dirty.size === 0) return;
    this.dirty.clear();
    await saveCanvas(this.gatewayBase, this.gridX, this.gridY, this.pixels);
    this._setStatus(true, "saved");
  }

  async _fetchAndRender() {
    const remote = await fetchCanvas(this.gatewayBase, this.gridX, this.gridY);
    // Merge remote: remote wins only for pixels not in the dirty set
    for (const [key, color] of remote) {
      if (!this.dirty.has(key)) {
        this.pixels.set(key, color);
      }
    }
    this._render();
    this._setStatus(true, "synced");
  }

  _startPolling() {
    this._pollInterval = setInterval(() => this._fetchAndRender(), 2000);
  }

  destroy() {
    clearTimeout(this._flushTimer);
    clearInterval(this._pollInterval);
  }
}

// ---------------------------------------------------------------------------
// PresenterMosaic
// ---------------------------------------------------------------------------

/**
 * Renders a cols×rows mosaic of canvas tiles (read-only, auto-refreshing).
 * Click a tile to enter draw mode for that tile.
 *
 * @param {HTMLElement} container
 * @param {string}      gatewayBase
 * @param {number}      cols
 * @param {number}      rows
 */
export class PresenterMosaic {
  constructor(container, gatewayBase, cols, rows) {
    this.gatewayBase = gatewayBase;
    this.cols = cols;
    this.rows = rows;

    // Compute tile display size so the mosaic fits in ~700px
    this.tileDisplayPx = Math.max(4, Math.floor(Math.min(700 / cols, 700 / rows)));
    this.cellSize = Math.max(1, Math.floor(this.tileDisplayPx / CANVAS_SIZE));

    // pixelMaps[x][y] = Map<"px:py", color>
    this.pixelMaps = Array.from({ length: cols }, () =>
      Array.from({ length: rows }, () => new Map())
    );

    this._pollInterval = null;
    this._activeEditor = null;

    this._buildUI(container);
    this._fetchAll();
    this._startPolling();
  }

  _buildUI(container) {
    const tilePx = this.cellSize * CANVAS_SIZE;

    const gridStyle = [
      "display:grid",
      `grid-template-columns:repeat(${this.cols}, ${tilePx}px)`,
      "gap:2px",
      "border:1px solid #ccc",
      "padding:4px",
      "background:#f3f4f6",
      "width:fit-content",
    ].join(";");

    container.innerHTML = `
      <div style="font-family:sans-serif;display:flex;flex-direction:column;align-items:center;gap:8px">
        <div style="display:flex;align-items:center;gap:12px;flex-wrap:wrap;justify-content:center">
          <span id="mosaic-status" style="font-size:13px;color:#666">● loading...</span>
          <span style="font-size:12px;color:#999">${this.cols}×${this.rows} | ${this.gatewayBase}</span>
        </div>
        <div id="mosaic-grid" style="${gridStyle}"></div>
        <div id="tile-editor" style="display:none"></div>
      </div>`;

    this.statusEl = container.querySelector("#mosaic-status");
    this.gridEl   = container.querySelector("#mosaic-grid");
    this.editorEl = container.querySelector("#tile-editor");

    this.canvases = [];
    for (let y = 0; y < this.rows; y++) {
      for (let x = 0; x < this.cols; x++) {
        const el = document.createElement("canvas");
        el.width  = tilePx;
        el.height = tilePx;
        el.title  = `canvas-${x}-${y}`;
        el.style.cssText = `cursor:pointer;image-rendering:pixelated;display:block`;
        el.addEventListener("click", () => this._openEditor(x, y));
        this.gridEl.appendChild(el);
        this.canvases.push({ el, x, y });
      }
    }
  }

  _canvasEl(x, y) {
    return this.canvases.find(c => c.x === x && c.y === y)?.el;
  }

  async _fetchAll() {
    const promises = [];
    for (let x = 0; x < this.cols; x++) {
      for (let y = 0; y < this.rows; y++) {
        promises.push(
          fetchCanvas(this.gatewayBase, x, y).then(map => ({ x, y, map }))
        );
      }
    }
    const results = await Promise.allSettled(promises);
    for (const r of results) {
      if (r.status !== "fulfilled") continue;
      const { x, y, map } = r.value;
      this.pixelMaps[x][y] = map;
      const el = this._canvasEl(x, y);
      if (el) renderPixels(el, map, this.cellSize);
    }
    this.statusEl.textContent = `● live — ${new Date().toLocaleTimeString()}`;
    this.statusEl.style.color = "#22c55e";
  }

  _startPolling() {
    this._pollInterval = setInterval(() => this._fetchAll(), 1000);
  }

  _openEditor(x, y) {
    if (this._activeEditor) {
      this._activeEditor.destroy();
      this._activeEditor = null;
    }
    this.editorEl.style.display = "block";
    this.editorEl.innerHTML = `
      <div style="border-top:1px solid #e5e7eb;padding-top:8px;margin-top:4px">
        <div style="font-size:13px;margin-bottom:6px;font-family:sans-serif">
          Editing canvas-${x}-${y} &nbsp;
          <button id="close-editor" style="padding:2px 8px;cursor:pointer">✕ close</button>
        </div>
        <div id="editor-mount"></div>
      </div>`;
    this.editorEl.querySelector("#close-editor").addEventListener("click", () => {
      this._activeEditor?.destroy();
      this._activeEditor = null;
      this.editorEl.style.display = "none";
    });
    this._activeEditor = new AudienceCanvas(
      this.editorEl.querySelector("#editor-mount"),
      this.gatewayBase, x, y
    );
  }

  destroy() {
    clearInterval(this._pollInterval);
    this._activeEditor?.destroy();
  }
}
