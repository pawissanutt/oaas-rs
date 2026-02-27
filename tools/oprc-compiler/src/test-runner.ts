/**
 * Test Runner — execute OaaS script methods in Node.js with mock infrastructure.
 *
 * Uses the SAME esbuild bundling pipeline as the compile endpoint to ensure
 * the user's code is transpiled and wired to the real @oaas/sdk. Instead of
 * componentizing to WASM, the bundle is loaded in Node.js with mock proxy
 * and context objects, giving behavior-consistent test results.
 *
 * Key guarantee: decorators, state management, and dispatch logic come from
 * the real SDK — not a reimplementation.
 */

import * as esbuild from "esbuild";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

// ─── Types ─────────────────────────────────────────────────

export interface TestRequest {
  /** TypeScript source code. */
  source: string;
  /** Method name to invoke. */
  methodName: string;
  /** JSON payload to pass as the method argument. */
  payload?: unknown;
  /** Initial state fields (key → JSON-serializable value). */
  initialState?: Record<string, unknown>;
}

export interface TestResult {
  success: boolean;
  /** Return value from the method. */
  result?: unknown;
  /** Updated state after method execution. */
  finalState?: Record<string, unknown>;
  /** Log output captured during execution. */
  logs: Array<{ level: string; message: string }>;
  /** Error message if the test failed. */
  error?: string;
  /** Execution time in milliseconds. */
  durationMs: number;
}

// ─── Test Entry Point Template ─────────────────────────────

/**
 * Generate the test entry point shim.
 *
 * This is similar to the compile entry point but instead of exporting a
 * WIT guest-object, it exports an async `testInvoke` function that:
 * - Uses the real SDK's registerService + discoverFields
 * - Uses the real SDK's loadFields / saveChangedFields for state
 * - Uses the real SDK's getMethodMetadata for stateless detection
 * - Uses the real SDK's OAAS_CONTEXT + ObjectProxy for context injection
 * - Adds `await` for async method support (unlike WASM where async is sync)
 */
function generateTestEntryPoint(): string {
  return `\
import UserClass from './user_module';
import {
  registerService,
  discoverFields, loadFields, saveChangedFields,
  getMethodMetadata, isOaaSError,
  ObjectProxy, OAAS_CONTEXT
} from '@oaas/sdk';

// Register using the real SDK (same as WASM path)
registerService(UserClass);

/**
 * Async-aware invocation using real SDK internals.
 * Mirrors handleInvoke from dispatch.ts but adds await for async methods.
 */
export async function testInvoke(selfProxy, functionName, payloadBytes, context) {
  // Discover fields (real SDK)
  const fields = discoverFields(UserClass);

  // Create instance (real OaaSObject subclass)
  const instance = new UserClass();

  // Set up context (real OAAS_CONTEXT symbol + real ObjectProxy)
  const proxy = new ObjectProxy(selfProxy);
  instance[OAAS_CONTEXT] = { selfProxy: proxy, hostContext: context };

  // Look up the method
  const methodFn = instance[functionName];
  if (typeof methodFn !== 'function') {
    throw new Error(\`Method "\${functionName}" not found on service class\`);
  }

  // Check decorator metadata (real SDK getMethodMetadata)
  const methodMeta = getMethodMetadata(Object.getPrototypeOf(instance));
  const meta = methodMeta.get(functionName);
  const isStateless = meta?.stateless ?? false;

  // Parse payload (same as handleInvoke)
  let arg = undefined;
  if (payloadBytes !== null && payloadBytes !== undefined && payloadBytes.length > 0) {
    arg = JSON.parse(new TextDecoder().decode(payloadBytes));
  }

  // State management: load fields before (real SDK)
  let snapshot = null;
  if (!isStateless) {
    snapshot = loadFields(instance, selfProxy, fields);
  }

  // Call the user's method WITH AWAIT (the only difference from WASM dispatch)
  const result = await methodFn.call(instance, arg);

  // State management: save changed fields after (real SDK)
  if (!isStateless && snapshot !== null) {
    saveChangedFields(instance, selfProxy, fields, snapshot);
  }

  return result;
}
`;
}

// ─── Mock Infrastructure ───────────────────────────────────

interface FieldEntry {
  key: string;
  value: Uint8Array;
}

/**
 * Create a mock HostObjectProxy backed by an in-memory Map.
 * Matches the HostObjectProxy interface from the SDK types.
 */
