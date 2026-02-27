/**
 * @oaas/sdk type definitions for Monaco IntelliSense.
 *
 * This file provides the TypeScript declaration content that gets registered
 * as an extra lib in the Monaco editor, giving users IntelliSense when writing
 * OaaS scripts.
 */

export const OAAS_SDK_TYPE_DEFINITIONS = `
declare module "@oaas/sdk" {
  /**
   * Immutable object identity: (class, partitionId, objectId).
   */
  export class ObjectRef {
    readonly cls: string;
    readonly partitionId: number;
    readonly objectId: string;

    /** Construct an ObjectRef from individual components. */
    static from(cls: string, partitionId: number, objectId: string): ObjectRef;

    /** Parse a string of the form "cls/partition/objectId". */
    static parse(refStr: string): ObjectRef;

    /** Returns the string form "cls/partition/objectId". */
    toString(): string;

    /** Check equality with another ObjectRef. */
    equals(other: ObjectRef): boolean;
  }

  /**
   * Proxy handle for cross-object access.
   * Wraps a WIT object-proxy resource.
   */
  export class ObjectProxy {
    /** The identity of the referenced object. */
    readonly ref: ObjectRef;

    /** Read a single field, JSON-deserialized. */
    get<T = unknown>(key: string): Promise<T | null>;

    /** Batch read multiple fields. */
    getMany<T = unknown>(...keys: string[]): Promise<Record<string, T>>;

    /** Write a single field (JSON-serialized). */
    set<T = unknown>(key: string, value: T): Promise<void>;

    /** Batch write multiple fields. */
    setMany(entries: Record<string, unknown>): Promise<void>;

    /** Delete a field. */
    delete(key: string): Promise<void>;

    /** Read the entire object. */
    getAll(): Promise<Record<string, unknown>>;

    /** Invoke a method on this object. */
    invoke<T = unknown>(fnName: string, payload?: unknown): Promise<T>;

    /** Returns the string form of the object's identity. */
    toString(): string;
  }

  /**
   * Abstract base class for OaaS services.
   *
   * Extend this class, decorate with @service and @method, and export as default.
   * Fields declared with defaults are automatically persisted.
   *
   * @example
   * \`\`\`ts
   * import { service, method, OaaSObject } from "@oaas/sdk";
   *
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
   * export default Counter;
   * \`\`\`
   */
  export abstract class OaaSObject {
    /** The identity of the current object (set by the runtime). */
    readonly ref: ObjectRef;

    /**
     * Get a proxy to another object for cross-object access.
     * @param ref - ObjectRef instance or string form "cls/partition/objectId"
     */
    object(ref: ObjectRef | string): ObjectProxy;

    /**
     * Structured logging to the host tracing system.
     * @param level - "debug" | "info" | "warn" | "error"
     * @param message - Log message
     */
    log(level: "debug" | "info" | "warn" | "error", message: string): void;
  }

  /**
   * Application-level error.
   * Throw this from a method to return an "app-error" response.
   * Any other thrown error becomes a "system-error".
   */
  export class OaaSError extends Error {
    constructor(message: string);
  }

  /**
   * Type guard for OaaSError.
   */
  export function isOaaSError(err: unknown): err is OaaSError;

  // ---- Decorators ----

  interface ServiceOptions {
    /** Package name for the service. */
    package?: string;
  }

  interface MethodOptions {
    /**
     * If true, skip loading/saving object state.
     * Use for pure-compute methods with no side effects.
     */
    stateless?: boolean;
    /** Timeout in milliseconds. */
    timeout?: number;
  }

  /**
   * Register a class as an OaaS service.
   *
   * @param name - The service name (maps to the OaaS class key)
   * @param opts - Optional configuration (package name, etc.)
   */
  export function service(name: string, opts?: ServiceOptions): ClassDecorator;

  /**
   * Expose a method as an invocable function.
   * Methods are stateful by default — object fields are loaded before
   * and saved after each invocation.
   *
   * @param opts - Optional: { stateless: true } to skip state management
   */
  export function method(opts?: MethodOptions): MethodDecorator;

  /**
   * Mark a method as a read-only getter for a field.
   * Not exported as an RPC endpoint.
   *
   * @param field - Field name (defaults to method name)
   */
  export function getter(field?: string): MethodDecorator;

  /**
   * Mark a method as a write accessor for a field.
   * Not exported as an RPC endpoint.
   *
   * @param field - Field name (defaults to method name)
   */
  export function setter(field?: string): MethodDecorator;
}
`;

/**
 * Default template for a new OaaS script.
 */
export const DEFAULT_SCRIPT_TEMPLATE = `import { service, method, OaaSObject } from "@oaas/sdk";

@service("MyService", { package: "my-package" })
class MyService extends OaaSObject {
  // Declare fields with defaults — they are auto-persisted
  count: number = 0;

  /**
   * Example stateful method.
   * Object fields are loaded before and saved after each invocation.
   */
  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;
    this.log("info", \`Counter incremented to \${this.count}\`);
    return this.count;
  }

  /**
   * Example stateless method — pure computation, no state access.
   */
  @method({ stateless: true })
  async add(args: { a: number; b: number }): Promise<number> {
    return args.a + args.b;
  }
}

export default MyService;
`;

/**
 * Counter example template.
 */
export const COUNTER_TEMPLATE = `import { service, method, getter, setter, OaaSObject, OaaSError } from "@oaas/sdk";

@service("Counter", { package: "example" })
class Counter extends OaaSObject {
  count: number = 0;
  history: string[] = [];

  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;
    this.history.push(\`Added \${amount}\`);
    this.log("info", \`Counter incremented by \${amount} → \${this.count}\`);
    return this.count;
  }

  @method()
  async decrement(amount: number = 1): Promise<number> {
    if (this.count - amount < 0) {
      throw new OaaSError(
        \`Cannot decrement by \${amount}: current count is \${this.count}\`
      );
    }
    this.count -= amount;
    this.history.push(\`Removed \${amount}\`);
    return this.count;
  }

  @method()
  async reset(): Promise<void> {
    this.count = 0;
    this.history = [];
    this.log("info", "Counter reset");
  }

  @method({ stateless: true })
  async add(args: { a: number; b: number }): Promise<number> {
    return args.a + args.b;
  }
}

export default Counter;
`;

/**
 * Greeting (stateless) example template.
 */
export const GREETING_TEMPLATE = `import { service, method, OaaSObject } from "@oaas/sdk";

@service("Greeting", { package: "example" })
class Greeting extends OaaSObject {
  @method({ stateless: true })
  async greet(name: string): Promise<string> {
    return \`Hello, \${name}! Welcome to OaaS.\`;
  }

  @method({ stateless: true })
  async farewell(name: string): Promise<string> {
    return \`Goodbye, \${name}! See you next time.\`;
  }
}

export default Greeting;
`;

/**
 * All available script templates.
 */
export const SCRIPT_TEMPLATES = [
  { name: "Blank Service", description: "Empty OaaS service with one example method", template: DEFAULT_SCRIPT_TEMPLATE },
  { name: "Counter", description: "Stateful counter with increment/decrement", template: COUNTER_TEMPLATE },
  { name: "Greeting", description: "Stateless greeting service", template: GREETING_TEMPLATE },
] as const;
