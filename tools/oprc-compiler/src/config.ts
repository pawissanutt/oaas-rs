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
    maxSourceSize: Number(process.env.MAX_SOURCE_SIZE ?? 50 * 1024 * 1024), // 50 MB
    compileTimeoutMs: Number(process.env.COMPILE_TIMEOUT_MS ?? 120_000), // 2 minutes
  };
}

function resolveDefault(relativePath: string): string {
  const url = new URL(relativePath, import.meta.url);
  return url.pathname;
}
