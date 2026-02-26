/**
 * Integration Tests — full end-to-end scenarios combining all SDK components.
 *
 * These tests simulate realistic usage patterns including:
 * - Stateful counter with multiple operations
 * - Stateless compute functions
 * - Cross-object references
 * - Error scenarios
 * - State persistence across invocations
 */

import { describe, it, expect, beforeEach } from "vitest";
import { registerService, handleInvoke } from "../src/dispatch.js";
import { OaaSObject } from "../src/oaas_object.js";
import { OaaSError } from "../src/errors.js";
import { ObjectRef } from "../src/object_ref.js";
import { ObjectProxy as ObjectProxyClass } from "../src/object_proxy.js";
import { service, method, getter, setter } from "../src/decorators.js";
import { extractPackageMetadata } from "../src/metadata.js";
import { createMockProxy, createMockContext, jsonBytes, fromJsonBytes } from "./helpers.js";

const encoder = new TextEncoder();

// ─── Realistic Service Classes ─────────────────────────────

@service("Counter", { package: "example" })
class Counter extends OaaSObject {
  count = 0;
  history: string[] = [];
  lastModified = "";

  @method()
  increment(amount?: number) {
    const inc = amount ?? 1;
    this.count += inc;
    this.history.push(`Added ${inc}`);
    this.lastModified = "now";
    return this.count;
  }

  @method()
  decrement(amount?: number) {
    const dec = amount ?? 1;
    this.count -= dec;
    this.history.push(`Removed ${dec}`);
    this.lastModified = "now";
    return this.count;
  }

  @method()
  getState() {
    return {
      count: this.count,
      history: this.history,
      ref: this.ref.toString(),
    };
  }

  @method({ stateless: true })
  add(args: { a: number; b: number }) {
    return args.a + args.b;
  }

  @method()
  transfer(targetRefStr: string) {
    const target = this.object(targetRefStr);
    // In the real system, target.invoke would transfer the value.
    // Here we just verify cross-object access works.
    this.log("info", `Transferring ${this.count} to ${target.ref.toString()}`);
    const amount = this.count;
    this.count = 0;
    this.history.push(`Transferred ${amount} to ${targetRefStr}`);
    return { transferred: amount, target: target.ref.toString() };
  }

  @method()
  failIfNegative() {
    if (this.count < 0) {
      throw new OaaSError("Counter cannot be negative");
    }
    return this.count;
  }

  @getter("count")
  getCount() {
    return this.count;
  }

  @setter("count")
  setCount(v: number) {
    this.count = v;
  }
}

@service("UserProfile", { package: "social" })
class UserProfile extends OaaSObject {
  name = "";
  email = "";
  followers: string[] = [];
  settings = { theme: "light", notifications: true };

  @method()
  updateProfile(data: { name?: string; email?: string }) {
    if (data.name) this.name = data.name;
    if (data.email) this.email = data.email;
    return { name: this.name, email: this.email };
  }

  @method()
  addFollower(followerId: string) {
    if (this.followers.includes(followerId)) {
      throw new OaaSError(`Already following: ${followerId}`);
    }
    this.followers.push(followerId);
    return this.followers.length;
  }

  @method()
  updateSettings(newSettings: Record<string, unknown>) {
    this.settings = { ...this.settings, ...newSettings } as typeof this.settings;
    return this.settings;
  }

  @method({ stateless: true })
  validateEmail(email: string) {
    return email.includes("@");
  }
}

// ─── Tests ─────────────────────────────────────────────────

