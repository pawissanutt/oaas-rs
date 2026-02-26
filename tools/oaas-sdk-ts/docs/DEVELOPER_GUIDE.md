# @oaas/sdk — Developer Guide

> Internal architecture, module design, and extension guide for SDK maintainers.

## Architecture Overview

The SDK sits between the **user's TypeScript class** and the **OaaS WASM host runtime**. Its role is to translate the OOP class model into the procedural WIT Component Model interface.

```
User TypeScript class
    │
    ├─ @service / @method / @getter / @setter  ← decorators store metadata
    │
    ▼
┌──────────────────────────────────────────────┐
│              @oaas/sdk                        │
│                                              │
│  ┌────────────┐  ┌────────────┐              │
│  │ decorators  │  │  metadata  │  ← compile-time metadata extraction  │
│  └─────┬──────┘  └─────┬──────┘              │
│        │               │                     │
│  ┌─────▼───────────────▼──────┐              │
│  │      dispatch shim          │  ← runtime invocation routing        │
│  └─────┬──────────────┬───────┘              │
│        │              │                      │
│  ┌─────▼──────┐ ┌─────▼──────┐              │
│  │   state     │ │ oaas_object │  ← field load/save + base class     │
│  └─────┬──────┘ └─────┬──────┘              │
│        │              │                      │
│  ┌─────▼──────────────▼──────┐              │
│  │  ObjectProxy / ObjectRef   │  ← host bridge types                 │
│  └────────────┬──────────────┘              │
│               │                              │
│  ┌────────────▼──────────────┐              │
│  │        types.ts            │  ← WIT mirror types                  │
│  └───────────────────────────┘              │
└──────────────────────────────────────────────┘
    │
    ▼
WIT Component Model (oaas:odgm world)
    │
    ▼
ODGM Host Runtime (Rust)
```

## Module Map

| Module | File | Responsibility |
|--------|------|----------------|
| **types** | `src/types.ts` | TypeScript mirrors of WIT types — zero logic, pure interfaces |
| **object_ref** | `src/object_ref.ts` | Immutable object identity value type with validation |
| **errors** | `src/errors.ts` | `OaaSError` class + cross-module type guard |
| **object_proxy** | `src/object_proxy.ts` | High-level wrapper around `HostObjectProxy` with JSON ser/de |
| **decorators** | `src/decorators.ts` | `@service`, `@method`, `@getter`, `@setter` + metadata storage |
| **oaas_object** | `src/oaas_object.ts` | Abstract base class providing `this.ref`, `this.object()`, `this.log()` |
| **state** | `src/state.ts` | Field discovery, load, snapshot, diff, and save |
| **dispatch** | `src/dispatch.ts` | `registerService()` + `handleInvoke()` — the WIT entry point |
| **metadata** | `src/metadata.ts` | Extract OPackage descriptor from decorator metadata |
| **index** | `src/index.ts` | Re-exports for the public API surface |

### Dependency graph (simplified)

```
index.ts ──► all modules (re-exports)

dispatch.ts ──► object_proxy, oaas_object, errors, decorators, state, types
metadata.ts ──► decorators, oaas_object, state
oaas_object.ts ──► object_ref, object_proxy, types
object_proxy.ts ──► object_ref, types
state.ts ──► types
object_ref.ts ──► types
decorators.ts ──► (standalone)
errors.ts ──► (standalone)
types.ts ──► (standalone)
```

## WIT Interface Mapping

The SDK implements the `oaas-object` world defined in `data-plane/oprc-wasm/wit/oaas.wit`:

### Imports (host → guest)

The host provides the `object-context` interface:

| WIT | SDK mapping |
|-----|-------------|
| `resource object-proxy` | `HostObjectProxy` interface → wrapped by `ObjectProxy` class |
| `object-proxy.ref()` | `ObjectProxy.ref` (cached, single host call) |
| `object-proxy.get(key)` | `ObjectProxy.get<T>(key)` (JSON deserialization) |
| `object-proxy.get-many(keys)` | `ObjectProxy.getMany(...keys)` |
| `object-proxy.set(key, value)` | `ObjectProxy.set(key, value)` (JSON serialization) |
| `object-proxy.set-many(entries)` | `ObjectProxy.setMany(entries)` |
| `object-proxy.delete(key)` | `ObjectProxy.delete(key)` |
| `object-proxy.get-all()` | `ObjectProxy.getAll()` |
| `object-proxy.invoke(fn, payload)` | `ObjectProxy.invoke(fn, payload)` |
| `object(ref)` | `OaaSObject.object(ref)` → creates `ObjectProxy` via `hostContext.object()` |
| `object-by-str(ref-str)` | `OaaSObject.object("cls/part/id")` → `hostContext.objectByStr()` |
| `log(level, message)` | `OaaSObject.log(level, message)` → `hostContext.log()` |