function createMockProxy(
  initialState: Record<string, unknown>,
): { proxy: Record<string, Function>; data: Map<string, Uint8Array> } {
  const encoder = new TextEncoder();
  const data = new Map<string, Uint8Array>();

  // Seed initial state
  for (const [key, value] of Object.entries(initialState)) {
    data.set(key, encoder.encode(JSON.stringify(value)));
  }

  const proxy = {
    ref: () => ({ cls: "TestClass", partitionId: 0, objectId: "test-obj-1" }),

    get: (key: string): Uint8Array | null => data.get(key) ?? null,

    getMany: (keys: string[]): FieldEntry[] => {
      const entries: FieldEntry[] = [];
      for (const key of keys) {
        const value = data.get(key);
        if (value !== undefined) entries.push({ key, value });
      }
      return entries;
    },

    set: (key: string, value: Uint8Array): void => {
      data.set(key, value);
    },

    setMany: (entries: FieldEntry[]): void => {
      for (const e of entries) data.set(e.key, e.value);
    },

    delete: (key: string): void => {
      data.delete(key);
    },

    getAll: () => ({
      entries: Array.from(data.entries()).map(([key, raw]) => ({
        key,
        value: { data: raw, valType: "byte" as const },
      })),
    }),

    setAll: (objData: { entries: Array<{ key: string; value: { data: Uint8Array } }> }): void => {
      data.clear();
      for (const entry of objData.entries) data.set(entry.key, entry.value.data);
    },

    invoke: (_fnName: string, _payload: Uint8Array | null): Uint8Array | null => null,
  };

  return { proxy, data };
}

/**
 * Create a mock HostObjectContext that collects log entries.
 */
function createMockContext(
  logs: Array<{ level: string; message: string }>,
) {
  return {
    object: () => {
      throw new Error("Cross-object access not supported in test mode");
    },
    objectByStr: () => {
      throw new Error("Cross-object access not supported in test mode");
    },
    log: (level: string, message: string) => {
      logs.push({ level, message });
    },
  };
}

// ─── Bundle + Execute ──────────────────────────────────────

/**
 * Test a script by bundling with esbuild (using the real SDK) and
 * executing in Node.js with mock infrastructure.
 */
export async function testScript(
  source: string,
  sdkPath: string,
  request: TestRequest,
): Promise<TestResult> {
  const startTime = Date.now();
  const logs: Array<{ level: string; message: string }> = [];

  if (!source || source.trim().length === 0) {
    return {
      success: false,
      logs,
      error: "Source code is empty",
      durationMs: Date.now() - startTime,
    };
  }

  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "oprc-test-"));

  try {
    // 1. Write user source
    const userModulePath = path.join(tmpDir, "user_module.ts");
    await fs.writeFile(userModulePath, source);

    // 2. Write test entry point
    const entryPath = path.join(tmpDir, "test_entry.ts");
    await fs.writeFile(entryPath, generateTestEntryPoint());

    // 3. Bundle with esbuild — same config as compile, but no WIT externals
    const bundlePath = path.join(tmpDir, "test_bundle.mjs");

    let buildResult: esbuild.BuildResult;
    try {
      buildResult = await esbuild.build({
        entryPoints: [entryPath],
        outfile: bundlePath,
        bundle: true,
        format: "esm",
        target: "es2022",
        platform: "neutral",
        // Resolve @oaas/sdk to the real SDK source (same as compile)
        alias: {
          "@oaas/sdk": path.resolve(sdkPath, "index.ts"),
        },
        // Enable experimental decorators (same as compile)
        tsconfigRaw: JSON.stringify({
          compilerOptions: {
            experimentalDecorators: true,
          },
        }),
        logLevel: "silent",
      });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        success: false,
        logs,
        error: `Bundling failed: ${message}`,
        durationMs: Date.now() - startTime,
      };
    }

    if (buildResult.errors.length > 0) {
      return {
        success: false,
        logs,
        error: buildResult.errors.map((e) => e.text).join("\n"),
        durationMs: Date.now() - startTime,
      };
    }

    // 4. Dynamically import the bundle
    // Use file:// URL for ESM import
    const bundleUrl = `file://${bundlePath}`;
    const mod = await import(bundleUrl);

    // 5. Set up mock infrastructure
    const { proxy, data } = createMockProxy(request.initialState ?? {});
    const context = createMockContext(logs);

    // 6. Encode payload
    const encoder = new TextEncoder();
    const decoder = new TextDecoder();
    let payloadBytes: Uint8Array | null = null;
    if (request.payload !== undefined) {
      payloadBytes = encoder.encode(JSON.stringify(request.payload));
    }

    // 7. Execute the test invocation
    const result = await mod.testInvoke(
      proxy,
      request.methodName,
      payloadBytes,
      context,
    );

    // 8. Extract final state from proxy data
    const finalState: Record<string, unknown> = {};
    for (const [key, value] of data.entries()) {
      try {
        finalState[key] = JSON.parse(decoder.decode(value));
      } catch {
        finalState[key] = `<binary: ${value.length} bytes>`;
      }
    }

    return {
      success: true,
      result,
      finalState,
      logs,
      durationMs: Date.now() - startTime,
    };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      success: false,
      logs,
      error: message,
      durationMs: Date.now() - startTime,
    };
  } finally {
    // Clean up temp directory
    await fs.rm(tmpDir, { recursive: true, force: true }).catch(() => {});
  }
}
