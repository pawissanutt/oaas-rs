/**
 * oprc-compiler — OaaS Compiler Service
 *
 * Compiles TypeScript source into WASM Components for the OaaS runtime.
 * Runs as a standalone HTTP service (Fastify) called by the Package Manager.
 */

import { loadConfig } from "./config.js";
import { createServer } from "./server.js";

async function main(): Promise<void> {
  const config = loadConfig();
  const server = createServer(config);

  try {
    const address = await server.listen({ port: config.port, host: config.host });
    console.log(`oprc-compiler listening on ${address}`);
    console.log(`  WIT path: ${config.witPath}`);
    console.log(`  SDK path: ${config.sdkPath}`);
    console.log(`  Max source size: ${config.maxSourceSize} bytes`);
    console.log(`  Compile timeout: ${config.compileTimeoutMs}ms`);
  } catch (err) {
    console.error("Failed to start server:", err);
    process.exit(1);
  }
}

main();