### Exports (guest → host)

| WIT | SDK mapping |
|-----|-------------|
| `guest-object.on-invoke(self, fn, payload, headers, ctx)` | `handleInvoke()` in `dispatch.ts` |

## Decorator Metadata Storage

Decorators use **Symbols** as property keys to store metadata, avoiding name collisions with user-defined properties.

### Storage locations

| Symbol | Stored on | Data type | Written by |
|--------|-----------|-----------|------------|
| `SERVICE_METADATA` | Class constructor | `ServiceMetadata` | `@service()` |
| `METHOD_METADATA` | Class prototype | `Map<string, MethodMetadata>` | `@method()` |
| `GETTER_METADATA` | Class prototype | `Map<string, GetterMetadata>` | `@getter()` |
| `SETTER_METADATA` | Class prototype | `Map<string, SetterMetadata>` | `@setter()` |

### Why Symbols?

- **No conflicts**: Symbols never collide with user property names or TypeScript built-ins.
- **Not enumerable**: Symbol-keyed properties are excluded from `Object.keys()`, `JSON.stringify()`, and field discovery.
- **Cross-module access**: The same Symbol instance is shared via module imports — no global registry pollution.

### Access pattern

```typescript
// Writing (inside decorator):
(target as any)[SERVICE_METADATA] = metadata;

// Reading (inside dispatch/metadata):
const meta = getServiceMetadata(cls);  // reads cls[SERVICE_METADATA]
```

### Type casting

Symbol-indexed properties require `(target as any)[SYMBOL]` casts because TypeScript's type system doesn't support arbitrary symbol indexing on unknown types. This is an intentional tradeoff for type safety at the API boundary while allowing internal flexibility.

## State Management Internals

The state module (`src/state.ts`) implements transparent field persistence with a **snapshot-diff** strategy.

### Field discovery: `discoverFields(cls)`

```typescript
function discoverFields(cls: new (...args: any[]) => object): string[]
```

1. Instantiates the class with `new cls()` (no arguments).
2. Calls `Object.keys(instance)` to get enumerable own properties.
3. Returns the list of field names.

**Edge cases:**
- If the constructor throws (e.g., requires arguments), returns `[]`. The service operates without auto-persisted fields.
- Only properties assigned in the constructor or as class field initializers are detected. Prototype methods, getters, and non-enumerable properties are excluded.
- Called once at `registerService()` time, result is cached.

### Field loading: `loadFields(instance, proxy, fields)`

```typescript
function loadFields(
  instance: Record<string, unknown>,
  proxy: HostObjectProxy,
  fields: string[]
): Map<string, string>
```

1. Calls `proxy.getMany(fields)` — single batch read from the host.
2. For each returned entry, deserializes from JSON and sets `instance[field] = value`.
3. For fields not returned by the host, leaves the class default value in place.
4. Creates a **snapshot** map: `field → JSON.stringify(value)` for every field (whether loaded or default).
5. Returns the snapshot.

**Why JSON for snapshots?** JSON serialization produces a canonical string representation that captures deep equality. Comparing `JSON.stringify(a) !== JSON.stringify(b)` detects changes to nested objects, arrays, and primitives uniformly.

### Change detection: `saveChangedFields(instance, proxy, fields, snapshot)`

```typescript
function saveChangedFields(
  instance: Record<string, unknown>,
  proxy: HostObjectProxy,
  fields: string[],
  snapshot: Map<string, string>
): void
```

1. For each field, serializes the current value: `JSON.stringify(instance[field])`.
2. Compares against the pre-call snapshot string.
3. Collects changed fields into a `FieldEntry[]`.
4. If any fields changed, calls `proxy.setMany(changed)` — single batch write.

