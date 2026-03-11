/**
 * Gateway API client for pixel-canvas objects.
 *
 * Handles encoding/decoding of pixel data and REST calls to the OaaS gateway.
 */

import { CLASS_NAME, PARTITION } from "./types.js";
import type { PixelMap, ObjectResponse } from "./types.js";

/** Encode a CSS color string to base64 (UTF-8 safe). */
function encodeColor(color: string): string {
  return btoa(unescape(encodeURIComponent(color)));
}

/** Decode base64 entry data back to a CSS color string. */
function decodeColor(b64: string): string {
  try {
    return decodeURIComponent(escape(atob(b64)));
  } catch {
    return "#ffffff";
  }
}

/** Build the object URL for a canvas tile. */
function objectUrl(gatewayBase: string, gridX: number, gridY: number): string {
  return `${gatewayBase}/api/class/${CLASS_NAME}/${PARTITION}/objects/canvas-${gridX}-${gridY}`;
}

export interface FetchResult {
  ok: boolean;
  pixels: PixelMap;
}

/**
 * Fetch a canvas object from the gateway.
 * Returns { ok, pixels } — ok=false on network/server errors.
 */
export async function fetchCanvas(
  gatewayBase: string,
  gridX: number,
  gridY: number
): Promise<FetchResult> {
  const url = objectUrl(gatewayBase, gridX, gridY);
  let res: Response;
  try {
    res = await fetch(url, { headers: { Accept: "application/json" } });
  } catch (e) {
    console.warn("fetchCanvas network error:", e);
    return { ok: false, pixels: new Map() };
  }
  if (res.status === 404) return { ok: true, pixels: new Map() };
  if (!res.ok) {
    console.warn("fetchCanvas error", res.status);
    return { ok: false, pixels: new Map() };
  }

  const obj: ObjectResponse = await res.json();
  const entries = obj.entries ?? {};
  const pixels: PixelMap = new Map();
  for (const [key, val] of Object.entries(entries)) {
    pixels.set(key, decodeColor(val.data));
  }
  return { ok: true, pixels };
}

/**
 * Save a full pixel map to the gateway via PUT.
 * Returns true on success, false on error.
 */
export async function saveCanvas(
  gatewayBase: string,
  gridX: number,
  gridY: number,
  pixelMap: PixelMap
): Promise<boolean> {
  const entries: Record<string, { data: string; type: string }> = {};
  for (const [key, color] of pixelMap) {
    entries[key] = { data: encodeColor(color), type: "BYTE" };
  }
  const body = {
    metadata: {
      cls_id: CLASS_NAME,
      partition_id: PARTITION,
      object_id: `canvas-${gridX}-${gridY}`,
    },
    entries,
  };
  const url = objectUrl(gatewayBase, gridX, gridY);
  try {
    const res = await fetch(url, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      console.warn("saveCanvas PUT failed:", res.status);
      return false;
    }
    return true;
  } catch (e) {
    console.warn("saveCanvas network error:", e);
    return false;
  }
}
