# @oaas/sdk — User Guide

> Write TypeScript classes that run as OaaS WASM functions with automatic state management.

## Overview

`@oaas/sdk` is a TypeScript SDK that lets you author OaaS (Object-as-a-Service) functions as plain TypeScript classes. You extend `OaaSObject`, decorate your class and methods, and the SDK handles:

- **Method dispatch** — routing incoming invocations to the right method
- **Auto-persisted state** — object fields are loaded before and saved after each method call
- **Cross-object access** — call methods on other objects via typed proxies
- **Structured logging** — emit log messages to the host tracing system
- **Error classification** — distinguish application errors from system failures

Your class is compiled to a WASM Component (via ComponentizeJS) and deployed into the OaaS runtime, where it runs in-process inside the ODGM data grid — no container images required.

## Installation

```bash
npm install @oaas/sdk
```

## Quick Start

```typescript
import { service, method, OaaSObject } from "@oaas/sdk";

@service("Greeting", { package: "example" })
class Greeting extends OaaSObject {
  @method({ stateless: true })
  async greet(name: string): Promise<string> {
    return `Hello, ${name}! Welcome to OaaS.`;
  }
}

export default Greeting;
```

Key points:
- **One class per module.** Export your class as the default export.
- **`@service`** registers the class name and optional package.
- **`@method`** marks methods as invocable OaaS functions.
- Methods receive a single JSON-deserialized argument and return a JSON-serializable value.

## Decorators

### `@service(name, opts?)`

Registers a class as an OaaS service.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | `string` | Yes | The OaaS class name (used in routing/deployment) |
| `opts.package` | `string` | No | Package name (defaults to class name) |

```typescript
@service("Counter", { package: "example" })
class Counter extends OaaSObject { ... }
```

### `@method(opts?)`

Marks a method as an invocable OaaS function.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `opts.stateless` | `boolean` | `false` | Skip the load/save state cycle |
| `opts.timeout` | `number` | — | Timeout in milliseconds |

```typescript
// Stateful — fields are loaded before and saved after
@method()
async increment(amount: number = 1): Promise<number> { ... }

// Stateless — no field loading/saving, pure computation
@method({ stateless: true })
async add(args: { a: number; b: number }): Promise<number> {
  return args.a + args.b;
}
```

### `@getter(field?)`

Declares a read-only accessor for a field. Used in package metadata generation.

```typescript
@getter("count")
getCount(): number { return this.count; }
```

### `@setter(field?)`

Declares a write accessor for a field. Used in package metadata generation.

```typescript
@setter("count")
setCount(value: number): void { this.count = value; }
```

## OaaSObject Base Class

All service classes must extend `OaaSObject`. It provides:

### `this.ref: ObjectRef`

The identity of the current object (class name, partition ID, object ID). Available inside any `@method`.

```typescript
@method()
async whoAmI(): Promise<string> {
  return this.ref.toString(); // "Counter/0/obj-123"
}
```

### `this.object(ref): ObjectProxy`

Get a proxy to another object. Accepts an `ObjectRef` instance or a string in the form `"cls/partition/objectId"`.

```typescript
@method()
async transfer(targetRef: string): Promise<void> {
  const target = this.object(targetRef);
  await target.invoke("increment", this.count);
  this.count = 0;
}
```

### `this.log(level, message): void`

Emit a structured log message to the host tracing system.

```typescript
this.log("info", `Counter incremented to ${this.count}`);
this.log("error", "Something went wrong");
```

Log levels: `"debug"`, `"info"`, `"warn"`, `"error"`.

## State Management

Fields declared on your class with default values are **automatically persisted** across invocations.

```typescript
@service("Counter")
class Counter extends OaaSObject {
  count: number = 0;           // ← auto-persisted
  history: string[] = [];      // ← auto-persisted

  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;                    // mutate directly
    this.history.push(`Added ${amount}`);    // arrays/objects too
    return this.count;
  }
}
```

### How it works

