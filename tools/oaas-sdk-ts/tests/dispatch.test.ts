/**
 * Dispatch Tests — method dispatch shim (handleInvoke, registerService).
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  registerService,
  handleInvoke,
  headersToRecord,
} from "../src/dispatch.js";
import { OaaSObject } from "../src/oaas_object.js";
import { OaaSError } from "../src/errors.js";
import { service, method } from "../src/decorators.js";
import { createMockProxy, createMockContext, jsonBytes, fromJsonBytes } from "./helpers.js";

const REF = { cls: "TestService", partitionId: 0, objectId: "obj-1" };
const encoder = new TextEncoder();

// ─── Test Service Classes ──────────────────────────────────

@service("Counter", { package: "test" })
class Counter extends OaaSObject {
  count = 0;
  history: string[] = [];

  @method()
  increment(amount?: number) {
    const inc = amount ?? 1;
    this.count += inc;
    this.history.push(`+${inc}`);
    return this.count;
  }

  @method()
  getCount() {
    return this.count;
  }

  @method()
  reset() {
    this.count = 0;
    this.history = [];
  }
}

@service("Calculator")
class Calculator extends OaaSObject {
  @method({ stateless: true })
  add(args: { a: number; b: number }) {
    return args.a + args.b;
  }

  @method({ stateless: true })
  multiply(args: { a: number; b: number }) {
    return args.a * args.b;
  }
}

@service("Faulty")
class Faulty extends OaaSObject {
  @method()
  appError() {
    throw new OaaSError("application-level failure");
  }

  @method()
  systemError() {
    throw new Error("unexpected system crash");
  }

  @method()
  throwString() {
    throw "raw string error";
  }
}

@service("Logger")
class LoggerService extends OaaSObject {
  @method()
  doWork() {
    this.log("info", "starting work");
    this.log("debug", "details here");
    return "done";
  }
}

@service("CrossRef")
class CrossRefService extends OaaSObject {
  @method()
  readOther(refStr: string) {
    const other = this.object(refStr);
    return { ref: other.ref.toString() };
  }
}

// ─── Tests ─────────────────────────────────────────────────

describe("handleInvoke", () => {
  // ─── Stateful method dispatch ──────────────────────────

  describe("stateful methods", () => {
    beforeEach(() => {
      registerService(Counter);
    });

    it("should invoke a method and return the result", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "increment",
        encoder.encode(JSON.stringify(5)),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(5);
    });

    it("should load state before and save state after", () => {
      // Pre-populate storage with count=10
      const initial = new Map([
        ["count", jsonBytes(10)],
        ["history", jsonBytes(["initial"])],
      ]);
      const mock = createMockProxy({ ref: REF, initialData: initial });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "increment",
        encoder.encode(JSON.stringify(3)),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(13);

      // Verify state was saved back
      expect(fromJsonBytes(mock.data.get("count")!)).toBe(13);
      expect(fromJsonBytes(mock.data.get("history")!)).toEqual([
        "initial",
        "+3",
      ]);
    });

    it("should use default field values when storage is empty", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "increment",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      // default count=0, increment by default 1 → 1
      expect(fromJsonBytes(response.payload!)).toBe(1);
      expect(fromJsonBytes(mock.data.get("count")!)).toBe(1);
    });

    it("should handle read-only methods (no state changes)", () => {
      const initial = new Map([
        ["count", jsonBytes(42)],
        ["history", jsonBytes([])],
      ]);
      const mock = createMockProxy({ ref: REF, initialData: initial });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "getCount",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(42);

      // No setMany should have been called (nothing changed)
      const setManyCalls = mock.calls.filter((c) => c.method === "setMany");
      expect(setManyCalls.length).toBe(0);
    });

    it("should handle method that resets state", () => {
      const initial = new Map([
        ["count", jsonBytes(100)],
        ["history", jsonBytes(["old"])],
      ]);
      const mock = createMockProxy({ ref: REF, initialData: initial });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "reset",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(mock.data.get("count")!)).toBe(0);
      expect(fromJsonBytes(mock.data.get("history")!)).toEqual([]);
    });

    it("should handle multiple sequential invocations", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      handleInvoke(mock.proxy, "increment", jsonBytes(5), [], ctx.context);
      handleInvoke(mock.proxy, "increment", jsonBytes(3), [], ctx.context);
      handleInvoke(mock.proxy, "increment", jsonBytes(2), [], ctx.context);

      expect(fromJsonBytes(mock.data.get("count")!)).toBe(10);
      expect(fromJsonBytes(mock.data.get("history")!)).toEqual([
        "+5",
        "+3",
        "+2",
      ]);
    });
  });

  // ─── Stateless method dispatch ─────────────────────────

  describe("stateless methods", () => {
    beforeEach(() => {
      registerService(Calculator);
    });

    it("should invoke a stateless method without loading/saving state", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "add",
        jsonBytes({ a: 3, b: 7 }),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(10);

      // No getMany or setMany should have been called
      const getManyCalls = mock.calls.filter((c) => c.method === "getMany");
      const setManyCalls = mock.calls.filter((c) => c.method === "setMany");
      expect(getManyCalls.length).toBe(0);
      expect(setManyCalls.length).toBe(0);
    });

    it("should handle multiply stateless method", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "multiply",
        jsonBytes({ a: 4, b: 5 }),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(20);
    });
  });

  // ─── Error handling ────────────────────────────────────

  describe("error handling", () => {
    beforeEach(() => {
      registerService(Faulty);
    });

    it("should return app-error for OaaSError", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "appError",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("app-error");
      const payload = fromJsonBytes<{ error: string }>(response.payload!);
      expect(payload.error).toBe("application-level failure");
    });

    it("should return system-error for regular Error", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "systemError",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("system-error");
      const payload = fromJsonBytes<{ error: string }>(response.payload!);
      expect(payload.error).toBe("unexpected system crash");
    });

    it("should return system-error for thrown strings", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "throwString",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("system-error");
      const payload = fromJsonBytes<{ error: string }>(response.payload!);
      expect(payload.error).toBe("raw string error");
    });

    it("should return invalid-request for unknown method", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "nonExistentMethod",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("invalid-request");
    });

    it("should return invalid-request for invalid JSON payload", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "appError",
        encoder.encode("{{not json"),
        [],
        ctx.context
      );

      expect(response.status).toBe("invalid-request");
    });
  });

  // ─── Logging ───────────────────────────────────────────

  describe("logging", () => {
    beforeEach(() => {
      registerService(LoggerService);
    });

    it("should route log calls to host context", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      handleInvoke(mock.proxy, "doWork", null, [], ctx.context);

      expect(ctx.logs).toEqual([
        { level: "info", message: "starting work" },
        { level: "debug", message: "details here" },
      ]);
    });
  });

  // ─── Cross-object access ──────────────────────────────

  describe("cross-object access", () => {
    beforeEach(() => {
      registerService(CrossRefService);
    });

    it("should allow getting a proxy to another object", () => {
      const mock = createMockProxy({ ref: REF });
      const otherRef = {
        cls: "OtherClass",
        partitionId: 5,
        objectId: "other-1",
      };
      const otherMock = createMockProxy({ ref: otherRef });
      const ctx = createMockContext();
      ctx.proxies.set("OtherClass/5/other-1", otherMock.proxy);

      const response = handleInvoke(
        mock.proxy,
        "readOther",
        jsonBytes("OtherClass/5/other-1"),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      const result = fromJsonBytes<{ ref: string }>(response.payload!);
      expect(result.ref).toBe("OtherClass/5/other-1");
    });
  });

  // ─── Payload handling ─────────────────────────────────

  describe("payload handling", () => {
    beforeEach(() => {
      registerService(Counter);
    });

    it("should handle null payload (method with optional args)", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "increment",
        null,
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      // Default increment amount = 1
      expect(fromJsonBytes(response.payload!)).toBe(1);
    });

    it("should handle empty payload bytes", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "increment",
        new Uint8Array(0),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(1);
    });

    it("should pass deserialized payload as argument", () => {
      const mock = createMockProxy({ ref: REF });
      const ctx = createMockContext();

      const response = handleInvoke(
        mock.proxy,
        "increment",
        jsonBytes(7),
        [],
        ctx.context
      );

      expect(response.status).toBe("okay");
      expect(fromJsonBytes(response.payload!)).toBe(7);
    });
  });
});

// ─── headersToRecord ───────────────────────────────────────

describe("headersToRecord", () => {
  it("should convert key-value array to record", () => {
    const headers = [
      { key: "Content-Type", value: "application/json" },
      { key: "X-Custom", value: "test" },
    ];
    expect(headersToRecord(headers)).toEqual({
      "Content-Type": "application/json",
      "X-Custom": "test",
    });
  });

  it("should return empty record for empty headers", () => {
    expect(headersToRecord([])).toEqual({});
  });

  it("should use last value for duplicate keys", () => {
    const headers = [
      { key: "X-Key", value: "first" },
      { key: "X-Key", value: "second" },
    ];
    expect(headersToRecord(headers)).toEqual({ "X-Key": "second" });
  });
});
