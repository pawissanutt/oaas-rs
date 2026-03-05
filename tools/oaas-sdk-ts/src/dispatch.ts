/**
 * Method Dispatch Shim — the entry point compiled by ComponentizeJS.
 *
 * This module wires the `guest-object.on-invoke` export to the user's
 * decorated class methods. It handles:
 * - Loading the user's default export class
 * - Discovering declared fields for auto-persistence
 * - Routing `on-invoke(self, fn-name, payload, headers)` to the correct method
 * - Transparent state management (load before, save after)
 * - Error wrapping (OaaSError → app-error, other → system-error)
 *
 * This is the bridge between the WIT component model and the user's TypeScript class.
 */

import { ObjectProxy } from "./object_proxy.js";
import { OaaSObject, OAAS_CONTEXT } from "./oaas_object.js";
import type { OaaSObjectContext } from "./oaas_object.js";
import { isOaaSError } from "./errors.js";
import { getMethodMetadata } from "./decorators.js";
import { discoverFields, loadFields, saveChangedFields } from "./state.js";
import type {
  InvocationResponse,
  KeyValue,
  HostObjectProxy,
  HostObjectContext,
} from "./types.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

// ─── Registry ──────────────────────────────────────────────

/** The registered service class (set by registerService or from default export). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let registeredClass: (new (...args: any[]) => OaaSObject) | null = null;

/** Cached list of declared fields for the registered class. */
let registeredFields: string[] | null = null;

/**
 * Register a service class for dispatch.
 * Called by the module entry point with the user's default export.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function registerService(cls: new (...args: any[]) => OaaSObject): void {
  registeredClass = cls;
  registeredFields = discoverFields(cls);
}

/**
 * Get the registered class and fields.
 * @throws Error if no class has been registered.
 */
function getRegistered(): {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cls: new (...args: any[]) => OaaSObject;
  fields: string[];
} {
  if (!registeredClass || !registeredFields) {
    throw new Error(
      "No OaaS service class registered. " +
        "Ensure your module has a default export extending OaaSObject."
    );
  }
  return { cls: registeredClass, fields: registeredFields };
}

// ─── Response Helpers ──────────────────────────────────────

function okayResponse(payload?: unknown): InvocationResponse {
  return {
    payload:
      payload !== undefined
        ? encoder.encode(JSON.stringify(payload))
        : null,
    status: "okay",
    headers: [],
  };
}

function appErrorResponse(message: string): InvocationResponse {
  return {
    payload: encoder.encode(JSON.stringify({ error: message })),
    status: "app-error",
    headers: [],
  };
}

function systemErrorResponse(message: string): InvocationResponse {
  return {
    payload: encoder.encode(JSON.stringify({ error: message })),
    status: "system-error",
    headers: [],
  };
}

function invalidRequestResponse(message: string): InvocationResponse {
  return {
    payload: encoder.encode(JSON.stringify({ error: message })),
    status: "invalid-request",
    headers: [],
  };
}

// ─── Dispatch ──────────────────────────────────────────────

/**
 * Handle an incoming invocation — implements `guest-object.on-invoke`.
 *
 * This is the main dispatch function that maps WIT calls to user class methods.
 *
 * @param selfProxy - The WIT object-proxy for the target object (host handle).
 * @param functionName - The name of the method to invoke.
 * @param payload - Optional payload bytes (JSON-encoded arguments).
 * @param headers - Key-value headers from the invocation.
 * @param hostContext - The host object-context for cross-object access.
 */
export function handleInvoke(
  selfProxy: HostObjectProxy,
  functionName: string,
  payload: Uint8Array | null,
  headers: KeyValue[],
  hostContext: HostObjectContext
): InvocationResponse {
  try {
    const { cls, fields } = getRegistered();

    // Create an instance of the user's class.
    const instance = new cls();

    // Set up the OaaS context so this.ref, this.object(), this.log() work.
    const proxy = new ObjectProxy(selfProxy);
    const ctx: OaaSObjectContext = {
      selfProxy: proxy,
      hostContext,
    };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (instance as any)[OAAS_CONTEXT] = ctx;

    // Look up the method on the instance.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const methodFn = (instance as any)[functionName];
    if (typeof methodFn !== "function") {
      return invalidRequestResponse(
        `Method "${functionName}" not found on service class`
      );
    }

    // Check if this method has metadata (is it decorated with @method?)
    const methodMeta = getMethodMetadata(
      Object.getPrototypeOf(instance) as object
    );
    const meta = methodMeta.get(functionName);
    const isStateless = meta?.stateless ?? false;

    // Parse the payload.
    let arg: unknown = undefined;
    if (payload !== null && payload.length > 0) {
      try {
        arg = JSON.parse(decoder.decode(payload));
      } catch {
        return invalidRequestResponse(
          `Failed to parse payload as JSON for method "${functionName}"`
        );
      }
    }

    // State management: load fields before (unless stateless).
    let snapshot: Map<string, string> | null = null;
    if (!isStateless) {
      snapshot = loadFields(
        instance as unknown as Record<string, unknown>,
        selfProxy,
        fields
      );
    }

    // Call the user's method.
    // Note: In the WASM context, async/await is synchronous under the hood.
    // The method may return a value or a Promise (resolved synchronously).
    const result = methodFn.call(instance, arg);

    // State management: save changed fields after (unless stateless).
    if (!isStateless && snapshot !== null) {
      saveChangedFields(
        instance as unknown as Record<string, unknown>,
        selfProxy,
        fields,
        snapshot
      );
    }

    return okayResponse(result);
  } catch (err: unknown) {
    if (isOaaSError(err)) {
      return appErrorResponse((err as Error).message);
    }
    const message =
      err instanceof Error ? err.message : String(err);
    return systemErrorResponse(message);
  }
}

// ─── Headers Utility ───────────────────────────────────────

/**
 * Convert KeyValue array to a plain record for easier access.
 */
export function headersToRecord(
  headers: KeyValue[]
): Record<string, string> {
  const result: Record<string, string> = {};
  for (const kv of headers) {
    result[kv.key] = kv.value;
  }
  return result;
}