**In-place mutation detection**: Because the snapshot is a JSON string (not a reference), mutating an array (`this.history.push(...)`) or object (`this.config.x = 1`) is correctly detected as a change. The new JSON string will differ from the snapshot string.

### State management flow

```
handleInvoke()
  │
  ├─ if stateful:
  │     loadFields(instance, proxy, fields)
  │       → proxy.getMany() → deserialize → set on this
  │       → snapshot = Map<field, jsonString>
  │
  ├─ call method
  │     → user code mutates this.* freely
  │
  └─ if stateful:
        saveChangedFields(instance, proxy, fields, snapshot)
          → serialize each field
          → compare with snapshot
          → proxy.setMany(changedOnly)
```

## Dispatch Shim Flow

`handleInvoke()` in `dispatch.ts` is the core runtime entry point — it implements the `guest-object.on-invoke` WIT export.

### Invocation lifecycle

```
on-invoke(selfProxy, functionName, payload, headers, hostContext)
  │
  ├─ 1. Get registered class + cached fields
  │     (throws if registerService() wasn't called)
  │
  ├─ 2. Create instance: new RegisteredClass()
  │
  ├─ 3. Set up OaaS context
  │     │  - Wrap selfProxy → ObjectProxy
  │     │  - Create OaaSObjectContext { selfProxy, hostContext }
  │     └  - Set instance[OAAS_CONTEXT] = ctx
  │
  ├─ 4. Look up method on instance
  │     │  - (instance as any)[functionName]
  │     └  - If not found → return invalid-request response
  │
  ├─ 5. Check @method metadata
  │     │  - Read METHOD_METADATA from prototype
  │     └  - Determine isStateless flag
  │
  ├─ 6. Parse payload (JSON)
  │     └  - If parse fails → return invalid-request response
  │
  ├─ 7. Load state (if stateful)
  │     └  - loadFields(instance, selfProxy, fields) → snapshot
  │
  ├─ 8. Call user method
  │     └  - result = method.call(instance, arg)
  │
  ├─ 9. Save state (if stateful)
  │     └  - saveChangedFields(instance, selfProxy, fields, snapshot)
  │
  └─ 10. Return response
        ├  - result → JSON → InvocationResponse { status: "okay" }
        └  - on error: OaaSError → "app-error", other → "system-error"
```

### Error handling matrix

| Error type | Response status | Payload |
|-----------|----------------|---------|
| No error | `okay` | JSON-serialized return value |
| `OaaSError` | `app-error` | `{ "error": "<message>" }` |
| Other `Error` | `system-error` | `{ "error": "<message>" }` |
| Method not found | `invalid-request` | `{ "error": "Method \"<name>\" not found..." }` |
| JSON parse failure | `invalid-request` | `{ "error": "Failed to parse payload..." }` |
| No class registered | `system-error` | `{ "error": "No OaaS service class registered..." }` |

### Instance lifecycle

A **new instance** is created for every invocation — there is no instance reuse between calls. This simplifies concurrency: each invocation gets isolated state, and the proxy handles persistence.

The `OAAS_CONTEXT` Symbol is set on the instance **before** any user code runs, ensuring `this.ref`, `this.object()`, and `this.log()` are available inside methods.

## ObjectProxy Internals

`ObjectProxy` wraps the host-provided `HostObjectProxy` resource handle.

### JSON serialization

All data flows through JSON:
- **Read**: `proxy.get(key)` → `Uint8Array` → `TextDecoder.decode()` → `JSON.parse()` → typed value
- **Write**: typed value → `JSON.stringify()` → `TextEncoder.encode()` → `proxy.set(key, Uint8Array)`

`TextEncoder`/`TextDecoder` are used (requires the `DOM` lib in tsconfig) because they're the standard API for UTF-8 string/byte conversion.

### Ref caching

The `ref` property calls `this._host.ref()` once and caches the result as an `ObjectRef`. Since object identity is immutable, this avoids repeated host calls.

### Raw byte access

`getRaw()` and `setRaw()` bypass JSON serialization for binary data or pre-serialized content.

## ObjectRef Immutability

`ObjectRef` instances are frozen with `Object.freeze(this)` in the constructor. This ensures:
- Fields (`cls`, `partitionId`, `objectId`) cannot be reassigned.
- No additional properties can be added.
- This holds at runtime, not just at compile time (TypeScript `readonly` only provides compile-time checks).