1. **Before** method call: all declared fields are loaded from the data grid and set on `this`.
2. **During** the method: read and write fields normally — they're plain TypeScript properties.
3. **After** method call: the SDK compares each field against a pre-call snapshot. Only changed fields are written back.

### Stateless methods

Mark methods with `@method({ stateless: true })` to skip the load/save cycle entirely. Use this for pure computations that don't touch object state:

```typescript
@method({ stateless: true })
async computeHash(data: string): Promise<string> {
  // No field loading/saving happens here
  return someHashFunction(data);
}
```

### Field detection

The SDK discovers fields by instantiating your class (with no arguments) and calling `Object.keys()`. Only enumerable own properties with default values are tracked. Private fields, getters, and prototype methods are excluded.

## ObjectRef

`ObjectRef` represents the identity of an OaaS object: **(class, partitionId, objectId)**.

```typescript
import { ObjectRef } from "@oaas/sdk";

// Create from components
const ref = ObjectRef.from("Counter", 0, "my-counter");

// Parse from string form
const ref2 = ObjectRef.parse("Counter/0/my-counter");

// Convert to string
ref.toString(); // "Counter/0/my-counter"

// Compare
ref.equals(ref2); // true
```

`ObjectRef` instances are **immutable** (frozen with `Object.freeze`).

### Validation rules

- `cls` and `objectId` must not contain `/`.
- `partitionId` must be a non-negative integer fitting in a `u32` (0 – 4,294,967,295).

## ObjectProxy

`ObjectProxy` provides typed access to another object's fields and methods.

```typescript
const proxy = this.object("Counter/0/my-counter");

// Read a field (JSON-deserialized)
const count = await proxy.get<number>("count");

// Write a field (JSON-serialized)
await proxy.set("count", 42);

// Read multiple fields
const fields = await proxy.getMany<number>("count", "max");

// Write multiple fields
await proxy.setMany({ count: 0, history: [] });

// Delete a field
await proxy.delete("old-field");

// Read all fields
const all = await proxy.getAll();

// Invoke a method on the remote object
const result = await proxy.invoke<{ a: number; b: number }, number>("add", { a: 1, b: 2 });

// Access the proxy's identity
proxy.ref; // ObjectRef
```

## Error Handling

### Application errors: `OaaSError`

Throw `OaaSError` to signal a user-visible application error. The SDK returns a response with `status: "app-error"`.

```typescript
import { OaaSError } from "@oaas/sdk";

@method()
async decrement(amount: number = 1): Promise<number> {
  if (this.count - amount < 0) {
    throw new OaaSError(`Cannot decrement by ${amount}: count is ${this.count}`);
  }
  this.count -= amount;
  return this.count;
}
```

### System errors

Any non-`OaaSError` exception produces `status: "system-error"`. These indicate bugs or infrastructure failures that the caller shouldn't need to handle explicitly.

### Type guard

Use `isOaaSError()` to check if an unknown error is an `OaaSError` — works across module boundaries:

```typescript
import { isOaaSError } from "@oaas/sdk";

try { ... } catch (err) {
  if (isOaaSError(err)) {
    console.log("App error:", err.message);
  }
}
```

## Package Metadata

The SDK can extract a deployment descriptor from your decorated class. This is used by the Package Manager to register the class and its function bindings.

```typescript
import { extractPackageMetadata, packageMetadataToJson } from "@oaas/sdk";

const meta = extractPackageMetadata(Counter);
// {
//   name: "example",
//   classes: [{
//     name: "Counter",
//     stateFields: ["count", "history"],
//     functions: [
//       { name: "increment", stateless: false },
//       { name: "add", stateless: true }
//     ],
//     getters: [{ method: "getCount", field: "count" }],
//     setters: [{ method: "setCount", field: "count" }]
//   }]
// }

// Or as a JSON string:
const json = packageMetadataToJson(Counter);
```

## Full Example: Stateful Counter

