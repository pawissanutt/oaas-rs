/**
 * Compile the TS counter guest to a WASM Component for benchmarking.
 *
 * Writes the output to target/wasm-ts-guest/counter.wasm so that the
 * Rust bench_wasm_runtime criterion benchmark can load it alongside the
 * Rust wasm-guest-echo component.
 *
 * Usage:
 *   npx tsx scripts/build-bench-guest.ts
 */

import { compileTypeScript } from "../src/compiler.js";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WORKSPACE_ROOT = path.resolve(__dirname, "../../..");
const WIT_PATH = path.resolve(WORKSPACE_ROOT, "data-plane/oprc-wasm/wit");
const SDK_PATH = path.resolve(WORKSPACE_ROOT, "tools/oaas-sdk-ts/src");
const GUEST_SOURCE = path.resolve(
  WORKSPACE_ROOT,
  "tests/wasm-guest-ts-counter/counter.ts",
);
const OUTPUT_DIR = path.resolve(WORKSPACE_ROOT, "target/wasm-ts-guest");
const OUTPUT_PATH = path.join(OUTPUT_DIR, "counter.wasm");

async function main(): Promise<void> {
  console.log("=== Building TS Counter WASM guest for benchmarks ===");
  console.log(`  WIT:    ${WIT_PATH}`);
  console.log(`  SDK:    ${SDK_PATH}`);
  console.log(`  Source: ${GUEST_SOURCE}`);

  const source = await fs.readFile(GUEST_SOURCE, "utf-8");

  console.log("Compiling...");
  const t0 = Date.now();
  const result = await compileTypeScript(source, WIT_PATH, SDK_PATH, 120_000, [
    "http",
  ]);
  const elapsed = ((Date.now() - t0) / 1000).toFixed(1);

  if (!result.success) {
    console.error(`Compilation failed after ${elapsed}s:`);
    for (const e of result.errors) console.error(`  - ${e}`);
    process.exit(1);
  }

  const sizeMB = (result.component.byteLength / 1024 / 1024).toFixed(2);
  console.log(`Compiled in ${elapsed}s  (${sizeMB} MB)`);

  await fs.mkdir(OUTPUT_DIR, { recursive: true });
  await fs.writeFile(OUTPUT_PATH, result.component);
  console.log(`Written to ${OUTPUT_PATH}`);
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
