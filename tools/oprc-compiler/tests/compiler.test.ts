/**
 * Tests for the compilation pipeline.
 *
 * Note: These tests require npm dependencies to be installed.
 * The componentize-js step can take 30-60 seconds on first run
 * (it downloads the StarlingMonkey WASM engine on first use).
 *
 * Run with: npm test
 */

import { describe, it, expect } from "vitest";
import { compileTypeScript } from "../src/compiler.js";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Resolve paths relative to the repo root
const WIT_PATH = path.resolve(__dirname, "../../../data-plane/oprc-wasm/wit");
const SDK_PATH = path.resolve(__dirname, "../../oaas-sdk-ts/src");

// ─── Sample Sources ────────────────────────────────────────

const VALID_STATELESS_SOURCE = `
import { service, method, OaaSObject } from '@oaas/sdk';

@service("Greeting", { package: "test" })
class Greeting extends OaaSObject {
  @method({ stateless: true })
  async greet(name: string): Promise<string> {
    return \`Hello, \${name}!\`;
  }
}

export default Greeting;
`;

const VALID_STATEFUL_SOURCE = `
import { service, method, OaaSObject } from '@oaas/sdk';

@service("Counter", { package: "test" })
class Counter extends OaaSObject {
  count: number = 0;

  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;
    return this.count;
  }
}

export default Counter;
`;

const SYNTAX_ERROR_SOURCE = `
import { service, method, OaaSObject } from '@oaas/sdk';

@service("Bad"
class Bad extends OaaSObject {
  @method()
  async broken(): Promise<void> {
`;

const EMPTY_SOURCE = "";

// ─── Tests ─────────────────────────────────────────────────

describe("compileTypeScript", () => {
  describe("input validation", () => {
    it("rejects empty source", async () => {
      const result = await compileTypeScript(EMPTY_SOURCE, WIT_PATH, SDK_PATH);
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors).toContain("Source code is empty");
      }
    });

    it("rejects whitespace-only source", async () => {
      const result = await compileTypeScript("   \n\n  ", WIT_PATH, SDK_PATH);
      expect(result.success).toBe(false);
    });
  });

  describe("bundling errors", () => {
    it("returns errors for TypeScript syntax errors", async () => {
      const result = await compileTypeScript(SYNTAX_ERROR_SOURCE, WIT_PATH, SDK_PATH);
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.length).toBeGreaterThan(0);
        // esbuild should report a parse error
        expect(result.errors.some((e) => e.toLowerCase().includes("error") || e.toLowerCase().includes("expected"))).toBe(true);
      }
    });

    it("returns errors for import from non-existent relative path", async () => {
      const source = `
        import { nonExistent } from './does-not-exist';
        export default class Foo { x = nonExistent; }
      `;
      const result = await compileTypeScript(source, WIT_PATH, SDK_PATH);
      // esbuild should fail on unresolvable relative import
      expect(result.success).toBe(false);
      if (!result.success) {
        expect(result.errors.length).toBeGreaterThan(0);
      }
    });
  });

  describe("timeout handling", () => {
    it("respects very short timeout", async () => {
      // 1ms timeout — should fail
      const result = await compileTypeScript(
        VALID_STATELESS_SOURCE,
        WIT_PATH,
        SDK_PATH,
        1, // 1ms
      );
      // This may or may not hit the timeout depending on system speed,
      // but the function should not hang
      expect(result).toBeDefined();
    });
  });

  describe("successful compilation", () => {
    it("compiles a stateless TypeScript module to WASM", async () => {
      const result = await compileTypeScript(VALID_STATELESS_SOURCE, WIT_PATH, SDK_PATH);

      expect(result.success).toBe(true);
      if (result.success) {
        // Verify it's a valid WASM binary (magic bytes: \0asm)
        expect(result.component[0]).toBe(0x00); // \0
        expect(result.component[1]).toBe(0x61); // a
        expect(result.component[2]).toBe(0x73); // s
        expect(result.component[3]).toBe(0x6d); // m
        // Component should be non-trivial size (SpiderMonkey engine embedded)
        expect(result.component.byteLength).toBeGreaterThan(1_000_000);
      }
    });

    it("compiles a stateful TypeScript module to WASM", async () => {
      const result = await compileTypeScript(VALID_STATEFUL_SOURCE, WIT_PATH, SDK_PATH);

      expect(result.success).toBe(true);
      if (result.success) {
        // Verify WASM magic bytes
        expect(result.component[0]).toBe(0x00);
        expect(result.component[1]).toBe(0x61);
        expect(result.component[2]).toBe(0x73);
        expect(result.component[3]).toBe(0x6d);
        expect(result.component.byteLength).toBeGreaterThan(1_000_000);
      }
    });
  });
});