```typescript
import {
  service, method, getter, setter,
  OaaSObject, OaaSError
} from "@oaas/sdk";

@service("Counter", { package: "example" })
class Counter extends OaaSObject {
  count: number = 0;
  history: string[] = [];

  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;
    this.history.push(`Added ${amount}`);
    this.log("info", `Counter incremented by ${amount} → ${this.count}`);
    return this.count;
  }

  @method()
  async decrement(amount: number = 1): Promise<number> {
    if (this.count - amount < 0) {
      throw new OaaSError(
        `Cannot decrement by ${amount}: current count is ${this.count}`
      );
    }
    this.count -= amount;
    this.history.push(`Removed ${amount}`);
    return this.count;
  }

  @method()
  async reset(): Promise<void> {
    this.count = 0;
    this.history = [];
    this.log("info", "Counter reset");
  }

  @method()
  async transfer(targetRef: string): Promise<{ transferred: number }> {
    const target = this.object(targetRef);
    await target.invoke("increment", this.count);
    const transferred = this.count;
    this.count = 0;
    this.history.push(`Transferred ${transferred} to ${targetRef}`);
    this.log("info", `Transferred ${transferred} to ${target.ref.toString()}`);
    return { transferred };
  }

  @method({ stateless: true })
  async add(args: { a: number; b: number }): Promise<number> {
    return args.a + args.b;
  }

  @getter("count")
  getCount(): number { return this.count; }

  @setter("count")
  setCount(value: number): void { this.count = value; }
}

export default Counter;
```

## Full Example: Stateless Greeting

```typescript
import { service, method, OaaSObject } from "@oaas/sdk";

@service("Greeting", { package: "example" })
class Greeting extends OaaSObject {
  @method({ stateless: true })
  async greet(name: string): Promise<string> {
    return `Hello, ${name}! Welcome to OaaS.`;
  }

  @method({ stateless: true })
  async farewell(name: string): Promise<string> {
    return `Goodbye, ${name}! See you next time.`;
  }

  @method({ stateless: true })
  async greetLocalized(args: { name: string; language?: string }): Promise<string> {
    const lang = args.language ?? "en";
    switch (lang) {
      case "en": return `Hello, ${args.name}!`;
      case "es": return `¡Hola, ${args.name}!`;
      case "fr": return `Bonjour, ${args.name}!`;
      case "de": return `Hallo, ${args.name}!`;
      case "ja": return `こんにちは、${args.name}さん！`;
      default:   return `Hello, ${args.name}! (${lang})`;
    }
  }

  @method({ stateless: true })
  async echo(input: unknown): Promise<unknown> {
    return input;
  }
}

export default Greeting;
```

## Method Signatures

All `@method`-decorated methods follow this contract:

| Aspect | Rule |
|--------|------|
| **Arguments** | A single parameter, deserialized from the JSON invocation payload. Use a destructured object for multiple values. |
| **Return** | Any JSON-serializable value. `void` / `undefined` results in a null payload. |
| **Async** | Methods should be declared `async` / return `Promise<T>`. In the WASM context, async is resolved synchronously. |
| **Errors** | Throw `OaaSError` for application errors, any other Error for system errors. |
| **Side effects** | Mutate `this.*` fields freely — the SDK detects and persists changes. |

## API Reference Summary

| Export | Kind | Description |
|--------|------|-------------|
| `OaaSObject` | Class | Base class for all services |
| `ObjectRef` | Class | Immutable object identity |
| `ObjectProxy` | Class | Typed access to another object's fields/methods |
| `OaaSError` | Class | Application-level error |
| `isOaaSError(err)` | Function | Type guard for `OaaSError` |
| `@service(name, opts?)` | Decorator | Register class as OaaS service |
| `@method(opts?)` | Decorator | Mark method as invocable function |
| `@getter(field?)` | Decorator | Read accessor metadata |
| `@setter(field?)` | Decorator | Write accessor metadata |
| `registerService(cls)` | Function | Register class for dispatch (entry point) |
| `handleInvoke(...)` | Function | Dispatch an invocation (WIT entry point) |
| `extractPackageMetadata(cls)` | Function | Extract deployment descriptor |
| `packageMetadataToJson(cls)` | Function | Serialize descriptor to JSON |
| `headersToRecord(headers)` | Function | Convert `KeyValue[]` to plain object |
