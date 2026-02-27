/**
 * Tests for the OaaS SDK type definitions and templates (src/lib/oaas-sdk-types.ts).
 *
 * Covers:
 * - Type definitions contain all required SDK symbols
 * - Templates are valid TypeScript syntax (contain expected patterns)  
 * - Template array has correct entries
 */
import { describe, it, expect } from "vitest";
import {
  OAAS_SDK_TYPE_DEFINITIONS,
  DEFAULT_SCRIPT_TEMPLATE,
  COUNTER_TEMPLATE,
  GREETING_TEMPLATE,
  SCRIPT_TEMPLATES,
} from "@/lib/oaas-sdk-types";

describe("OAAS_SDK_TYPE_DEFINITIONS", () => {
  it("declares the @oaas/sdk module", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain('declare module "@oaas/sdk"');
  });

  it("declares ObjectRef class with factory methods", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("class ObjectRef");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("static from(");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("static parse(");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("toString(): string");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("equals(other: ObjectRef): boolean");
  });

  it("declares ObjectProxy class with all methods", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("class ObjectProxy");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("get<T");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("getMany<T");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("set<T");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("setMany(");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("delete(key: string)");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("getAll()");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("invoke<T");
  });

  it("declares OaaSObject abstract class", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("abstract class OaaSObject");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("ref: ObjectRef");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("object(ref: ObjectRef | string): ObjectProxy");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain('log(level: "debug" | "info" | "warn" | "error"');
  });

  it("declares OaaSError class", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("class OaaSError extends Error");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("function isOaaSError");
  });

  it("declares all decorators", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("function service(name: string");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("function method(opts?: MethodOptions)");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("function getter(field?: string)");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("function setter(field?: string)");
  });

  it("declares ServiceOptions interface", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("interface ServiceOptions");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("package?: string");
  });

  it("declares MethodOptions interface", () => {
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("interface MethodOptions");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("stateless?: boolean");
    expect(OAAS_SDK_TYPE_DEFINITIONS).toContain("timeout?: number");
  });
});

describe("DEFAULT_SCRIPT_TEMPLATE", () => {
  it("imports from @oaas/sdk", () => {
    expect(DEFAULT_SCRIPT_TEMPLATE).toContain('from "@oaas/sdk"');
  });

  it("uses @service decorator", () => {
    expect(DEFAULT_SCRIPT_TEMPLATE).toContain("@service(");
  });

  it("uses @method decorator", () => {
    expect(DEFAULT_SCRIPT_TEMPLATE).toContain("@method()");
  });

  it("extends OaaSObject", () => {
    expect(DEFAULT_SCRIPT_TEMPLATE).toContain("extends OaaSObject");
  });

  it("has an export default", () => {
    expect(DEFAULT_SCRIPT_TEMPLATE).toContain("export default");
  });

  it("includes a stateless method example", () => {
    expect(DEFAULT_SCRIPT_TEMPLATE).toContain("stateless: true");
  });
});

describe("COUNTER_TEMPLATE", () => {
  it("defines a Counter class", () => {
    expect(COUNTER_TEMPLATE).toContain('@service("Counter"');
    expect(COUNTER_TEMPLATE).toContain("class Counter");
  });

  it("includes stateful increment method", () => {
    expect(COUNTER_TEMPLATE).toContain("increment");
    expect(COUNTER_TEMPLATE).toContain("this.count += amount");
  });

  it("includes OaaSError usage", () => {
    expect(COUNTER_TEMPLATE).toContain("OaaSError");
    expect(COUNTER_TEMPLATE).toContain("throw new OaaSError");
  });

  it("includes history field for mutation detection demo", () => {
    expect(COUNTER_TEMPLATE).toContain("history: string[]");
    expect(COUNTER_TEMPLATE).toContain("this.history.push");
  });
});

describe("GREETING_TEMPLATE", () => {
  it("defines a Greeting class", () => {
    expect(GREETING_TEMPLATE).toContain('@service("Greeting"');
    expect(GREETING_TEMPLATE).toContain("class Greeting");
  });

  it("has only stateless methods", () => {
    // Every @method in this template should have stateless: true
    const methodMatches = GREETING_TEMPLATE.match(/@method\([^)]*\)/g) ?? [];
    expect(methodMatches.length).toBeGreaterThan(0);
    for (const m of methodMatches) {
      expect(m).toContain("stateless: true");
    }
  });
});

describe("SCRIPT_TEMPLATES", () => {
  it("has three templates", () => {
    expect(SCRIPT_TEMPLATES).toHaveLength(3);
  });

  it("each template has name, description, and template fields", () => {
    for (const tmpl of SCRIPT_TEMPLATES) {
      expect(tmpl.name).toBeTruthy();
      expect(tmpl.description).toBeTruthy();
      expect(tmpl.template).toBeTruthy();
      expect(tmpl.template).toContain("@oaas/sdk");
    }
  });

  it("first template is the blank service", () => {
    expect(SCRIPT_TEMPLATES[0].name).toBe("Blank Service");
    expect(SCRIPT_TEMPLATES[0].template).toBe(DEFAULT_SCRIPT_TEMPLATE);
  });

  it("second template is the counter", () => {
    expect(SCRIPT_TEMPLATES[1].name).toBe("Counter");
    expect(SCRIPT_TEMPLATES[1].template).toBe(COUNTER_TEMPLATE);
  });

  it("third template is the greeting", () => {
    expect(SCRIPT_TEMPLATES[2].name).toBe("Greeting");
    expect(SCRIPT_TEMPLATES[2].template).toBe(GREETING_TEMPLATE);
  });
});
