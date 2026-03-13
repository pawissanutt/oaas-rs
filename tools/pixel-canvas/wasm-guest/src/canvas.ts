/**
 * PixelCanvas — TypeScript OaaS WASM guest for the pixel canvas tutorial.
 *
 * Each instance represents a 32×32 canvas tile in the mosaic.
 * Each pixel is stored as its own entry on the object:
 *   key: "x:y" (e.g. "15:31")
 *   value: CSS color string (e.g. "#FF0000")
 *
 * Uses this.self (ObjectProxy) for direct per-entry operations
 * so each pixel maps to one value entry in the object store.
 */

import { service, method, OaaSObject } from "@oaas/sdk";

const SIZE = 32;

function parseColor(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.substring(0, 2), 16),
    parseInt(h.substring(2, 4), 16),
    parseInt(h.substring(4, 6), 16),
  ];
}

function averageColors(colors: string[]): string {
  let r = 0,
    g = 0,
    b = 0;
  for (const c of colors) {
    const [cr, cg, cb] = parseColor(c);
    r += cr;
    g += cg;
    b += cb;
  }
  const n = colors.length;
  r = Math.round(r / n);
  g = Math.round(g / n);
  b = Math.round(b / n);
  return (
    "#" +
    r.toString(16).padStart(2, "0") +
    g.toString(16).padStart(2, "0") +
    b.toString(16).padStart(2, "0")
  ).toUpperCase();
}

@service("PixelCanvas", { package: "pixel-canvas" })
class PixelCanvas extends OaaSObject {
  // No declared fields — all state is managed via per-entry proxy ops.

  /** Paint a single pixel at (x, y) with the given color. */
  @method()
  async paint(input: { x: number; y: number; color: string }): Promise<void> {
    await this.self.set(`${input.x}:${input.y}`, input.color);
  }

  /** Paint multiple pixels at once. entries: Record<"x:y", color> */
  @method()
  async paintBatch(entries: Record<string, string>): Promise<void> {
    await this.self.setMany(entries);
  }

  /** Get the full canvas pixel map (all entries). */
  @method()
  async getCanvas(): Promise<Record<string, string>> {
    const all = await this.self.getAll();
    const result: Record<string, string> = {};
    for (const [key, value] of Object.entries(all)) {
      if (typeof value === "string") {
        result[key] = value;
      }
    }
    return result;
  }

  /** Set metadata (e.g. display name). */
  @method()
  async setMeta(input: { name: string }): Promise<void> {
    await this.self.set("_meta", { name: input.name });
  }

  /** Clear all pixels by setting them to white. */
  @method()
  async clear(): Promise<void> {
    const all = await this.self.getAll();
    for (const key of Object.keys(all)) {
      if (key !== "_meta") {
        await this.self.delete(key);
      }
    }
  }

  /**
   * Run one Game of Life step across the entire mosaic grid.
   *
   * All canvas objects are stitched into a global (cols*32)×(rows*32) grid.
   * Standard Conway rules apply:
   *   - Alive cell with 2-3 neighbors → survives (keeps color)
   *   - Dead cell with exactly 3 neighbors → born (average color of parents)
   *   - Otherwise → dies
   *
   * This is a stateless cross-object function: it reads/writes multiple
   * canvas objects via this.object(), not this.self.
   */
  @method({ stateless: true })
  async golStep(
    input: { cols: number; rows: number },
  ): Promise<{ births: number; deaths: number }> {
    const { cols, rows } = input;
    const totalW = cols * SIZE;
    const totalH = rows * SIZE;

    // 1. Read all canvas objects into a global color grid
    //    grid[gy][gx] = color string or null
    const grid: (string | null)[][] = Array.from({ length: totalH }, () =>
      Array<string | null>(totalW).fill(null),
    );

    interface CanvasRef {
      cx: number;
      cy: number;
      proxy: ReturnType<OaaSObject["object"]>;
    }
    const canvases: CanvasRef[] = [];

    for (let cy = 0; cy < rows; cy++) {
      for (let cx = 0; cx < cols; cx++) {
        const proxy = this.sibling(`canvas-${cx}-${cy}`);
        canvases.push({ cx, cy, proxy });
        const all = await proxy.getAll();
        for (const [key, value] of Object.entries(all)) {
          if (key.startsWith("_") || typeof value !== "string") continue;
          const sep = key.indexOf(":");
          if (sep < 0) continue;
          const lx = parseInt(key.substring(0, sep), 10);
          const ly = parseInt(key.substring(sep + 1), 10);
          if (isNaN(lx) || isNaN(ly)) continue;
          grid[cy * SIZE + ly][cx * SIZE + lx] = value;
        }
      }
    }

    // 2. Compute next generation
    const nextGrid: (string | null)[][] = Array.from(
      { length: totalH },
      () => Array<string | null>(totalW).fill(null),
    );

    let births = 0;
    let deaths = 0;

    for (let gy = 0; gy < totalH; gy++) {
      for (let gx = 0; gx < totalW; gx++) {
        const neighbors: string[] = [];
        for (let dy = -1; dy <= 1; dy++) {
          for (let dx = -1; dx <= 1; dx++) {
            if (dx === 0 && dy === 0) continue;
            const nx = gx + dx;
            const ny = gy + dy;
            if (nx < 0 || nx >= totalW || ny < 0 || ny >= totalH) continue;
            const c = grid[ny][nx];
            if (c !== null) neighbors.push(c);
          }
        }

        const alive = grid[gy][gx] !== null;
        const count = neighbors.length;

        if (alive && (count === 2 || count === 3)) {
          nextGrid[gy][gx] = grid[gy][gx]; // survives, keep color
        } else if (!alive && count === 3) {
          nextGrid[gy][gx] = averageColors(neighbors); // born
          births++;
        } else if (alive) {
          deaths++; // dies
        }
      }
    }

    // 3. Write back only changed pixels to each canvas
    for (const { cx, cy, proxy } of canvases) {
      const updates: Record<string, string> = {};
      const deletes: string[] = [];

      for (let ly = 0; ly < SIZE; ly++) {
        for (let lx = 0; lx < SIZE; lx++) {
          const gy = cy * SIZE + ly;
          const gx = cx * SIZE + lx;
          const key = `${lx}:${ly}`;
          const was = grid[gy][gx];
          const now = nextGrid[gy][gx];

          if (was !== now) {
            if (now !== null) {
              updates[key] = now;
            } else {
              deletes.push(key);
            }
          }
        }
      }

      if (Object.keys(updates).length > 0) {
        await proxy.setMany(updates);
      }
      for (const key of deletes) {
        await proxy.delete(key);
      }
    }

    return { births, deaths };
  }
}

export default PixelCanvas;
