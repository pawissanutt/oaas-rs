/**
 * Package Metadata Extraction Tests.
 */

import { describe, it, expect } from "vitest";
import { extractPackageMetadata, packageMetadataToJson } from "../src/metadata.js";
import { OaaSObject } from "../src/oaas_object.js";
import { service, method, getter, setter } from "../src/decorators.js";

// ─── Test Service Classes ──────────────────────────────────

@service("Counter", { package: "example" })
class Counter extends OaaSObject {
  count = 0;
  name = "default";

  @method()
  increment() {}

  @method({ stateless: true })
  compute() {}

  @getter("count")
  getCount() {
    return 0;
  }

  @setter("count")
  setCount(_v: number) {}
}

@service("Simple")
class SimpleService extends OaaSObject {
  @method()
  doWork() {}
}

class UndecoratedClass extends OaaSObject {}

// ─── Tests ─────────────────────────────────────────────────

describe("extractPackageMetadata", () => {
  it("should extract full metadata from a decorated class", () => {
    const meta = extractPackageMetadata(Counter);

    expect(meta).not.toBeNull();
    expect(meta!.name).toBe("example");
    expect(meta!.classes).toHaveLength(1);

    const cls = meta!.classes[0];
    expect(cls.name).toBe("Counter");
    expect(cls.stateFields).toEqual(["count", "name"]);
    expect(cls.functions).toHaveLength(2);

    const increment = cls.functions.find((f) => f.name === "increment")!;
    expect(increment.stateless).toBe(false);

    const compute = cls.functions.find((f) => f.name === "compute")!;
    expect(compute.stateless).toBe(true);

    expect(cls.getters).toHaveLength(1);
    expect(cls.getters[0]).toEqual({ method: "getCount", field: "count" });

    expect(cls.setters).toHaveLength(1);
    expect(cls.setters[0]).toEqual({ method: "setCount", field: "count" });
  });

  it("should use service name as package name when no package specified", () => {
    const meta = extractPackageMetadata(SimpleService);

    expect(meta).not.toBeNull();
    expect(meta!.name).toBe("Simple");
  });

  it("should return null for undecorated class", () => {
    const meta = extractPackageMetadata(UndecoratedClass);
    expect(meta).toBeNull();
  });

  it("should extract class with no state fields", () => {
    @service("Stateless", { package: "pkg" })
    class StatelessSvc extends OaaSObject {
      @method({ stateless: true })
      compute() {}
    }

    const meta = extractPackageMetadata(StatelessSvc);
    expect(meta!.classes[0].stateFields).toEqual([]);
    expect(meta!.classes[0].functions).toHaveLength(1);
  });

  it("should extract class with no methods", () => {
    @service("DataOnly", { package: "pkg" })
    class DataOnly extends OaaSObject {
      value = 42;
    }

    const meta = extractPackageMetadata(DataOnly);
    expect(meta!.classes[0].stateFields).toEqual(["value"]);
    expect(meta!.classes[0].functions).toEqual([]);
  });

  it("should include timeout in method metadata", () => {
    @service("WithTimeout", { package: "pkg" })
    class WithTimeout extends OaaSObject {
      @method({ timeout: 30000 })
      slowOp() {}
    }

    const meta = extractPackageMetadata(WithTimeout);
    expect(meta!.classes[0].functions[0].timeout).toBe(30000);
  });
});

describe("packageMetadataToJson", () => {
  it("should return pretty-printed JSON", () => {
    const json = packageMetadataToJson(Counter);
    expect(json).not.toBeNull();

    const parsed = JSON.parse(json!);
    expect(parsed.name).toBe("example");
    expect(parsed.classes).toHaveLength(1);
  });

  it("should return null for undecorated class", () => {
    const json = packageMetadataToJson(UndecoratedClass);
    expect(json).toBeNull();
  });

  it("should produce valid JSON that roundtrips", () => {
    const json = packageMetadataToJson(Counter);
    const parsed = JSON.parse(json!);
    const reparsed = JSON.parse(JSON.stringify(parsed));
    expect(reparsed).toEqual(parsed);
  });
});
