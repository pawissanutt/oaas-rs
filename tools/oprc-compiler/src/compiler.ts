/**
 * Compilation Pipeline — TypeScript → JavaScript → WASM Component.
 *
 * Steps:
 * 1. Generate an entry point shim that wires the user's class to the WIT exports.
 * 2. Write user source + entry point to a temp directory.
 * 3. Use esbuild to transpile TS → JS and bundle all dependencies (SDK + user code)
 *    into a single file, keeping WIT imports (`oaas:odgm/object-context`) external.
 * 4. Call componentize-js to produce a WASM Component from the bundled JS + WIT.
 * 5. Return the component bytes or error messages.
 */

import { componentize } from "@bytecodealliance/componentize-js";
import * as esbuild from "esbuild";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

// ─── Result Types ──────────────────────────────────────────

export interface CompileSuccess {
  success: true;
  component: Uint8Array;
}

export interface CompileFailure {
  success: false;
  errors: string[];
}

export type CompileResult = CompileSuccess | CompileFailure;

// ─── Entry Point Template ──────────────────────────────────

/**
 * Generate the shim entry point that bridges the user's class to the
 * WIT `oaas-object` world exports.
 *
 * This shim:
 * - Imports WIT host functions from `oaas:odgm/object-context`
 * - Imports the user's default export class
 * - Registers it with the SDK dispatcher
 * - Exports the `guest-object` interface expected by the WIT world
 */
function generateEntryPoint(): string {
  return `\
// WIT host imports — resolved by ComponentizeJS at compile time
import { object, objectByStr, log } from 'oaas:odgm/object-context';

// User's service class (bundled by esbuild)
import UserClass from './user_module';

// SDK dispatch shim (bundled by esbuild)
import { registerService, handleInvoke } from '@oaas/sdk';

// Register the user's service at module init time
registerService(UserClass);

// Create a HostObjectContext from the WIT imports
const hostContext = { object, objectByStr, log };

// Export the guest-object interface for the oaas-object world
export const guestObject = {
  onInvoke(self, functionName, payload, headers) {
    return handleInvoke(self, functionName, payload, headers, hostContext);
  }
};
`;
}

// ─── Compilation Pipeline ──────────────────────────────────

/**
 * Compile TypeScript source code into a WASM Component.
 *
 * @param source - The user's TypeScript source code (single module).
 * @param witPath - Path to the WIT directory containing oaas.wit.
 * @param sdkPath - Path to the @oaas/sdk TypeScript source directory.
 * @param timeoutMs - Maximum compilation time in milliseconds.
 */
export async function compileTypeScript(
  source: string,
  witPath: string,
  sdkPath: string,
  timeoutMs: number = 120_000,
): Promise<CompileResult> {
  // Validate inputs
  if (!source || source.trim().length === 0) {
    return { success: false, errors: ["Source code is empty"] };
  }

  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "oprc-compile-"));

  try {
    // Set up abort controller for timeout
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), timeoutMs);

    try {
      const result = await compileInDir(tmpDir, source, witPath, sdkPath, controller.signal);
      return result;
    } finally {
      clearTimeout(timeout);
    }
  } finally {
    // Clean up temp directory
    await fs.rm(tmpDir, { recursive: true, force: true }).catch(() => {});
  }
}

async function compileInDir(
  tmpDir: string,
  source: string,
  witPath: string,
  sdkPath: string,
  signal: AbortSignal,
): Promise<CompileResult> {
  // Step 1: Write user source to temp file
  const userModulePath = path.join(tmpDir, "user_module.ts");
  await fs.writeFile(userModulePath, source);

  // Step 2: Write entry point shim
  const entryPath = path.join(tmpDir, "entry.ts");
  await fs.writeFile(entryPath, generateEntryPoint());

  if (signal.aborted) {
    return { success: false, errors: ["Compilation timed out"] };
  }

  // Step 3: Bundle with esbuild
  //   - Transpile TypeScript → JavaScript
  //   - Bundle SDK + user code into a single file
  //   - Keep WIT imports external (componentize-js resolves them)
  const bundlePath = path.join(tmpDir, "entry.js");

  let buildResult: esbuild.BuildResult;
  try {
    buildResult = await esbuild.build({
      entryPoints: [entryPath],
      outfile: bundlePath,
      bundle: true,
      format: "esm",
      target: "es2020",
      platform: "neutral",
      // WIT imports must stay as external — componentize-js resolves them
      external: ["oaas:odgm/*"],
      // Resolve @oaas/sdk to the SDK source
      alias: {
        "@oaas/sdk": path.resolve(sdkPath, "index.ts"),
      },
      // Enable experimental decorators for TypeScript
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
      errors: [`Bundling failed: ${message}`],
    };
  }

  // Check for esbuild errors
  if (buildResult.errors.length > 0) {
    return {
      success: false,
      errors: buildResult.errors.map((e) =>
        formatEsbuildError(e),
      ),
    };
  }

  if (signal.aborted) {
    return { success: false, errors: ["Compilation timed out"] };
  }

  // Step 4: Componentize — JS + WIT → WASM Component
  try {
    const { component } = await componentize({
      sourcePath: bundlePath,
      witPath: witPath,
      worldName: "oaas-object",
    });

    return { success: true, component };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      success: false,
      errors: [`WASM componentization failed: ${message}`],
    };
  }
}

/**
 * Format an esbuild error/warning message into a readable string.
 */
function formatEsbuildError(error: esbuild.Message): string {
  let msg = error.text;
  if (error.location) {
    const loc = error.location;
    // Map temp file names back to user-friendly names
    const file = loc.file?.replace(/.*user_module\.ts/, "source.ts")
      ?? "source.ts";
    msg = `${file}:${loc.line}:${loc.column}: ${msg}`;
  }
  return msg;
}
