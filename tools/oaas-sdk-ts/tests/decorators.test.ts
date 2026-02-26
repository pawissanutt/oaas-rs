/**
 * Decorator Tests — @service, @method, @getter, @setter.
 */

import { describe, it, expect } from "vitest";
import {
  service,
  method,
  getter,
  setter,
  getServiceMetadata,
  getMethodMetadata,
  getGetterMetadata,
  getSetterMetadata,
} from "../src/decorators.js";

describe("@service", () => {
  it("should store service metadata on the class", () => {
    @service("Counter", { package: "example" })
    class Counter {}

    const meta = getServiceMetadata(Counter);
    expect(meta).toBeDefined();
    expect(meta!.name).toBe("Counter");
    expect(meta!.package).toBe("example");
  });

  it("should work without package option", () => {
    @service("Simple")
    class Simple {}

    const meta = getServiceMetadata(Simple);
    expect(meta).toBeDefined();
    expect(meta!.name).toBe("Simple");
    expect(meta!.package).toBeUndefined();
  });

  it("should return undefined for undecorated class", () => {
    class Plain {}
    expect(getServiceMetadata(Plain)).toBeUndefined();
  });
});

describe("@method", () => {
  it("should register a method with default options", () => {
    class Svc {
      @method()
      increment() {}
    }

    const meta = getMethodMetadata(Svc.prototype);
    expect(meta.has("increment")).toBe(true);
    expect(meta.get("increment")!.name).toBe("increment");
    expect(meta.get("increment")!.stateless).toBe(false);
    expect(meta.get("increment")!.timeout).toBeUndefined();
  });

  it("should register a stateless method", () => {
    class Svc {
      @method({ stateless: true })
      compute() {}
    }

    const meta = getMethodMetadata(Svc.prototype);
    expect(meta.get("compute")!.stateless).toBe(true);
  });

  it("should register a method with timeout", () => {
    class Svc {
      @method({ timeout: 5000 })
      slow() {}
    }

    const meta = getMethodMetadata(Svc.prototype);
    expect(meta.get("slow")!.timeout).toBe(5000);
  });

  it("should register multiple methods on the same class", () => {
    class Svc {
      @method()
      methodA() {}

      @method({ stateless: true })
      methodB() {}

      @method({ timeout: 1000 })
      methodC() {}
    }

    const meta = getMethodMetadata(Svc.prototype);
    expect(meta.size).toBe(3);
    expect(meta.has("methodA")).toBe(true);
    expect(meta.has("methodB")).toBe(true);
    expect(meta.has("methodC")).toBe(true);
  });

  it("should return empty map for undecorated class", () => {
    class Plain {
      doStuff() {}
    }
    const meta = getMethodMetadata(Plain.prototype);
    expect(meta.size).toBe(0);
  });
});

describe("@getter", () => {
  it("should register a getter with explicit field", () => {
    class Svc {
      @getter("count")
      getCount() {
        return 0;
      }
    }

    const meta = getGetterMetadata(Svc.prototype);
    expect(meta.has("getCount")).toBe(true);
    expect(meta.get("getCount")!.field).toBe("count");
    expect(meta.get("getCount")!.methodName).toBe("getCount");
  });

  it("should default field to method name", () => {
    class Svc {
      @getter()
      total() {
        return 0;
      }
    }

    const meta = getGetterMetadata(Svc.prototype);
    expect(meta.get("total")!.field).toBe("total");
  });

  it("should return empty map for undecorated class", () => {
    class Plain {}
    const meta = getGetterMetadata(Plain.prototype);
    expect(meta.size).toBe(0);
  });
});

describe("@setter", () => {
  it("should register a setter with explicit field", () => {
    class Svc {
      @setter("count")
      setCount(_v: number) {}
    }

    const meta = getSetterMetadata(Svc.prototype);
    expect(meta.has("setCount")).toBe(true);
    expect(meta.get("setCount")!.field).toBe("count");
    expect(meta.get("setCount")!.methodName).toBe("setCount");
  });

  it("should default field to method name", () => {
    class Svc {
      @setter()
      total(_v: number) {}
    }

    const meta = getSetterMetadata(Svc.prototype);
    expect(meta.get("total")!.field).toBe("total");
  });

  it("should return empty map for undecorated class", () => {
    class Plain {}
    const meta = getSetterMetadata(Plain.prototype);
    expect(meta.size).toBe(0);
  });
});

describe("combined decorators", () => {
  it("should handle class with all decorators", () => {
    @service("FullService", { package: "test" })
    class FullService {
      @method()
      doWork() {}

      @method({ stateless: true })
      compute() {}

      @getter("score")
      getScore() {
        return 0;
      }

      @setter("score")
      setScore(_v: number) {}
    }

    const svcMeta = getServiceMetadata(FullService);
    expect(svcMeta!.name).toBe("FullService");
    expect(svcMeta!.package).toBe("test");

    const methods = getMethodMetadata(FullService.prototype);
    expect(methods.size).toBe(2);

    const getters = getGetterMetadata(FullService.prototype);
    expect(getters.size).toBe(1);

    const setters = getSetterMetadata(FullService.prototype);
    expect(setters.size).toBe(1);
  });
});
