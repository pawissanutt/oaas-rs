/**
 * ObjectProxy Tests — high-level wrapper around WIT object-proxy resource.
 */

import { describe, it, expect } from "vitest";
import { ObjectProxy } from "../src/object_proxy.js";
import { createMockProxy, jsonBytes, fromJsonBytes } from "./helpers.js";

const REF = { cls: "TestClass", partitionId: 1, objectId: "obj-1" };

describe("ObjectProxy", () => {
  // ─── ref ──────────────────────────────────────────────

  describe("ref", () => {
    it("should return an ObjectRef from the host proxy", () => {
      const { proxy } = createMockProxy({ ref: REF });
      const op = new ObjectProxy(proxy);
      expect(op.ref.cls).toBe("TestClass");
      expect(op.ref.partitionId).toBe(1);
      expect(op.ref.objectId).toBe("obj-1");
    });

    it("should cache the ref (only calls host once)", () => {
      const mock = createMockProxy({ ref: REF });
      const op = new ObjectProxy(mock.proxy);
      op.ref;
      op.ref;
      op.ref;
      const refCalls = mock.calls.filter((c) => c.method === "ref");
      expect(refCalls.length).toBe(1);
    });
  });

  // ─── get ──────────────────────────────────────────────

  describe("get()", () => {
    it("should return deserialized JSON value", async () => {
      const initial = new Map([["count", jsonBytes(42)]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      const result = await op.get<number>("count");
      expect(result).toBe(42);
    });

    it("should return null for missing field", async () => {
      const { proxy } = createMockProxy({ ref: REF });
      const op = new ObjectProxy(proxy);
      const result = await op.get("nonexistent");
      expect(result).toBeNull();
    });

    it("should handle string values", async () => {
      const initial = new Map([["name", jsonBytes("Alice")]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      expect(await op.get<string>("name")).toBe("Alice");
    });

    it("should handle object values", async () => {
      const obj = { x: 1, y: "two" };
      const initial = new Map([["data", jsonBytes(obj)]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      expect(await op.get("data")).toEqual(obj);
    });

    it("should handle array values", async () => {
      const arr = [1, 2, 3];
      const initial = new Map([["items", jsonBytes(arr)]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      expect(await op.get("items")).toEqual(arr);
    });

    it("should handle boolean values", async () => {
      const initial = new Map([["active", jsonBytes(true)]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      expect(await op.get<boolean>("active")).toBe(true);
    });

    it("should handle null JSON value", async () => {
      const initial = new Map([["empty", jsonBytes(null)]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      expect(await op.get("empty")).toBeNull();
    });
  });

  // ─── getRaw ───────────────────────────────────────────

  describe("getRaw()", () => {
    it("should return raw bytes", async () => {
      const raw = new Uint8Array([1, 2, 3]);
      const initial = new Map([["bin", raw]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      const result = await op.getRaw("bin");
      expect(result).toEqual(raw);
    });

    it("should return null for missing field", async () => {
      const { proxy } = createMockProxy({ ref: REF });
      const op = new ObjectProxy(proxy);
      expect(await op.getRaw("nope")).toBeNull();
    });
  });

  // ─── getMany ─────────────────────────────────────────

  describe("getMany()", () => {
    it("should return multiple deserialized fields", async () => {
      const initial = new Map([
        ["a", jsonBytes(1)],
        ["b", jsonBytes("two")],
        ["c", jsonBytes(true)],
      ]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      const result = await op.getMany("a", "b", "c");
      expect(result).toEqual({ a: 1, b: "two", c: true });
    });

    it("should omit missing fields", async () => {
      const initial = new Map([["a", jsonBytes(1)]]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      const result = await op.getMany("a", "missing");
      expect(result).toEqual({ a: 1 });
    });

    it("should return empty record for no keys", async () => {
      const { proxy } = createMockProxy({ ref: REF });
      const op = new ObjectProxy(proxy);
      const result = await op.getMany();
      expect(result).toEqual({});
    });
  });

  // ─── set ──────────────────────────────────────────────

  describe("set()", () => {
    it("should serialize and store a value", async () => {
      const mock = createMockProxy({ ref: REF });
      const op = new ObjectProxy(mock.proxy);
      await op.set("count", 42);
      const stored = mock.data.get("count")!;
      expect(fromJsonBytes(stored)).toBe(42);
    });

    it("should overwrite existing values", async () => {
      const initial = new Map([["name", jsonBytes("old")]]);
      const mock = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(mock.proxy);
      await op.set("name", "new");
      expect(fromJsonBytes(mock.data.get("name")!)).toBe("new");
    });

    it("should handle complex objects", async () => {
      const mock = createMockProxy({ ref: REF });
      const op = new ObjectProxy(mock.proxy);
      const complex = { nested: { arr: [1, 2], flag: true } };
      await op.set("data", complex);
      expect(fromJsonBytes(mock.data.get("data")!)).toEqual(complex);
    });
  });

  // ─── setRaw ──────────────────────────────────────────

  describe("setRaw()", () => {
    it("should store raw bytes without JSON serialization", async () => {
      const mock = createMockProxy({ ref: REF });
      const op = new ObjectProxy(mock.proxy);
      const raw = new Uint8Array([0xff, 0x00, 0xab]);
      await op.setRaw("binary", raw);
      expect(mock.data.get("binary")).toEqual(raw);
    });
  });

  // ─── setMany ─────────────────────────────────────────

  describe("setMany()", () => {
    it("should write multiple fields", async () => {
      const mock = createMockProxy({ ref: REF });
      const op = new ObjectProxy(mock.proxy);
      await op.setMany({ x: 1, y: "two", z: [3] });
      expect(fromJsonBytes(mock.data.get("x")!)).toBe(1);
      expect(fromJsonBytes(mock.data.get("y")!)).toBe("two");
      expect(fromJsonBytes(mock.data.get("z")!)).toEqual([3]);
    });

    it("should call host setMany once", async () => {
      const mock = createMockProxy({ ref: REF });
      const op = new ObjectProxy(mock.proxy);
      await op.setMany({ a: 1, b: 2 });
      const setManyCalls = mock.calls.filter((c) => c.method === "setMany");
      expect(setManyCalls.length).toBe(1);
    });
  });

  // ─── delete ──────────────────────────────────────────

  describe("delete()", () => {
    it("should remove a field", async () => {
      const initial = new Map([["x", jsonBytes(1)]]);
      const mock = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(mock.proxy);
      await op.delete("x");
      expect(mock.data.has("x")).toBe(false);
    });
  });

  // ─── getAll ──────────────────────────────────────────

  describe("getAll()", () => {
    it("should return all fields as deserialized values", async () => {
      const initial = new Map([
        ["a", jsonBytes(1)],
        ["b", jsonBytes("hello")],
      ]);
      const { proxy } = createMockProxy({ ref: REF, initialData: initial });
      const op = new ObjectProxy(proxy);
      const result = await op.getAll();
      expect(result).toEqual({ a: 1, b: "hello" });
    });

    it("should return empty record for no stored data", async () => {
      const { proxy } = createMockProxy({ ref: REF });
      const op = new ObjectProxy(proxy);
      const result = await op.getAll();
      expect(result).toEqual({});
    });
  });

  // ─── invoke ──────────────────────────────────────────

  describe("invoke()", () => {
    it("should serialize payload and deserialize result", async () => {
      const mock = createMockProxy({ ref: REF });
      mock.invokeHandler = (_fn, payload) => {
        const input = payload
          ? JSON.parse(new TextDecoder().decode(payload))
          : null;
        return new TextEncoder().encode(
          JSON.stringify({ result: (input?.x ?? 0) + 1 })
        );
      };
      const op = new ObjectProxy(mock.proxy);
      const result = await op.invoke<{ x: number }, { result: number }>(
        "compute",
        { x: 5 }
      );
      expect(result).toEqual({ result: 6 });
    });

    it("should handle null payload", async () => {
      const mock = createMockProxy({ ref: REF });
      mock.invokeHandler = (_fn, _payload) => {
        return new TextEncoder().encode(JSON.stringify("ok"));
      };
      const op = new ObjectProxy(mock.proxy);
      const result = await op.invoke("ping");
      expect(result).toBe("ok");
    });

    it("should return null when host returns null", async () => {
      const mock = createMockProxy({ ref: REF });
      mock.invokeHandler = () => null;
      const op = new ObjectProxy(mock.proxy);
      const result = await op.invoke("void_fn");
      expect(result).toBeNull();
    });
  });

  // ─── toString ─────────────────────────────────────────

  describe("toString()", () => {
    it("should return the ref string form", () => {
      const { proxy } = createMockProxy({ ref: REF });
      const op = new ObjectProxy(proxy);
      expect(op.toString()).toBe("TestClass/1/obj-1");
    });
  });
});
