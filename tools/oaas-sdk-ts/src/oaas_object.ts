/**
 * OaaSObject — abstract base class for user OaaS services.
 *
 * Users extend this class and decorate it with @service / @method.
 * The SDK handles:
 * - Auto-persisted state (type-annotated fields loaded/saved around each method call)
 * - Cross-object access via this.object(ref)
 * - Structured logging via this.log(level, message)
 *
 * Modeled after the Python OaaS SDK's OaaSObject base.
 */

import { ObjectRef } from "./object_ref.js";
import { ObjectProxy } from "./object_proxy.js";
import type { HostObjectContext, LogLevel } from "./types.js";

/**
 * Internal context set by the dispatch shim before calling user methods.
 * Not part of the public API.
 */
export interface OaaSObjectContext {
  /** The self proxy for the current invocation. */
  selfProxy: ObjectProxy;
  /** The host object-context for creating cross-object proxies. */
  hostContext: HostObjectContext;
}

/** Symbol for storing the internal context on an OaaSObject instance. */
export const OAAS_CONTEXT = Symbol("oaas:context");

/**
 * Abstract base class for OaaS service objects.
 *
 * Example:
 * ```typescript
 * @service("Counter", { package: "example" })
 * class Counter extends OaaSObject {
 *   count: number = 0;
 *
 *   @method()
 *   async increment(amount: number = 1): Promise<number> {
 *     this.count += amount;
 *     return this.count;
 *   }
 * }
 * ```
 */
export abstract class OaaSObject {
  /**
   * The identity of this object. Set by the SDK shim from invocation context.
   * Available inside @method decorated methods.
   */
  get ref(): ObjectRef {
    const ctx = this._getContext();
    return ctx.selfProxy.ref;
  }

  /**
   * Proxy to this object's own entries for direct key-value operations.
   * Use for per-entry storage instead of auto-persisted fields.
   *
   * Example:
   * ```typescript
   * await this.self.set("key", value);
   * const val = await this.self.get<string>("key");
   * const all = await this.self.getAll();
   * ```
   */
  get self(): ObjectProxy {
    return this._getContext().selfProxy;
  }

  /**
   * Get a proxy to another object by ObjectRef or string form "cls/partition/id".
   */
  object(ref: ObjectRef | string): ObjectProxy {
    const ctx = this._getContext();
    if (typeof ref === "string") {
      const hostProxy = ctx.hostContext.objectByStr(ref);
      return new ObjectProxy(hostProxy);
    }
    const hostProxy = ctx.hostContext.object(ref.toData());
    return new ObjectProxy(hostProxy);
  }

  /**
   * Get a proxy to another object in the same class and partition by object ID.
   *
   * Shorthand for `this.object(ObjectRef.from(this.ref.cls, this.ref.partitionId, objectId))`.
   */
  sibling(objectId: string): ObjectProxy {
    const { cls, partitionId } = this.ref;
    return this.object(ObjectRef.from(cls, partitionId, objectId));
  }

  /**
   * Structured logging to the host tracing system.
   */
  log(level: LogLevel, message: string): void {
    const ctx = this._getContext();
    ctx.hostContext.log(level, message);
  }

  /** Get the internal context, throwing if not set. */
  private _getContext(): OaaSObjectContext {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = (this as any)[OAAS_CONTEXT] as OaaSObjectContext | undefined;
    if (!ctx) {
      throw new Error(
        "OaaSObject context not initialized. " +
          "This method can only be called during an invocation handled by the SDK shim."
      );
    }
    return ctx;
  }
}