The constructor is `private` — instances are created through factory methods (`from()`, `parse()`, `fromData()`).

## Package Metadata Extraction

`extractPackageMetadata(cls)` in `metadata.ts` reads all decorator metadata from a class and produces an `OPackageDescriptor`:

```typescript
{
  name: "example",           // from @service opts.package
  classes: [{
    name: "Counter",         // from @service name
    stateFields: ["count", "history"],  // from discoverFields()
    functions: [
      { name: "increment", stateless: false },
      { name: "add", stateless: true }
    ],
    getters: [{ method: "getCount", field: "count" }],
    setters: [{ method: "setCount", field: "count" }]
  }]
}
```

This descriptor is used by the Package Manager to:
1. Register the OaaS class and its function bindings.
2. Determine which functions are stateless (affecting routing decisions).
3. Advertise getters/setters for direct field access.

`packageMetadataToJson(cls)` is a convenience wrapper that returns the JSON string.

## Test Infrastructure

### Test directory structure

```
tests/
├── helpers.ts              ← mock factories and utilities
├── object_ref.test.ts      ← 31 tests (validation, parsing, equality)
├── errors.test.ts          ← 15 tests (OaaSError, isOaaSError)
├── object_proxy.test.ts    ← 27 tests (get/set/invoke, JSON ser/de)
├── decorators.test.ts      ← 15 tests (all 4 decorators, metadata)
├── state.test.ts           ← 21 tests (discover, load, save, diff)
├── dispatch.test.ts        ← 21 tests (full invoke flow, error cases)
├── metadata.test.ts        ← 9 tests (extraction, JSON output)
└── integration.test.ts     ← 18 tests (end-to-end class scenarios)
```

### Mock infrastructure (`tests/helpers.ts`)

#### `createMockProxy(opts?)`

Creates an in-memory `HostObjectProxy` implementation for testing. Features:

- **In-memory store**: `Map<string, Uint8Array>` backing all get/set/delete operations.
- **Call tracking**: `getCalls`, `setCalls`, `deleteCalls`, `getManyCallCount`, `setManyCallCount` for verifying host interactions.
- **Configurable ref**: set via `opts.ref` (defaults to `TestClass/0/test-1`).
- **Initial data**: seed fields via `opts.initialData` (JSON-serialized on construction).
- **Invoke handler**: configurable via `invokeHandler` property — set a function to handle `invoke()` calls.

#### `createMockContext(opts?)`

Creates an in-memory `HostObjectContext` for testing. Features:

- **Proxy registry**: `opts.proxies` maps ref strings to mock proxies.
- **Log tracking**: `logCalls` records all `log(level, message)` invocations.
- **Error on missing proxy**: throws if `object()` is called with an unregistered ref.

#### Utility functions

- `jsonBytes(value)`: JSON-serialize a value to `Uint8Array`.
- `fromJsonBytes(bytes)`: Deserialize `Uint8Array` to a typed value.

### Test configuration (`vitest.config.ts`)

```typescript
{
  test: {
    include: ["tests/**/*.test.ts"],
    environment: "node",
    globals: true,
  },
  esbuild: {
    // Enable legacy/experimental decorators in the test transform.
    tsconfigRaw: {
      compilerOptions: {
        experimentalDecorators: true,
      },
    },
  },
}
```

The `esbuild.tsconfigRaw` override is critical — without it, vitest's esbuild transform doesn't recognize TypeScript experimental decorators.

### Test patterns

**Unit test pattern** (testing a single module):
```typescript
import { describe, it, expect } from "vitest";

describe("ObjectRef.from()", () => {
  it("creates a valid ref", () => {
    const ref = ObjectRef.from("MyClass", 0, "obj-1");
    expect(ref.cls).toBe("MyClass");
    expect(ref.partitionId).toBe(0);
    expect(ref.objectId).toBe("obj-1");
  });
});
```

**Integration test pattern** (full dispatch flow):
```typescript
it("handles stateful invocation end-to-end", () => {
  registerService(Counter);
  const proxy = createMockProxy({ initialData: { count: 5 } });
  const ctx = createMockContext();

  const response = handleInvoke(proxy, "increment", jsonBytes(3), [], ctx);

  expect(response.status).toBe("okay");
  expect(fromJsonBytes(response.payload!)).toBe(8);
  // Verify state was persisted
  expect(fromJsonBytes(proxy.store.get("count")!)).toBe(8);
});
```

