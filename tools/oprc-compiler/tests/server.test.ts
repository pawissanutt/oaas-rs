/**
 * Tests for the Fastify HTTP server.
 *
 * Uses Fastify's built-in injection for zero-network testing.
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createServer } from "../src/server.js";
import { type CompilerConfig } from "../src/config.js";
import type { FastifyInstance } from "fastify";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const testConfig: CompilerConfig = {
  port: 0, // not used in injection mode
  host: "127.0.0.1",
  witPath: path.resolve(__dirname, "../../../data-plane/oprc-wasm/wit"),
  sdkPath: path.resolve(__dirname, "../../oaas-sdk-ts/src"),
  maxSourceSize: 1024 * 1024,
  compileTimeoutMs: 120_000,
};

let server: FastifyInstance;

beforeAll(async () => {
  server = createServer(testConfig);
  await server.ready();
});

afterAll(async () => {
  await server.close();
});

describe("GET /health", () => {
  it("returns ok status", async () => {
    const response = await server.inject({
      method: "GET",
      url: "/health",
    });

    expect(response.statusCode).toBe(200);
    expect(response.json()).toEqual({ status: "ok" });
  });
});

describe("POST /compile", () => {
  it("rejects missing body", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      headers: { "content-type": "application/json" },
      payload: "{}",
    });

    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.success).toBe(false);
    expect(body.errors.length).toBeGreaterThan(0);
  });

  it("rejects missing source", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      payload: { language: "typescript" },
    });

    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.success).toBe(false);
  });

  it("rejects missing language", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      payload: { source: "const x = 1;" },
    });

    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.success).toBe(false);
  });

  it("rejects unsupported language", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      payload: { source: "print('hello')", language: "python" },
    });

    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.success).toBe(false);
    expect(body.errors[0]).toContain("python");
  });

  it("rejects empty source", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      payload: { source: "", language: "typescript" },
    });

    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.success).toBe(false);
    expect(body.errors).toContain("Source code is empty");
  });

  it("returns errors for syntax errors", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      payload: {
        source: `
          import { service } from '@oaas/sdk';
          @service("Bad"
          class Bad {}
        `,
        language: "typescript",
      },
    });

    expect(response.statusCode).toBe(400);
    const body = response.json();
    expect(body.success).toBe(false);
    expect(body.errors.length).toBeGreaterThan(0);
  });

  it("compiles valid TypeScript and returns WASM binary", async () => {
    const response = await server.inject({
      method: "POST",
      url: "/compile",
      payload: {
        source: `
          import { service, method, OaaSObject } from '@oaas/sdk';

          @service("TestService", { package: "test" })
          class TestService extends OaaSObject {
            @method({ stateless: true })
            async echo(input: unknown): Promise<unknown> {
              return input;
            }
          }

          export default TestService;
        `,
        language: "typescript",
      },
    });

    expect(response.statusCode).toBe(200);
    expect(response.headers["content-type"]).toBe("application/wasm");

    // Verify WASM magic bytes
    const body = response.rawPayload;
    expect(body[0]).toBe(0x00);
    expect(body[1]).toBe(0x61);
    expect(body[2]).toBe(0x73);
    expect(body[3]).toBe(0x6d);
    expect(body.byteLength).toBeGreaterThan(1_000_000);
  });
});
