/**
 * Tests for the scripts API module (src/lib/scripts-api.ts).
 *
 * Covers:
 * - compileScript: success and error responses
 * - deployScript: success and error responses, request body shape
 * - getScriptSource: success and 404 handling
 * - listScripts: extracts WASM functions from packages, handles empty/error
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  compileScript,
  deployScript,
  getScriptSource,
  listScripts,
} from "@/lib/scripts-api";

// ---------------------------------------------------------------------------
// Fetch mock
// ---------------------------------------------------------------------------

const originalFetch = globalThis.fetch;

function mockFetch(handler: (url: string, init?: RequestInit) => Promise<Response>) {
  globalThis.fetch = vi.fn(handler) as unknown as typeof fetch;
}

afterEach(() => {
  globalThis.fetch = originalFetch;
});

// ---------------------------------------------------------------------------
// compileScript
// ---------------------------------------------------------------------------

describe("compileScript", () => {
  it("returns success with wasm_size on successful compilation", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify({ success: true, wasm_size: 10240 }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    const result = await compileScript("const x = 1;", "typescript");
    expect(result.success).toBe(true);
    expect(result.wasm_size).toBe(10240);
    expect(result.errors).toEqual([]);
  });

  it("returns errors on compilation failure", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify({
          success: false,
          errors: ["Type error at line 5", "Cannot find module '@oaas/sdk'"],
        }),
        { status: 400, headers: { "Content-Type": "application/json" } },
      ),
    );

    const result = await compileScript("broken code");
    expect(result.success).toBe(false);
    expect(result.errors).toHaveLength(2);
    expect(result.errors[0]).toContain("Type error");
    expect(result.wasm_size).toBeUndefined();
  });

  it("sends correct request body", async () => {
    let capturedBody: unknown;
    mockFetch(async (_url, init) => {
      capturedBody = JSON.parse(init?.body as string);
      return new Response(JSON.stringify({ success: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });

    await compileScript("test source", "typescript");
    expect(capturedBody).toEqual({
      source: "test source",
      language: "typescript",
    });
  });

  it("defaults language to typescript", async () => {
    let capturedBody: unknown;
    mockFetch(async (_url, init) => {
      capturedBody = JSON.parse(init?.body as string);
      return new Response(JSON.stringify({ success: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });

    await compileScript("test source");
    expect((capturedBody as { language: string }).language).toBe("typescript");
  });

  it("handles missing fields gracefully", async () => {
    mockFetch(async () =>
      new Response(JSON.stringify({}), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    const result = await compileScript("source");
    expect(result.success).toBe(false);
    expect(result.errors).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// deployScript
// ---------------------------------------------------------------------------

describe("deployScript", () => {
  it("returns deployment details on success", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify({
          success: true,
          deployment_key: "example/Counter",
          artifact_id: "abc123",
          artifact_url: "http://pm:8080/api/v1/artifacts/abc123",
          message: "Deployed successfully",
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    const result = await deployScript({
      source: "class Counter {}",
      language: "typescript",
      package_name: "example",
      class_key: "Counter",
      function_bindings: [{ name: "increment", stateless: false }],
      target_envs: ["edge-1"],
    });

    expect(result.success).toBe(true);
    expect(result.deployment_key).toBe("example/Counter");
    expect(result.artifact_id).toBe("abc123");
    expect(result.artifact_url).toContain("abc123");
    expect(result.message).toBe("Deployed successfully");
  });

  it("returns errors on deployment failure", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify({
          success: false,
          errors: ["Compilation failed: syntax error"],
        }),
        { status: 400, headers: { "Content-Type": "application/json" } },
      ),
    );

    const result = await deployScript({
      source: "broken",
      language: "typescript",
      package_name: "test",
      class_key: "Test",
      function_bindings: [],
      target_envs: [],
    });

    expect(result.success).toBe(false);
    expect(result.errors).toHaveLength(1);
    expect(result.deployment_key).toBeUndefined();
  });

  it("sends complete request body with all fields", async () => {
    let capturedBody: unknown;
    mockFetch(async (_url, init) => {
      capturedBody = JSON.parse(init?.body as string);
      return new Response(JSON.stringify({ success: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });

    await deployScript({
      source: "src",
      language: "typescript",
      package_name: "pkg",
      class_key: "Cls",
      function_bindings: [
        { name: "fn1", stateless: false },
        { name: "fn2", stateless: true },
      ],
      target_envs: ["env-a", "env-b"],
    });

    expect(capturedBody).toEqual({
      source: "src",
      language: "typescript",
      package_name: "pkg",
      class_key: "Cls",
      function_bindings: [
        { name: "fn1", stateless: false },
        { name: "fn2", stateless: true },
      ],
      target_envs: ["env-a", "env-b"],
    });
  });
});

// ---------------------------------------------------------------------------
// getScriptSource
// ---------------------------------------------------------------------------

describe("getScriptSource", () => {
  it("returns source code for existing script", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify({
          package: "example",
          function: "Counter",
          source: "class Counter {}",
          language: "typescript",
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    const result = await getScriptSource("example", "Counter");
    expect(result.package).toBe("example");
    expect(result.function).toBe("Counter");
    expect(result.source).toBe("class Counter {}");
    expect(result.language).toBe("typescript");
  });

  it("throws on 404 not found", async () => {
    mockFetch(async () =>
      new Response("Not Found", { status: 404, statusText: "Not Found" }),
    );

    await expect(getScriptSource("nonexistent", "fn")).rejects.toThrow(
      "Failed to fetch script source",
    );
  });

  it("URL encodes package and function names", async () => {
    let capturedUrl = "";
    mockFetch(async (url) => {
      capturedUrl = url;
      return new Response(
        JSON.stringify({
          package: "my pkg",
          function: "fn/name",
          source: "code",
          language: "typescript",
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      );
    });

    await getScriptSource("my pkg", "fn/name");
    expect(capturedUrl).toContain("my%20pkg");
    expect(capturedUrl).toContain("fn%2Fname");
  });
});

// ---------------------------------------------------------------------------
// listScripts
// ---------------------------------------------------------------------------

describe("listScripts", () => {
  it("extracts WASM functions from packages", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify([
          {
            name: "pkg1",
            functions: [
              {
                key: "counter",
                function_type: "WASM",
                provision_config: { wasm_module_url: "http://..." },
              },
              {
                key: "builtin-fn",
                function_type: "BUILTIN",
                provision_config: null,
              },
            ],
          },
          {
            name: "pkg2",
            functions: [
              {
                key: "greeting",
                function_type: "CUSTOM",
                provision_config: { wasm_module_url: "http://..." },
              },
            ],
          },
        ]),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    const scripts = await listScripts();
    // "counter" is WASM type, "greeting" has wasm_module_url
    expect(scripts).toHaveLength(2);
    expect(scripts[0].key).toBe("counter");
    expect(scripts[0].package).toBe("pkg1");
    expect(scripts[1].key).toBe("greeting");
    expect(scripts[1].package).toBe("pkg2");
  });

  it("returns empty array when no packages exist", async () => {
    mockFetch(async () =>
      new Response(JSON.stringify([]), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );

    const scripts = await listScripts();
    expect(scripts).toEqual([]);
  });

  it("returns empty array on fetch error", async () => {
    mockFetch(async () => {
      throw new Error("Network error");
    });

    const scripts = await listScripts();
    expect(scripts).toEqual([]);
  });

  it("handles packages with no functions array", async () => {
    mockFetch(async () =>
      new Response(
        JSON.stringify([{ name: "empty-pkg" }]),
        { status: 200, headers: { "Content-Type": "application/json" } },
      ),
    );

    const scripts = await listScripts();
    expect(scripts).toEqual([]);
  });
});