## Build & Development

### Prerequisites

- Node.js ≥ 18
- npm ≥ 9

### Commands

| Command | Description |
|---------|-------------|
| `npm install` | Install dependencies |
| `npm run build` | Compile TypeScript → `dist/` |
| `npm test` | Run all 157 tests |
| `npm run test:watch` | Run tests in watch mode |
| `npm run test:coverage` | Run tests with V8 coverage |
| `npm run lint` | Lint `src/` with ESLint |
| `npm run clean` | Remove `dist/` |
| `npx tsc --noEmit` | Type-check without emitting |

### TypeScript configuration

Key settings in `tsconfig.json`:
- `target: "ES2020"` — supports modern JS features
- `module: "ESNext"` — native ES modules
- `lib: ["ES2020", "DOM"]` — `DOM` is needed for `TextEncoder`/`TextDecoder`
- `experimentalDecorators: true` — enables legacy decorator syntax
- `strict: true` — full type safety

### Adding a new module

1. Create `src/new_module.ts`.
2. Add exports to `src/index.ts`.
3. Create `tests/new_module.test.ts`.
4. Run `npm test` and `npx tsc --noEmit` to validate.

### Adding a new decorator

1. Define a new Symbol in `decorators.ts`: `export const NEW_METADATA = Symbol("oaas:new");`
2. Define the metadata interface (e.g., `NewMetadata`).
3. Add an accessor function (e.g., `getNewMetadata()`).
4. Implement the decorator function following the existing pattern.
5. Add metadata reading in `metadata.ts` (for `extractPackageMetadata`).
6. If the decorator affects dispatch behavior, update `dispatch.ts`.
7. Export from `index.ts`.

## Design Decisions

### Why experimental decorators (not TC39 Stage 3)?

The TC39 Stage 3 decorator proposal has different semantics and is not yet widely supported by bundlers in the WASM compilation pipeline (ComponentizeJS/esbuild). Experimental decorators are stable in TypeScript and match the Python SDK's usage patterns.

### Why synchronous dispatch with Promise-returning methods?

In the WASM Component Model, all host calls are synchronous (WIT functions are blocking). However, user-facing APIs return Promises for two reasons:
1. **Convention**: matches the Python SDK and typical async JS patterns.
2. **Future-proofing**: if the runtime eventually supports true async WASM, no API changes are needed.

In practice, `handleInvoke()` calls `methodFn.call(instance, arg)` directly — if it returns a Promise, it's resolved synchronously in the single-threaded WASM context.

### Why JSON diff for state management?

Alternatives considered:
- **Proxy/trap-based change tracking**: complex, fragile with arrays/nested objects, runtime overhead.
- **Dirty flag per field**: requires user discipline or code generation.
- **Always write all fields**: wastes bandwidth for objects with many fields.

JSON snapshot-diff is simple, correct for all value types (including nested mutations), and the serialization cost is negligible for typical field sizes.

### Why create a new instance per invocation?

Reusing instances would require clearing all fields between calls, managing shared mutable state, and handling errors that leave the instance in an inconsistent state. Creating a fresh instance is cheap and eliminates all concurrency issues.

## Extension Points

### Custom serialization

To support non-JSON serialization (e.g., MessagePack, Protobuf), modify `ObjectProxy` to accept a configurable serializer:

```typescript
interface Serializer {
  encode(value: unknown): Uint8Array;
  decode(bytes: Uint8Array): unknown;
}
```

The `state.ts` functions would also need to accept a serializer parameter.

### Method middleware

To add cross-cutting concerns (auth, metrics, rate limiting), insert middleware in `handleInvoke()` between steps 6 and 8:

```typescript
// Future: middleware hook
for (const mw of middlewares) {
  const result = mw(instance, functionName, arg, headers);
  if (result) return result; // short-circuit
}
```

### Multi-class packages

Currently `extractPackageMetadata()` handles a single class. To support packages with multiple classes, accumulate `OClassDescriptor[]` from multiple decorated classes:

```typescript
function extractMultiClassMetadata(
  classes: Array<new (...args: any[]) => OaaSObject>
): OPackageDescriptor { ... }
```
