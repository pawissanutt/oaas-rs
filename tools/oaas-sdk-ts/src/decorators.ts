/**
 * Decorators — @service, @method, @getter, @setter
 *
 * These decorators collect metadata about the user's OaaS service class.
 * The metadata is stored on the class constructor and used by the dispatch shim
 * and the package metadata extractor.
 *
 * Following the Python SDK's @oaas.service / @oaas.method pattern.
 */

// ─── Metadata Symbols ──────────────────────────────────────

/** Symbol key for storing service metadata on the class constructor. */
export const SERVICE_METADATA = Symbol("oaas:service");

/** Symbol key for storing method metadata on the class prototype methods. */
export const METHOD_METADATA = Symbol("oaas:method");

/** Symbol key for storing getter metadata on the class prototype methods. */
export const GETTER_METADATA = Symbol("oaas:getter");

/** Symbol key for storing setter metadata on the class prototype methods. */
export const SETTER_METADATA = Symbol("oaas:setter");

// ─── Metadata Interfaces ───────────────────────────────────

export interface ServiceMetadata {
  /** The OaaS class name. */
  name: string;
  /** Optional package name. */
  package?: string;
}

export interface MethodMetadata {
  /** Method name as registered in OaaS (defaults to the JS method name). */
  name: string;
  /** If true, skip the load/save state cycle for this method. */
  stateless: boolean;
  /** Optional timeout in milliseconds. */
  timeout?: number;
}

export interface GetterMetadata {
  /** Method name on the class. */
  methodName: string;
  /** The field this getter reads (defaults to method name). */
  field: string;
}

export interface SetterMetadata {
  /** Method name on the class. */
  methodName: string;
  /** The field this setter writes (defaults to method name). */
  field: string;
}

// ─── Metadata Storage ──────────────────────────────────────

/**
 * Get all method metadata registered on a class.
 */
export function getMethodMetadata(
  target: object
): Map<string, MethodMetadata> {
  return (
    (target as Record<symbol, Map<string, MethodMetadata>>)[METHOD_METADATA] ??
    new Map()
  );
}

/**
 * Get service metadata from a class constructor.
 */
export function getServiceMetadata(
  target: Function
): ServiceMetadata | undefined {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (target as any)[SERVICE_METADATA] as ServiceMetadata | undefined;
}

/**
 * Get all getter metadata registered on a class.
 */
export function getGetterMetadata(
  target: object
): Map<string, GetterMetadata> {
  return (
    (target as Record<symbol, Map<string, GetterMetadata>>)[GETTER_METADATA] ??
    new Map()
  );
}

/**
 * Get all setter metadata registered on a class.
 */
export function getSetterMetadata(
  target: object
): Map<string, SetterMetadata> {
  return (
    (target as Record<symbol, Map<string, SetterMetadata>>)[SETTER_METADATA] ??
    new Map()
  );
}

// ─── Decorators ────────────────────────────────────────────

/**
 * `@service(name, opts?)` — Register a class as an OaaS service.
 *
 * Example:
 * ```typescript
 * @service("Counter", { package: "example" })
 * class Counter extends OaaSObject { ... }
 * ```
 */
export function service(
  name: string,
  opts?: { package?: string }
): ClassDecorator {
  return function (target: Function) {
    const metadata: ServiceMetadata = {
      name,
      package: opts?.package,
    };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (target as any)[SERVICE_METADATA] = metadata;
  };
}

/**
 * `@method(opts?)` — Mark a method as an invocable OaaS function.
 *
 * Example:
 * ```typescript
 * @method()
 * async increment(amount: number = 1): Promise<number> { ... }
 *
 * @method({ stateless: true })
 * async computeHash(data: string): Promise<string> { ... }
 * ```
 */
export function method(
  opts?: { stateless?: boolean; timeout?: number }
): MethodDecorator {
  return function (
    target: object,
    propertyKey: string | symbol,
    _descriptor: PropertyDescriptor
  ) {
    const proto = target as Record<symbol, Map<string, MethodMetadata>>;
    if (!proto[METHOD_METADATA]) {
      proto[METHOD_METADATA] = new Map();
    }
    const methodName = String(propertyKey);
    proto[METHOD_METADATA].set(methodName, {
      name: methodName,
      stateless: opts?.stateless ?? false,
      timeout: opts?.timeout,
    });
  };
}

/**
 * `@getter(field?)` — Read-only accessor for a field.
 * Not exported as an RPC endpoint. Used for package metadata.
 *
 * Example:
 * ```typescript
 * @getter("count")
 * getCount(): number { return this.count; }
 * ```
 */
export function getter(field?: string): MethodDecorator {
  return function (
    target: object,
    propertyKey: string | symbol,
    _descriptor: PropertyDescriptor
  ) {
    const proto = target as Record<symbol, Map<string, GetterMetadata>>;
    if (!proto[GETTER_METADATA]) {
      proto[GETTER_METADATA] = new Map();
    }
    const methodName = String(propertyKey);
    proto[GETTER_METADATA].set(methodName, {
      methodName,
      field: field ?? methodName,
    });
  };
}

/**
 * `@setter(field?)` — Write accessor for a field.
 * Not exported as an RPC endpoint. Used for package metadata.
 *
 * Example:
 * ```typescript
 * @setter("count")
 * setCount(value: number): void { this.count = value; }
 * ```
 */
export function setter(field?: string): MethodDecorator {
  return function (
    target: object,
    propertyKey: string | symbol,
    _descriptor: PropertyDescriptor
  ) {
    const proto = target as Record<symbol, Map<string, SetterMetadata>>;
    if (!proto[SETTER_METADATA]) {
      proto[SETTER_METADATA] = new Map();
    }
    const methodName = String(propertyKey);
    proto[SETTER_METADATA].set(methodName, {
      methodName,
      field: field ?? methodName,
    });
  };
}
