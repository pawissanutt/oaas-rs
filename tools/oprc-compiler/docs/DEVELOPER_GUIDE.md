# Developer Guide — oprc-compiler

This guide covers the internal architecture, compilation pipeline, and development workflow for contributors to the compiler service.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     oprc-compiler Service                       │
│                                                                 │
│  ┌──────────┐    ┌──────────────────────────────────────────┐   │
│  │  Fastify  │───▶│          Compilation Pipeline            │   │
│  │  Server   │    │                                          │   │
│  │           │    │  1. Generate entry shim                   │   │
│  │ POST      │    │  2. esbuild: TS → bundled JS             │   │
│  │ /compile  │    │  3. componentize-js: JS + WIT → WASM     │   │
│  │           │    │                                          │   │
│  │ GET       │    └──────────────────────────────────────────┘   │
│  │ /health   │                                                   │
│  └──────────┘                                                   │
│       ▲                                                         │
│       │ HTTP                                                    │
└───────┼─────────────────────────────────────────────────────────┘
        │
   Package Manager (oprc-pm)
```

### Module Structure

| Module              | Responsibility                                         |
|---------------------|--------------------------------------------------------|
| `src/index.ts`      | Entry point — loads config, starts server              |
| `src/config.ts`     | Environment configuration (`CompilerConfig` interface) |
| `src/server.ts`     | Fastify HTTP server with `/compile` and `/health`      |
| `src/compiler.ts`   | Core compilation pipeline (esbuild + componentize-js)  |

## Compilation Pipeline

The pipeline transforms user TypeScript into a WASM Component in 4 steps:

### Step 1: Entry Point Generation

`generateEntryPoint()` creates a shim module (`entry.ts`) that bridges the user's class to the WIT world:

```typescript
// Generated entry.ts (simplified)
import { object, objectByStr, log } from 'oaas:odgm/object-context';  // WIT imports
import UserClass from './user_module';                                   // User's code
import { registerService, handleInvoke } from '@oaas/sdk';

registerService(UserClass);

const hostContext = { object, objectByStr, log };

export const guestObject = {
  onInvoke(self, functionName, payload, headers) {
    return handleInvoke(self, functionName, payload, headers, hostContext);
  }
};
```

This shim:
- Imports WIT host functions (resolved by componentize-js at compile time)
- Registers the user's class with the SDK dispatcher
- Exports the `guest-object` interface required by the `oaas-object` WIT world

### Step 2: Temp Directory Setup

Both the user's source (`user_module.ts`) and the generated entry point (`entry.ts`) are written to a temporary directory. This directory is automatically cleaned up after compilation.

### Step 3: esbuild Bundling

esbuild transpiles and bundles everything into a single JavaScript file:

```
entry.ts ─┐
           ├─ esbuild ──▶ entry.js (single bundle, ESM)
