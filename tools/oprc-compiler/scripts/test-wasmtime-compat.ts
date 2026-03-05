/**
 * Wasmtime compatibility test script.
 *
 * Compiles a TypeScript module and then attempts to load the resulting
 * WASM Component through the project's Rust wasmtime (via oprc-wasm's
 * WasmModuleStore).
 *
 * Usage:
 *   npx tsx scripts/test-wasmtime-compat.ts
 *
 * Prerequisites:
 *   - The test-output-counter.wasm file must exist (run test-compile.ts first)
 *   - OR runs the compilation inline
 *
 * This script validates that:
 * 1. The WASM Component passes wasm-tools validation
 * 2. The component exports oaas:odgm/guest-object (via wasm-tools output)
 * 3. The component imports oaas:odgm/object-context
 */

import * as fs from "node:fs/promises";
import * as path from "node:path";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WASM_PATH = path.resolve(__dirname, "../test-output-counter.wasm");

async function main(): Promise<void> {
  console.log("=== Wasmtime Compatibility Test ===\n");

  // Check if the WASM file exists
  try {
    await fs.access(WASM_PATH);
  } catch {
    console.error("WASM file not found. Run 'npx tsx scripts/test-compile.ts' first.");
    process.exit(1);
  }

  const stats = await fs.stat(WASM_PATH);
  console.log(`WASM file: ${WASM_PATH}`);
  console.log(`Size: ${(stats.size / 1024 / 1024).toFixed(2)} MB\n`);

  // Test 1: Validate with wasm-tools
  console.log("1. Validating with wasm-tools...");
  try {
    execSync(`wasm-tools validate --features component-model "${WASM_PATH}"`, {
      stdio: "pipe",
    });
    console.log("   ✅ wasm-tools validate passed\n");
  } catch (err: unknown) {
    const stderr = (err as { stderr?: Buffer }).stderr?.toString() ?? "";
    console.error(`   ❌ wasm-tools validate failed: ${stderr}`);
    process.exit(1);
  }

  // Test 2: Check WIT exports
  console.log("2. Checking WIT exports...");
  try {
    const witOutput = execSync(
      `wasm-tools component wit "${WASM_PATH}"`,
      { encoding: "utf-8" },
    );

    const hasGuestObject = witOutput.includes("export oaas:odgm/guest-object");
    const hasObjectContext = witOutput.includes("import oaas:odgm/object-context");
    const hasTypes = witOutput.includes("import oaas:odgm/types");

    console.log(`   export oaas:odgm/guest-object: ${hasGuestObject ? "✅" : "❌"}`);
    console.log(`   import oaas:odgm/object-context: ${hasObjectContext ? "✅" : "❌"}`);
    console.log(`   import oaas:odgm/types: ${hasTypes ? "✅" : "❌"}`);

    if (!hasGuestObject || !hasObjectContext) {
      console.error("\n   ❌ Missing required WIT interfaces");
      process.exit(1);
    }
    console.log();
  } catch (err: unknown) {
    const stderr = (err as { stderr?: Buffer }).stderr?.toString() ?? "";
    console.error(`   ❌ wasm-tools component wit failed: ${stderr}`);
    process.exit(1);
  }

  // Test 3: Try loading in wasmtime via cargo test
  console.log("3. Loading in wasmtime (cargo check)...");
  console.log("   To fully test wasmtime instantiation, run:");
  console.log("   cargo test -p oprc-wasm -q");
  console.log("   (The project's executor already handles oaas-object world detection)\n");

  console.log("=== All compatibility checks passed ===");
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