describe("Integration: Counter Service", () => {
  const REF = { cls: "Counter", partitionId: 0, objectId: "counter-1" };

  beforeEach(() => {
    registerService(Counter);
  });

  it("should handle a full lifecycle: create → increment → decrement → read", () => {
    const mock = createMockProxy({ ref: REF });
    const ctx = createMockContext();

    // First increment from zero
    let resp = handleInvoke(mock.proxy, "increment", jsonBytes(10), [], ctx.context);
    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toBe(10);

    // Second increment
    resp = handleInvoke(mock.proxy, "increment", jsonBytes(5), [], ctx.context);
    expect(fromJsonBytes(resp.payload!)).toBe(15);

    // Decrement
    resp = handleInvoke(mock.proxy, "decrement", jsonBytes(3), [], ctx.context);
    expect(fromJsonBytes(resp.payload!)).toBe(12);

    // Read full state
    resp = handleInvoke(mock.proxy, "getState", null, [], ctx.context);
    expect(resp.status).toBe("okay");
    const state = fromJsonBytes<{
      count: number;
      history: string[];
      ref: string;
    }>(resp.payload!);
    expect(state.count).toBe(12);
    expect(state.history).toEqual(["Added 10", "Added 5", "Removed 3"]);
    expect(state.ref).toBe("Counter/0/counter-1");

    // Verify underlying storage
    expect(fromJsonBytes(mock.data.get("count")!)).toBe(12);
    expect(fromJsonBytes(mock.data.get("history")!)).toHaveLength(3);
  });

  it("should support stateless method alongside stateful methods", () => {
    const mock = createMockProxy({ ref: REF });
    const ctx = createMockContext();

    // Stateless add — no state access
    const resp = handleInvoke(
      mock.proxy,
      "add",
      jsonBytes({ a: 100, b: 200 }),
      [],
      ctx.context
    );
    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toBe(300);

    // No state should have been read or written
    const getManyCalls = mock.calls.filter((c) => c.method === "getMany");
    expect(getManyCalls.length).toBe(0);
  });

  it("should handle cross-object transfer", () => {
    const mock = createMockProxy({ ref: REF });
    const targetRef = { cls: "Counter", partitionId: 1, objectId: "counter-2" };
    const targetMock = createMockProxy({ ref: targetRef });
    const ctx = createMockContext();
    ctx.proxies.set("Counter/1/counter-2", targetMock.proxy);

    // Set up initial count
    mock.data.set("count", jsonBytes(50));
    mock.data.set("history", jsonBytes([]));
    mock.data.set("lastModified", jsonBytes(""));

    // Transfer
    const resp = handleInvoke(
      mock.proxy,
      "transfer",
      jsonBytes("Counter/1/counter-2"),
      [],
      ctx.context
    );

    expect(resp.status).toBe("okay");
    const result = fromJsonBytes<{ transferred: number; target: string }>(
      resp.payload!
    );
    expect(result.transferred).toBe(50);
    expect(result.target).toBe("Counter/1/counter-2");

    // Count should be 0 after transfer
    expect(fromJsonBytes(mock.data.get("count")!)).toBe(0);

    // Log should show the transfer message
    expect(ctx.logs).toContainEqual({
      level: "info",
      message: "Transferring 50 to Counter/1/counter-2",
    });
  });

  it("should return app-error when business rule violated", () => {
    const mock = createMockProxy({ ref: REF });
    const ctx = createMockContext();

    // Set count to negative
    mock.data.set("count", jsonBytes(-5));
    mock.data.set("history", jsonBytes([]));
    mock.data.set("lastModified", jsonBytes(""));

    const resp = handleInvoke(
      mock.proxy,
      "failIfNegative",
      null,
      [],
      ctx.context
    );

    expect(resp.status).toBe("app-error");
    const payload = fromJsonBytes<{ error: string }>(resp.payload!);
    expect(payload.error).toBe("Counter cannot be negative");
  });

  it("should produce correct package metadata", () => {
    const meta = extractPackageMetadata(Counter);
    expect(meta).not.toBeNull();
    expect(meta!.name).toBe("example");

    const cls = meta!.classes[0];
    expect(cls.name).toBe("Counter");
    expect(cls.stateFields).toContain("count");
    expect(cls.stateFields).toContain("history");
    expect(cls.stateFields).toContain("lastModified");

    const methodNames = cls.functions.map((f) => f.name);
    expect(methodNames).toContain("increment");
    expect(methodNames).toContain("decrement");
    expect(methodNames).toContain("getState");
    expect(methodNames).toContain("add");
    expect(methodNames).toContain("transfer");
    expect(methodNames).toContain("failIfNegative");

    const addFn = cls.functions.find((f) => f.name === "add")!;
    expect(addFn.stateless).toBe(true);

    expect(cls.getters).toHaveLength(1);
    expect(cls.getters[0].field).toBe("count");
    expect(cls.setters).toHaveLength(1);
    expect(cls.setters[0].field).toBe("count");
  });
});

