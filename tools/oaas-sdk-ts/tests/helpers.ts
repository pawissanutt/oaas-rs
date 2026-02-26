/**
 * Test Helpers — mock implementations of WIT host interfaces.
 *
 * These mocks simulate the host-side behavior that would be provided by
 * the ODGM WASM runtime, allowing pure-JS testing of the SDK.
 */

import type {
  HostObjectProxy,
  HostObjectContext,
  ObjectRefData,
  FieldEntry,
  ObjData,
  LogLevel,
} from "../src/types.js";

// ─── Mock Object Proxy ─────────────────────────────────────

export interface MockProxyOptions {
  ref: ObjectRefData;
  /** Initial field data (key → raw bytes). */
  initialData?: Map<string, Uint8Array>;
}

const encoder = new TextEncoder();

/**
 * Creates a mock HostObjectProxy that stores data in-memory.
 * Tracks all calls for assertion.
 */
export function createMockProxy(opts: MockProxyOptions): {
  proxy: HostObjectProxy;
  data: Map<string, Uint8Array>;
  calls: Array<{ method: string; args: unknown[] }>;
  invokeHandler?: (
    fnName: string,
    payload: Uint8Array | null
  ) => Uint8Array | null;
} {
  const data = new Map(opts.initialData ?? []);
  const calls: Array<{ method: string; args: unknown[] }> = [];
  let invokeHandler:
    | ((fnName: string, payload: Uint8Array | null) => Uint8Array | null)
    | undefined;

  const proxy: HostObjectProxy = {
    ref(): ObjectRefData {
      calls.push({ method: "ref", args: [] });
      return { ...opts.ref };
    },

    get(key: string): Uint8Array | null {
      calls.push({ method: "get", args: [key] });
      return data.get(key) ?? null;
    },

    getMany(keys: string[]): FieldEntry[] {
      calls.push({ method: "getMany", args: [keys] });
      const entries: FieldEntry[] = [];
      for (const key of keys) {
        const value = data.get(key);
        if (value !== undefined) {
          entries.push({ key, value });
        }
      }
      return entries;
    },

    set(key: string, value: Uint8Array): void {
      calls.push({ method: "set", args: [key, value] });
      data.set(key, value);
    },

    setMany(entries: FieldEntry[]): void {
      calls.push({ method: "setMany", args: [entries] });
      for (const entry of entries) {
        data.set(entry.key, entry.value);
      }
    },

    delete(key: string): void {
      calls.push({ method: "delete", args: [key] });
      data.delete(key);
    },

    getAll(): ObjData {
      calls.push({ method: "getAll", args: [] });
      return {
        entries: Array.from(data.entries()).map(([key, rawBytes]) => ({
          key,
          value: { data: rawBytes, valType: "byte" as const },
        })),
      };
    },

    setAll(objData: ObjData): void {
      calls.push({ method: "setAll", args: [objData] });
      data.clear();
      for (const entry of objData.entries) {
        data.set(entry.key, entry.value.data);
      }
    },

    invoke(
      fnName: string,
      payload: Uint8Array | null
    ): Uint8Array | null {
      calls.push({ method: "invoke", args: [fnName, payload] });
      if (invokeHandler) {
        return invokeHandler(fnName, payload);
      }
      return null;
    },
  };

  const result = { proxy, data, calls, invokeHandler };

  // Allow setting invokeHandler after creation
  Object.defineProperty(result, "invokeHandler", {
    get: () => invokeHandler,
    set: (v) => {
      invokeHandler = v;
    },
  });

  return result;
}

// ─── Mock Object Context ───────────────────────────────────

export interface MockContextOptions {
  /** Map of ref-string → mock proxy for cross-object lookups. */
  proxies?: Map<string, HostObjectProxy>;
}

/**
 * Creates a mock HostObjectContext.
 * Tracks log calls and cross-object proxy lookups.
 */
export function createMockContext(opts?: MockContextOptions): {
  context: HostObjectContext;
  logs: Array<{ level: LogLevel; message: string }>;
  proxies: Map<string, HostObjectProxy>;
} {
  const proxies = opts?.proxies ?? new Map();
  const logs: Array<{ level: LogLevel; message: string }> = [];

  const context: HostObjectContext = {
    object(ref: ObjectRefData): HostObjectProxy {
      const key = `${ref.cls}/${ref.partitionId}/${ref.objectId}`;
      const proxy = proxies.get(key);
      if (!proxy) {
        throw new Error(`Mock: no proxy registered for ref "${key}"`);
      }
      return proxy;
    },

    objectByStr(refStr: string): HostObjectProxy {
      const proxy = proxies.get(refStr);
      if (!proxy) {
        throw new Error(
          `Mock: no proxy registered for ref-str "${refStr}"`
        );
      }
      return proxy;
    },

    log(level: LogLevel, message: string): void {
      logs.push({ level, message });
    },
  };

  return { context, logs, proxies };
}

// ─── Utility ───────────────────────────────────────────────

/**
 * Encode a JSON value to Uint8Array bytes.
 */
export function jsonBytes(value: unknown): Uint8Array {
  return encoder.encode(JSON.stringify(value));
}

/**
 * Decode Uint8Array bytes to a JSON value.
 */
export function fromJsonBytes<T = unknown>(bytes: Uint8Array): T {
  return JSON.parse(new TextDecoder().decode(bytes)) as T;
}
