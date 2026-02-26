# User Guide — oprc-compiler

This guide explains how to write TypeScript services that can be compiled into WASM Components by the `oprc-compiler` service.

## Overview

The compiler accepts a **single TypeScript module** that exports a default class. This class is your OaaS service — its methods become invocable functions, and its fields become persistent state managed by the OaaS data plane.

The `@oaas/sdk` decorators and types are available automatically; you do not need to install or bundle the SDK yourself.

## Writing a Service

### Stateless Service

A stateless service has no persistent fields. It receives input via function parameters and returns output.

```typescript
import { OaaSService, OaaSFunction } from "@oaas/sdk";

@OaaSService()
export default class Greeter {
  @OaaSFunction()
  greet(name: string): string {
    return `Hello, ${name}!`;
  }
}
```

Key rules:
- The class **must** be the **default export**.
- Decorate the class with `@OaaSService()`.
- Decorate public methods with `@OaaSFunction()`.

### Stateful Service

A stateful service has fields that are automatically persisted between invocations. The OaaS data plane serializes and restores the object's state.

```typescript
import { OaaSService, OaaSFunction } from "@oaas/sdk";

@OaaSService()
export default class Counter {
  count: number = 0;

  @OaaSFunction()
  increment(): void {
    this.count++;
  }

  @OaaSFunction()
  decrement(): void {
    this.count--;
  }

  @OaaSFunction()
  add(amount: number): void {
    this.count += amount;
  }

  @OaaSFunction()
  reset(): void {
    this.count = 0;
  }

  @OaaSFunction()
  getCount(): number {
    return this.count;
  }
}
```

### Function Parameters and Return Values

Functions receive parameters that are deserialized from the invocation payload and can return values that are serialized back to the caller.

- **Parameters**: Passed as the invocation payload (JSON-deserialized).
- **Return values**: Serialized as the response payload.
- **Void functions**: Functions that return `void` or `undefined` produce no response payload.

### Available SDK Imports

The following imports from `@oaas/sdk` are available:

| Import          | Description                                |
|-----------------|--------------------------------------------|
| `OaaSService`   | Class decorator — marks a class as an OaaS service |
| `OaaSFunction`  | Method decorator — marks a method as invocable     |

## Compiling via the REST API

### Request

Send a `POST` request to `/compile` with your source code:

```bash
curl -X POST http://localhost:3000/compile \
  -H "Content-Type: application/json" \
  -d '{
    "source": "<your TypeScript source>",
    "language": "typescript"
  }' \
  --output service.wasm
```

### Success

On success, the response is the raw WASM Component binary:
- **Status**: `200 OK`
- **Content-Type**: `application/wasm`
- **Body**: Binary WASM Component (~11 MB)

### Errors

On failure, the response contains structured error information:
- **Status**: `400 Bad Request`
- **Content-Type**: `application/json`

```json
{
  "success": false,
  "errors": [
    "source.ts:5:10: ')' expected."
  ]
}
```

Common error scenarios:

| Error | Cause |
|-------|-------|
| `"Source code is empty"` | The `source` field is empty or whitespace-only |
| `source.ts:N:M: ...` | TypeScript syntax or type error at line N, column M |
| `"Bundling failed: ..."` | Import resolution failure (e.g., referencing non-existent modules) |
| `"WASM componentization failed: ..."` | The bundled code is invalid for the WIT world |
| `"Compilation timed out"` | Compilation exceeded the configured timeout |

## Constraints

- **Single module**: The compiler accepts one TypeScript file. All code must be in a single module.
- **Default export**: Your service class must be the default export.
- **No Node.js APIs**: The compiled code runs inside a WASM sandbox (SpiderMonkey). Node.js built-in modules (`fs`, `path`, `http`, etc.) are not available.
- **No external packages**: You cannot import npm packages. Only `@oaas/sdk` is available.
- **Source size limit**: The source code must be under 1 MB (configurable via `MAX_SOURCE_SIZE`).
- **Compilation timeout**: Each compilation has a 2-minute timeout by default (configurable via `COMPILE_TIMEOUT_MS`).

## Output

The compiled WASM Component:

- Conforms to the **WASM Component Model**.
- Targets the `oaas-object` WIT world.
- **Exports** `oaas:odgm/guest-object` — the `onInvoke` interface called by the runtime.
- **Imports** `oaas:odgm/object-context` — host functions for state access and logging.
- Is approximately **11 MB** due to the embedded SpiderMonkey JavaScript engine.
- Is compatible with **wasmtime 33+**.

## Example: Complete Workflow

1. Write your service:

```typescript
// calculator.ts
import { OaaSService, OaaSFunction } from "@oaas/sdk";

@OaaSService()
export default class Calculator {
  result: number = 0;

  @OaaSFunction()
  add(value: number): number {
    this.result += value;
    return this.result;
  }

  @OaaSFunction()
  multiply(value: number): number {
    this.result *= value;
    return this.result;
  }

  @OaaSFunction()
  clear(): void {
    this.result = 0;
  }
}
```

2. Compile it:

```bash
curl -X POST http://localhost:3000/compile \
  -H "Content-Type: application/json" \
  -d "{\"source\": \"$(cat calculator.ts | jq -Rs .| sed 's/^"//;s/"$//')\", \"language\": \"typescript\"}" \
  --output calculator.wasm
```

3. Deploy the resulting `calculator.wasm` to the OaaS runtime via the Package Manager.
