/**
 * WIT-level types that mirror the oaas:odgm WIT interface.
 * These are the raw types that the WASM Component Model uses.
 * The SDK wraps these in higher-level TypeScript classes.
 */

// ─── WIT Mirror Types ──────────────────────────────────────

/** Mirrors WIT `response-status` enum. */
export type ResponseStatus =
  | "okay"
  | "invalid-request"
  | "app-error"
  | "system-error";

/** Mirrors WIT `invocation-response` record. */
export interface InvocationResponse {
  payload: Uint8Array | null;
  status: ResponseStatus;
  headers: KeyValue[];
}

/** Mirrors WIT `key-value` record. */
export interface KeyValue {
  key: string;
  value: string;
}

/** Mirrors WIT `object-ref` record. */
export interface ObjectRefData {
  cls: string;
  partitionId: number;
  objectId: string;
}

/** Mirrors WIT `field-entry` record. */
export interface FieldEntry {
  key: string;
  value: Uint8Array;
}

/** Mirrors WIT `odgm-error` variant. */
export type OdgmError =
  | { tag: "not-found" }
  | { tag: "permission-denied" }
  | { tag: "invalid-argument"; val: string }
  | { tag: "internal"; val: string };

/** Mirrors WIT `log-level` enum. */
export type LogLevel = "debug" | "info" | "warn" | "error";

/** Mirrors WIT `val-type` enum. */
export type ValType = "byte" | "crdt-map";

/** Mirrors WIT `val-data` record. */
export interface ValData {
  data: Uint8Array;
  valType: ValType;
}

/** Mirrors WIT `object-meta` record. */
export interface ObjectMeta {
  clsId: string;
  partitionId: number;
  objectId?: string;
}

/** Mirrors WIT `entry` record. */
export interface Entry {
  key: string;
  value: ValData;
}

/** Mirrors WIT `obj-data` record. */
export interface ObjData {
  metadata?: ObjectMeta;
  entries: Entry[];
}

// ─── Host Interface Types ──────────────────────────────────

/**
 * Host-side object-proxy interface.
 * In the actual WASM runtime, this is backed by the WIT resource.
 * In the SDK shim, we wrap this with the ObjectProxy class.
 */
export interface HostObjectProxy {
  ref(): ObjectRefData;
  get(key: string): Uint8Array | null;
  getMany(keys: string[]): FieldEntry[];
  set(key: string, value: Uint8Array): void;
  setMany(entries: FieldEntry[]): void;
  delete(key: string): void;
  getAll(): ObjData;
  setAll(data: ObjData): void;
  invoke(fnName: string, payload: Uint8Array | null): Uint8Array | null;
}

/**
 * Host-side object-context interface.
 * In the actual WASM runtime, this is backed by the WIT imports.
 */
export interface HostObjectContext {
  object(ref: ObjectRefData): HostObjectProxy;
  objectByStr(refStr: string): HostObjectProxy;
  log(level: LogLevel, message: string): void;
}
