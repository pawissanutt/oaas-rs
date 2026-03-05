/**
 * Script test runner — calls the compiler service to execute tests.
 *
 * Instead of in-browser transpilation with a reimplemented SDK shim,
 * this module sends the source code to the compiler service's /test endpoint.
 * The compiler bundles the code with the REAL @oaas/sdk via esbuild and
 * runs it in Node.js, ensuring behavior-consistent results with WASM deployment.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface TestConfig {
  /** The TypeScript source code of the script. */
  source: string;
  /** Method name to invoke. */
  methodName: string;
  /** JSON payload to pass to the method. */
  payload?: unknown;
  /** Initial state fields (key → JSON-serializable value). */
  initialState?: Record<string, unknown>;
}

export interface TestResult {
  success: boolean;
  /** Return value from the method (JSON-serializable). */
  result?: unknown;
  /** Updated state after method execution. */
  finalState?: Record<string, unknown>;
  /** Console/log output captured during execution. */
  logs: TestLog[];
  /** Error message if the test failed. */
  error?: string;
  /** Execution time in milliseconds. */
  durationMs: number;
}

export interface TestLog {
  level: "debug" | "info" | "warn" | "error";
  message: string;
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";
const API_V1 = `${API_BASE}/api/v1`;

/**
 * Run a test invocation of an OaaS script method via the compiler service.
 *
 * The compiler service bundles the code with the real @oaas/sdk and runs it
 * in Node.js with mock infrastructure, giving consistent behavior with WASM.
 */
export async function runTest(config: TestConfig): Promise<TestResult> {
  const start = performance.now();

  try {
    const res = await fetch(`${API_V1}/scripts/test`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        source: config.source,
        method_name: config.methodName,
        payload: config.payload,
        initial_state: config.initialState ?? {},
      }),
    });

    const data = await res.json();

    // Map camelCase response from compiler to our types
    return {
      success: data.success ?? false,
      result: data.result,
      finalState: data.finalState ?? data.final_state,
      logs: (data.logs ?? []).map((l: { level: string; message: string }) => ({
        level: l.level as TestLog["level"],
        message: l.message,
      })),
      error: data.error,
      durationMs: data.durationMs ?? data.duration_ms ?? (performance.now() - start),
    };
  } catch (e) {
    return {
      success: false,
      logs: [],
      error: e instanceof Error ? e.message : String(e),
      durationMs: performance.now() - start,
    };
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Extract available method names from TypeScript source using regex.
 * Used to populate the method dropdown in the test panel.
 */
export function extractMethodNames(source: string): string[] {
  const methods: string[] = [];
  const regex = /@method\([^)]*\)\s*\n\s*(?:async\s+)?(\w+)\s*\(/g;
  let match;
  while ((match = regex.exec(source)) !== null) {
    methods.push(match[1]);
  }
  return methods;
}
