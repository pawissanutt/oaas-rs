import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    environment: "node",
    globals: true,
    // Compilation tests are slow (componentize-js downloads engine on first run)
    testTimeout: 180_000,
  },
});