describe("Integration: UserProfile Service", () => {
  const REF = { cls: "UserProfile", partitionId: 0, objectId: "user-1" };

  beforeEach(() => {
    registerService(UserProfile);
  });

  it("should handle profile update with partial data", () => {
    const mock = createMockProxy({ ref: REF });
    const ctx = createMockContext();

    // Update only name
    let resp = handleInvoke(
      mock.proxy,
      "updateProfile",
      jsonBytes({ name: "Alice" }),
      [],
      ctx.context
    );
    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toEqual({ name: "Alice", email: "" });

    // Update email
    resp = handleInvoke(
      mock.proxy,
      "updateProfile",
      jsonBytes({ email: "alice@example.com" }),
      [],
      ctx.context
    );
    expect(fromJsonBytes(resp.payload!)).toEqual({
      name: "Alice",
      email: "alice@example.com",
    });
  });

  it("should handle array mutation (addFollower)", () => {
    const initial = new Map([
      ["name", jsonBytes("Alice")],
      ["email", jsonBytes("alice@test.com")],
      ["followers", jsonBytes(["bob"])],
      ["settings", jsonBytes({ theme: "light", notifications: true })],
    ]);
    const mock = createMockProxy({ ref: REF, initialData: initial });
    const ctx = createMockContext();

    const resp = handleInvoke(
      mock.proxy,
      "addFollower",
      jsonBytes("charlie"),
      [],
      ctx.context
    );

    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toBe(2);
    expect(fromJsonBytes(mock.data.get("followers")!)).toEqual([
      "bob",
      "charlie",
    ]);
  });

  it("should return app-error for duplicate follower", () => {
    const initial = new Map([
      ["name", jsonBytes("Alice")],
      ["email", jsonBytes("")],
      ["followers", jsonBytes(["bob"])],
      ["settings", jsonBytes({ theme: "light", notifications: true })],
    ]);
    const mock = createMockProxy({ ref: REF, initialData: initial });
    const ctx = createMockContext();

    const resp = handleInvoke(
      mock.proxy,
      "addFollower",
      jsonBytes("bob"),
      [],
      ctx.context
    );

    expect(resp.status).toBe("app-error");
    const payload = fromJsonBytes<{ error: string }>(resp.payload!);
    expect(payload.error).toContain("Already following: bob");
  });

  it("should merge nested settings object", () => {
    const initial = new Map([
      ["name", jsonBytes("")],
      ["email", jsonBytes("")],
      ["followers", jsonBytes([])],
      ["settings", jsonBytes({ theme: "light", notifications: true })],
    ]);
    const mock = createMockProxy({ ref: REF, initialData: initial });
    const ctx = createMockContext();

    const resp = handleInvoke(
      mock.proxy,
      "updateSettings",
      jsonBytes({ theme: "dark" }),
      [],
      ctx.context
    );

    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toEqual({
      theme: "dark",
      notifications: true,
    });
  });

  it("should handle stateless validation", () => {
    const mock = createMockProxy({ ref: REF });
    const ctx = createMockContext();

    let resp = handleInvoke(
      mock.proxy,
      "validateEmail",
      jsonBytes("valid@email.com"),
      [],
      ctx.context
    );
    expect(fromJsonBytes(resp.payload!)).toBe(true);

    resp = handleInvoke(
      mock.proxy,
      "validateEmail",
      jsonBytes("invalid"),
      [],
      ctx.context
    );
    expect(fromJsonBytes(resp.payload!)).toBe(false);
  });

  it("should produce correct package metadata", () => {
    const meta = extractPackageMetadata(UserProfile);
    expect(meta).not.toBeNull();
    expect(meta!.name).toBe("social");

    const cls = meta!.classes[0];
    expect(cls.name).toBe("UserProfile");
    expect(cls.stateFields).toEqual(["name", "email", "followers", "settings"]);

    const methodNames = cls.functions.map((f) => f.name);
    expect(methodNames).toContain("updateProfile");
    expect(methodNames).toContain("addFollower");
    expect(methodNames).toContain("updateSettings");
    expect(methodNames).toContain("validateEmail");

    const validateFn = cls.functions.find((f) => f.name === "validateEmail")!;
    expect(validateFn.stateless).toBe(true);
  });
});

