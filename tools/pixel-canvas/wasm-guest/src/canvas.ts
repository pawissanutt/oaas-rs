/**
 * PixelCanvas — TypeScript OaaS WASM guest for the pixel canvas tutorial.
 *
 * Each instance represents a 32×32 canvas tile in the mosaic.
 * Pixels are stored as key-value pairs in the `pixels` field:
 *   key: "x:y" (e.g. "15:31")
 *   value: CSS color string (e.g. "#FF0000")
 *
 * The `meta` field stores optional metadata like display name.
 */

import { service, method, OaaSObject } from "@oaas/sdk";

@service("PixelCanvas", { package: "pixel-canvas" })
class PixelCanvas extends OaaSObject {
  pixels: Record<string, string> = {};
  meta: Record<string, string> = {};

  /** Paint a single pixel at (x, y) with the given color. */
  @method()
  async paint(x: number, y: number, color: string): Promise<void> {
    this.pixels[`${x}:${y}`] = color;
  }

  /** Paint multiple pixels at once. entries: Record<"x:y", color> */
  @method()
  async paintBatch(entries: Record<string, string>): Promise<void> {
    for (const [key, color] of Object.entries(entries)) {
      this.pixels[key] = color;
    }
  }

  /** Get the full canvas pixel map. */
  @method({ stateless: false })
  async getCanvas(): Promise<Record<string, string>> {
    return this.pixels;
  }

  /** Set metadata (e.g. display name). */
  @method()
  async setMeta(name: string): Promise<void> {
    this.meta["name"] = name;
  }

  /** Clear all pixels. */
  @method()
  async clear(): Promise<void> {
    this.pixels = {};
  }
}

export default PixelCanvas;
