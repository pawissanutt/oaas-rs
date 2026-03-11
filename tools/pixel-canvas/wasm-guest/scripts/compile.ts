/**
 * Standalone compilation script — compiles PixelCanvas TypeScript to WASM Component.
 *
 * Uses oprc-compiler's `compileTypeScript` directly (no server needed).
 *
 * Usage:
 *   npx tsx scripts/compile.ts
 */

import { compileTypeScript } from "oprc-compiler/src/compiler.js";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WIT_PATH = path.resolve(__dirname, "../../../../data-plane/oprc-wasm/wit");
const SDK_PATH = path.resolve(__dirname, "../../../oaas-sdk-ts/src");

async function main(): Promise<void> {
  const sourceFile = path.resolve(__dirname, "../src/canvas.ts");
  const source = await fs.readFile(sourceFile, "utf-8");

  console.log(`Source:   ${sourceFile}`);
  console.log(`WIT dir:  ${WIT_PATH}`);
  console.log(`SDK dir:  ${SDK_PATH}`);
  console.log();
  console.log("Compiling PixelCanvas...");

  const startTime = Date.now();
  const result = await compileTypeScript(source, WIT_PATH, SDK_PATH);
  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

  if (!result.success) {
    console.error(`Compilation failed after ${elapsed}s:`);
    for (const error of result.errors) {
      console.error(`  - ${error}`);
    }
    process.exit(1);
  }

  console.log(`Compilation succeeded in ${elapsed}s`);
  console.log(`  WASM Component size: ${(result.component.byteLength / 1024 / 1024).toFixed(2)} MB`);

  // Verify WASM magic bytes
  const magic = result.component.slice(0, 4);
  const expectedMagic = new Uint8Array([0x00, 0x61, 0x73, 0x6d]);
  const magicOk = magic.every((b, i) => b === expectedMagic[i]);
  console.log(`  WASM magic: ${magicOk ? "valid" : "INVALID"}`);

  if (!magicOk) {
    process.exit(1);
  }

  const outDir = path.resolve(__dirname, "../dist");
  await fs.mkdir(outDir, { recursive: true });
  const outPath = path.resolve(outDir, "pixel-canvas.wasm");
  await fs.writeFile(outPath, result.component);
  console.log(`  Output: ${outPath}`);
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
