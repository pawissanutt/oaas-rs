/** Shared type definitions for the pixel-canvas frontend. */

/** Canvas dimensions (pixels per side). */
export const CANVAS_SIZE = 32;

/** Default display pixels per canvas pixel (audience mode). */
export const CELL_PX = 10;

/** OaaS class name for pixel canvas objects. */
export const CLASS_NAME = "pixel-canvas";

/** Default partition. */
export const PARTITION = 0;

/** Map of "x:y" → CSS color string. */
export type PixelMap = Map<string, string>;

/** Configuration resolved from URL parameters or config form. */
export interface AppConfig {
  mode: "audience" | "presenter";
  gateway: string;
  /** Audience: grid column index. */
  gridX?: number;
  /** Audience: grid row index. */
  gridY?: number;
  /** Presenter: number of columns. */
  cols?: number;
  /** Presenter: number of rows. */
  rows?: number;
}

/** A single entry from the gateway object response. */
export interface EntryData {
  data: string;
  type?: string;
}

/** Gateway GET object response shape. */
export interface ObjectResponse {
  metadata?: {
    cls_id?: string;
    partition_id?: number;
    object_id?: string;
  };
  entries?: Record<string, EntryData>;
}
