/**
 * Fastify HTTP server — REST API for the compiler service.
 *
 * Endpoints:
 *   POST /compile  — compile TypeScript source → WASM Component binary
 *   POST /test     — test a script method using real SDK in Node.js
 *   GET  /health   — health check
 */

import Fastify, { type FastifyInstance } from "fastify";
import { compileTypeScript } from "./compiler.js";
import { testScript, type TestRequest } from "./test-runner.js";
import { type CompilerConfig } from "./config.js";

// ─── Request/Response Types ────────────────────────────────

interface CompileRequestBody {
  source: string;
  language: string;
}

interface CompileErrorResponse {
  success: false;
  errors: string[];
}

interface TestRequestBody {
  source: string;
  methodName: string;
  payload?: unknown;
  initialState?: Record<string, unknown>;
}

// ─── Server Factory ────────────────────────────────────────

export function createServer(config: CompilerConfig): FastifyInstance {
  const server = Fastify({
    logger: {
      level: process.env.LOG_LEVEL ?? "info",
    },
    bodyLimit: config.maxSourceSize,
  });

  // ── Health Check ─────────────────────────────────────────

  server.get("/health", async () => {
    return { status: "ok" };
  });

  // ── Compile Endpoint ─────────────────────────────────────

  server.post<{ Body: CompileRequestBody }>("/compile", async (request, reply) => {
    const { source, language } = request.body ?? {};

    // Validate request
    if (source === undefined || source === null || typeof source !== "string") {
      return reply.code(400).send({
        success: false,
        errors: ["Missing or invalid 'source' field (expected string)"],
      } satisfies CompileErrorResponse);
    }

    if (!language || typeof language !== "string") {
      return reply.code(400).send({
        success: false,
        errors: ["Missing or invalid 'language' field (expected string)"],
      } satisfies CompileErrorResponse);
    }

    if (language !== "typescript") {
      return reply.code(400).send({
        success: false,
        errors: [`Unsupported language: "${language}". Only "typescript" is supported.`],
      } satisfies CompileErrorResponse);
    }

    // Source size check (defense in depth — Fastify bodyLimit handles this too)
    if (Buffer.byteLength(source, "utf-8") > config.maxSourceSize) {
      return reply.code(413).send({
        success: false,
        errors: [`Source code exceeds maximum size of ${config.maxSourceSize} bytes`],
      } satisfies CompileErrorResponse);
    }

    // Compile
    request.log.info("Starting TypeScript compilation");
    const startTime = Date.now();

    const result = await compileTypeScript(
      source,
      config.witPath,
      config.sdkPath,
      config.compileTimeoutMs,
    );

    const elapsed = Date.now() - startTime;

    if (!result.success) {
      request.log.warn({ errors: result.errors, elapsed }, "Compilation failed");
      return reply.code(400).send({
        success: false,
        errors: result.errors,
      } satisfies CompileErrorResponse);
    }

    request.log.info(
      { sizeBytes: result.component.byteLength, elapsed },
      "Compilation succeeded",
    );

    // Return raw WASM bytes with application/wasm content type
    return reply
      .code(200)
      .type("application/wasm")
      .send(Buffer.from(result.component));
  });

  // ── Test Endpoint ────────────────────────────────────────

  server.post<{ Body: TestRequestBody }>("/test", async (request, reply) => {
    const { source, methodName, payload, initialState } = request.body ?? {};

    // Validate request
    if (!source || typeof source !== "string") {
      return reply.code(400).send({
        success: false,
        logs: [],
        error: "Missing or invalid 'source' field (expected string)",
      });
    }

    if (!methodName || typeof methodName !== "string") {
      return reply.code(400).send({
        success: false,
        logs: [],
        error: "Missing or invalid 'methodName' field (expected string)",
      });
    }

    request.log.info({ methodName }, "Starting script test");
    const testReq: TestRequest = {
      source,
      methodName,
      payload,
      initialState: initialState ?? {},
    };

    const result = await testScript(source, config.sdkPath, testReq);

    request.log.info(
      { success: result.success, durationMs: result.durationMs },
      "Script test completed",
    );

    return reply.code(200).send(result);
  });

  return server;
}