describe("Integration: ObjectRef roundtrip", () => {
  it("should roundtrip through string and parse", () => {
    const ref = ObjectRef.from("MyClass", 42, "my-object");
    const str = ref.toString();
    const parsed = ObjectRef.parse(str);
    expect(parsed.equals(ref)).toBe(true);
  });

  it("should roundtrip through toData and fromData", () => {
    const ref = ObjectRef.from("SomeClass", 7, "abc-123");
    const data = ref.toData();
    const restored = ObjectRef.fromData(data);
    expect(restored.equals(ref)).toBe(true);
  });

  it("should work in proxy ref context", () => {
    const refData = { cls: "Test", partitionId: 3, objectId: "x" };
    const mock = createMockProxy({ ref: refData });
    const proxy = new ObjectProxyClass(mock.proxy);
    const ref = proxy.ref;
    expect(ref.toString()).toBe("Test/3/x");
    expect(ref.cls).toBe("Test");
    expect(ref.partitionId).toBe(3);
    expect(ref.objectId).toBe("x");
  });
});

describe("Integration: Edge cases", () => {
  it("should handle service with no state fields", () => {
    @service("Pure", { package: "math" })
    class PureService extends OaaSObject {
      @method({ stateless: true })
      square(n: number) {
        return n * n;
      }
    }

    registerService(PureService);
    const mock = createMockProxy({
      ref: { cls: "Pure", partitionId: 0, objectId: "p1" },
    });
    const ctx = createMockContext();

    const resp = handleInvoke(mock.proxy, "square", jsonBytes(7), [], ctx.context);
    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toBe(49);
  });

  it("should handle service returning complex nested data", () => {
    @service("Complex")
    class ComplexService extends OaaSObject {
      @method({ stateless: true })
      buildResponse() {
        return {
          users: [
            { id: 1, name: "Alice", tags: ["admin"] },
            { id: 2, name: "Bob", tags: ["user", "editor"] },
          ],
          metadata: { total: 2, page: 1 },
          flags: { active: true, archived: false },
        };
      }
    }

    registerService(ComplexService);
    const mock = createMockProxy({
      ref: { cls: "Complex", partitionId: 0, objectId: "c1" },
    });
    const ctx = createMockContext();

    const resp = handleInvoke(
      mock.proxy,
      "buildResponse",
      null,
      [],
      ctx.context
    );
    expect(resp.status).toBe("okay");
    const result = fromJsonBytes<any>(resp.payload!);
    expect(result.users).toHaveLength(2);
    expect(result.users[0].tags).toEqual(["admin"]);
    expect(result.metadata.total).toBe(2);
  });

  it("should handle method returning null", () => {
    @service("Nullable")
    class NullableService extends OaaSObject {
      @method({ stateless: true })
      findNothing() {
        return null;
      }
    }

    registerService(NullableService);
    const mock = createMockProxy({
      ref: { cls: "Nullable", partitionId: 0, objectId: "n1" },
    });
    const ctx = createMockContext();

    const resp = handleInvoke(
      mock.proxy,
      "findNothing",
      null,
      [],
      ctx.context
    );
    expect(resp.status).toBe("okay");
    expect(fromJsonBytes(resp.payload!)).toBeNull();
  });

  it("should handle method returning undefined (void)", () => {
    @service("Void")
    class VoidService extends OaaSObject {
      data = "";

      @method()
      setData(val: string) {
        this.data = val;
        // no return = undefined
      }
    }

    registerService(VoidService);
    const mock = createMockProxy({
      ref: { cls: "Void", partitionId: 0, objectId: "v1" },
    });
    const ctx = createMockContext();

    const resp = handleInvoke(
      mock.proxy,
      "setData",
      jsonBytes("hello"),
      [],
      ctx.context
    );
    expect(resp.status).toBe("okay");
    // undefined is not serializable to JSON, but the response still succeeds
    expect(fromJsonBytes(mock.data.get("data")!)).toBe("hello");
  });
});
