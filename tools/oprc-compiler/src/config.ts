/**
 * Environment configuration for the compiler service.
 */

export interface CompilerConfig {
  /** HTTP port to listen on. */
  port: number;
  /** Host to bind to. */
  host: string;
  /** Path to the WIT directory containing oaas.wit. */
  witPath: string;
  /** Path to the @oaas/sdk TypeScript source directory. */
  sdkPath: string;
  /** Maximum source code size in bytes (OOM protection). */
  maxSourceSize: number;
  /** Compilation timeout in milliseconds. */
  compileTimeoutMs: number;
}

export function loadConfig(): CompilerConfig {
  return {
    port: parseInt(process.env.PORT ?? "3000", 10),
    host: process.env.HOST ?? "0.0.0.0",
    witPath: process.env.WIT_PATH ?? resolveDefault("../../data-plane/oprc-wasm/wit"),
    sdkPath: process.env.SDK_PATH ?? resolveDefault("../oaas-sdk-ts/src"),
    maxSourceSize: parseInt(process.env.MAX_SOURCE_SIZE ?? String(1024 * 1024), 10), // 1 MB
    compileTimeoutMs: parseInt(process.env.COMPILE_TIMEOUT_MS ?? "120000", 10), // 2 minutes
  };
}

function resolveDefault(relativePath: string): string {
  const url = new URL(relativePath, import.meta.url);
  return url.pathname;
}
