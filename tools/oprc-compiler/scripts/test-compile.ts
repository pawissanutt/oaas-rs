/**
 * Manual test script — compile a sample TypeScript function and verify the output.
 *
 * Usage:
 *   npx tsx scripts/test-compile.ts
 *
 * This script:
 * 1. Compiles the Counter example through the full pipeline
 * 2. Verifies the output is a valid WASM Component
 * 3. Writes it to a file for further inspection (e.g., wasmtime loading)
 */

import { compileTypeScript } from "../src/compiler.js";
import * as path from "node:path";
import * as fs from "node:fs/promises";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WIT_PATH = path.resolve(__dirname, "../../../data-plane/oprc-wasm/wit");
const SDK_PATH = path.resolve(__dirname, "../../oaas-sdk-ts/src");

const COUNTER_SOURCE = `\
import { service, method, getter, setter, OaaSObject, OaaSError } from '@oaas/sdk';

@service("Counter", { package: "example" })
class Counter extends OaaSObject {
  count: number = 0;
  history: string[] = [];

  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;
    this.history.push(\`Added \${amount}\`);
    this.log("info", \`Counter incremented by \${amount} → \${this.count}\`);
    return this.count;
  }

  @method()
  async decrement(amount: number = 1): Promise<number> {
    if (this.count - amount < 0) {
      throw new OaaSError(\`Cannot decrement by \${amount}: count is \${this.count}\`);
    }
    this.count -= amount;
    this.history.push(\`Removed \${amount}\`);
    return this.count;
  }

  @method()
  async reset(): Promise<void> {
    this.count = 0;
    this.history = [];
    this.log("info", "Counter reset");
  }

  @method({ stateless: true })
  async add(args: { a: number; b: number }): Promise<number> {
    return args.a + args.b;
  }

  @getter("count")
  getCount(): number { return this.count; }

  @setter("count")
  setCount(value: number): void { this.count = value; }
}

export default Counter;
`;

async function main(): Promise<void> {
  console.log("=== OaaS Compiler Manual Test ===");
  console.log(`WIT path: ${WIT_PATH}`);
  console.log(`SDK path: ${SDK_PATH}`);
  console.log();

  console.log("Compiling Counter example...");
  const startTime = Date.now();

  const result = await compileTypeScript(COUNTER_SOURCE, WIT_PATH, SDK_PATH);

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

  if (!result.success) {
    console.error(`❌ Compilation failed after ${elapsed}s:`);
    for (const error of result.errors) {
      console.error(`  - ${error}`);
    }
    process.exit(1);
  }

  console.log(`✅ Compilation succeeded in ${elapsed}s`);
  console.log(`   WASM Component size: ${(result.component.byteLength / 1024 / 1024).toFixed(2)} MB`);

  // Verify WASM magic bytes
  const magic = result.component.slice(0, 4);
  const expectedMagic = new Uint8Array([0x00, 0x61, 0x73, 0x6d]);
  const magicOk = magic.every((b, i) => b === expectedMagic[i]);
  console.log(`   WASM magic bytes: ${magicOk ? "✅ valid" : "❌ invalid"}`);

  if (!magicOk) {
    process.exit(1);
  }

  // Write output for further inspection
  const outputPath = path.resolve(__dirname, "../test-output-counter.wasm");
  await fs.writeFile(outputPath, result.component);
  console.log(`   Written to: ${outputPath}`);
  console.log();
  console.log("To test wasmtime compatibility:");
  console.log(`  wasm-tools component wit ${outputPath}`);
  console.log();
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