user_module.ts ─┘
@oaas/sdk/ ─┘
```

Configuration:
- **Format**: ESM (`format: "esm"`)
- **Target**: ES2020 (`target: "es2020"`)
- **Platform**: Neutral (`platform: "neutral"`)
- **External**: `oaas:odgm/*` — WIT imports are kept as bare imports for componentize-js
- **Alias**: `@oaas/sdk` → SDK source directory (resolved at config time)
- **Decorators**: `experimentalDecorators: true` via inline tsconfig

esbuild handles TypeScript transpilation and tree-shaking in a single pass (~10ms for typical services).

### Step 4: componentize-js

The bundled JavaScript is compiled into a WASM Component:

```
entry.js ──┐
            ├─ componentize-js ──▶ component.wasm (~11 MB)
oaas.wit ──┘
```

componentize-js:
- Embeds the **SpiderMonkey** JavaScript engine into the WASM module
- Resolves the WIT world imports/exports (validates the module conforms to `oaas-object`)
- Produces a valid WASM Component (~11 MB, mostly SpiderMonkey)
- Takes ~2-3 seconds for typical services

### Error Handling

Each stage can produce errors that are returned to the caller:

| Stage             | Error Type                  | Example                              |
|-------------------|-----------------------------|--------------------------------------|
| Validation        | Empty source                | `"Source code is empty"`             |
| esbuild           | Syntax / import errors      | `"source.ts:5:10: ')' expected"`     |
| componentize-js   | WIT conformance errors      | `"WASM componentization failed: ..."` |
| Timeout           | AbortController             | `"Compilation timed out"`            |

The `formatEsbuildError()` function remaps temp file paths (e.g., `/tmp/oprc-compile-xxx/user_module.ts`) back to user-friendly names (`source.ts`).

## Development Workflow

### Setup

```bash
cd tools/oprc-compiler
npm install
```

### Run in Development

```bash
npm run dev
```

This uses `ts-node` to run TypeScript directly without a build step.

### Build

```bash
npm run build    # TypeScript → dist/
npm start        # Run the built version
```

### Run Tests

```bash
npm test              # Run all tests
npm run test:watch    # Watch mode
npm run test:compile  # Manual compilation test (outputs WASM file)
```

### Test Structure

Tests use **vitest** and are organized in `tests/`:

| File                  | Tests                                              |
|-----------------------|----------------------------------------------------|
| `compiler.test.ts`    | 7 unit tests for the compilation pipeline          |
| `server.test.ts`      | 8 integration tests for the HTTP API               |

**Compiler tests** (`compiler.test.ts`):
- Empty source → error
- Whitespace-only source → error
- Syntax errors → error with location
- Missing relative imports → bundling error
- Short timeout → timeout error
- Stateless compilation → valid WASM output
- Stateful compilation → valid WASM output

**Server tests** (`server.test.ts`):
- Health check → `{ status: "ok" }`
- Missing body → 400
- Missing source → 400
- Missing language → 400
- Unsupported language → 400
- Empty source → 400
- Syntax errors → 400 with error details
- Successful compilation → 200 with WASM binary

Server tests use Fastify's `.inject()` for in-process HTTP testing (no actual network).

### Manual Testing Scripts

Two scripts in `scripts/` provide hands-on validation:

**`test-compile.ts`** — Compiles a Counter example and writes the WASM to disk:
```bash
npm run test:compile
# Outputs: test-output-counter.wasm (~11 MB)
```

**`test-wasmtime-compat.ts`** — Validates the output WASM with `wasm-tools`:
```bash
node --loader ts-node/esm scripts/test-wasmtime-compat.ts
# Validates with wasm-tools, checks WIT exports/imports
```

## Docker

### Build

The Dockerfile uses a multi-stage build. Build context is the **repository root**:

```bash
docker build -f tools/oprc-compiler/Dockerfile -t oprc-compiler .
```

### Stages

1. **Builder** — Installs all dependencies, runs `tsc` to compile TypeScript.
2. **Production** — Installs production dependencies only, copies compiled JS, WIT files, and SDK source.

The SDK source is copied into the image at `/app/sdk-src/` because it is needed at runtime (esbuild bundles it into each user's compilation).

### Environment in Docker

| Variable             | Docker Default       |
|----------------------|----------------------|
| `PORT`               | `3000`               |
| `HOST`               | `0.0.0.0`            |
| `WIT_PATH`           | `/app/wit`           |
| `SDK_PATH`           | `/app/sdk-src`       |
| `MAX_SOURCE_SIZE`    | `1048576`            |
| `COMPILE_TIMEOUT_MS` | `120000`             |
| `NODE_OPTIONS`       | `--max-old-space-size=1024` |

## Key Dependencies

| Package                              | Purpose                                       |
|--------------------------------------|-----------------------------------------------|
| `@bytecodealliance/componentize-js`  | JS + WIT → WASM Component (SpiderMonkey embed) |
| `@bytecodealliance/jco`              | WASM Component toolchain (transitive dep)      |
| `esbuild`                            | TypeScript transpilation + bundling            |
| `fastify`                            | HTTP server framework                          |

### Why esbuild?

esbuild provides sub-second TypeScript transpilation and bundling. Unlike `tsc`, it can bundle multiple modules into a single file and resolve path aliases — both required for producing the single JS file that componentize-js expects.

### Why componentize-js?

componentize-js is the official Bytecode Alliance tool for producing WASM Components from JavaScript. It embeds SpiderMonkey (Mozilla's JS engine) into the WASM module, providing a complete JS runtime inside the sandbox.

## Adding New Features

### Supporting a New Language

1. Add a new compiler function in `src/compiler.ts` (e.g., `compileJavaScript()`).
2. Update the language check in `src/server.ts` to accept the new language string.
3. Add tests in `tests/compiler.test.ts` and `tests/server.test.ts`.

### Modifying the Entry Shim

The entry shim in `generateEntryPoint()` is the bridge between user code and the WIT world. If the WIT interface changes:

1. Update `generateEntryPoint()` in `src/compiler.ts`.
2. Update the SDK's `handleInvoke` if the function signature changes.
3. Run `npm run test:compile` to verify end-to-end.
4. Run `scripts/test-wasmtime-compat.ts` to validate WIT conformance.

### Reducing Output Size

The ~11 MB output is dominated by the SpiderMonkey engine embedded by componentize-js. This is a known trade-off. Future versions of componentize-js may support alternative, lighter engines.
