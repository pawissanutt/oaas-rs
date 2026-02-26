/**
 * State Management Tests — load/save/diff of object fields.
 */

import { describe, it, expect } from "vitest";
import { discoverFields, loadFields, saveChangedFields } from "../src/state.js";
import { createMockProxy, jsonBytes, fromJsonBytes } from "./helpers.js";

const REF = { cls: "Test", partitionId: 0, objectId: "obj-1" };

// ─── discoverFields ────────────────────────────────────────

describe("discoverFields", () => {
  it("should discover default-initialized fields", () => {
    class MyObj {
      count = 0;
      name = "default";
      items: string[] = [];
    }
    const fields = discoverFields(MyObj);
    expect(fields).toEqual(["count", "name", "items"]);
  });

  it("should return empty array for class with no fields", () => {
    class Empty {}
    const fields = discoverFields(Empty);
    expect(fields).toEqual([]);
  });

  it("should ignore prototype methods", () => {
    class WithMethod {
      count = 0;
      doStuff() {}
    }
    const fields = discoverFields(WithMethod);
    expect(fields).toEqual(["count"]);
    expect(fields).not.toContain("doStuff");
  });

  it("should handle boolean, null, and undefined defaults", () => {
    class Mixed {
      active = true;
      empty: null = null;
      maybeVal: undefined = undefined;
    }
    const fields = discoverFields(Mixed);
    expect(fields).toContain("active");
    expect(fields).toContain("empty");
    expect(fields).toContain("maybeVal");
  });

  it("should handle nested object defaults", () => {
    class Nested {
      config = { theme: "dark", count: 0 };
    }
    const fields = discoverFields(Nested);
    expect(fields).toEqual(["config"]);
  });

  it("should return empty array if constructor throws", () => {
    class Broken {
      constructor() {
        throw new Error("cannot construct");
      }
    }
    const fields = discoverFields(Broken);
    expect(fields).toEqual([]);
  });
});

// ─── loadFields ────────────────────────────────────────────

describe("loadFields", () => {
  it("should load stored field values into the instance", () => {
    const initial = new Map([
      ["count", jsonBytes(42)],
      ["name", jsonBytes("Alice")],
    ]);
    const { proxy } = createMockProxy({ ref: REF, initialData: initial });
    const instance: Record<string, unknown> = { count: 0, name: "" };
    const fields = ["count", "name"];

    loadFields(instance, proxy, fields);

    expect(instance.count).toBe(42);
    expect(instance.name).toBe("Alice");
  });

  it("should keep defaults for fields not in storage", () => {
    const { proxy } = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { count: 99, name: "default" };
    const fields = ["count", "name"];

    loadFields(instance, proxy, fields);

    expect(instance.count).toBe(99);
    expect(instance.name).toBe("default");
  });

  it("should return a snapshot of all field values", () => {
    const initial = new Map([["count", jsonBytes(5)]]);
    const { proxy } = createMockProxy({ ref: REF, initialData: initial });
    const instance: Record<string, unknown> = { count: 0, name: "def" };

    const snapshot = loadFields(instance, proxy, ["count", "name"]);

    expect(snapshot.get("count")).toBe(JSON.stringify(5));
    expect(snapshot.get("name")).toBe(JSON.stringify("def"));
  });

  it("should handle empty field list", () => {
    const { proxy } = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = {};

    const snapshot = loadFields(instance, proxy, []);
    expect(snapshot.size).toBe(0);
  });

  it("should call host getMany once", () => {
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { a: 0, b: 0 };

    loadFields(instance, mock.proxy, ["a", "b"]);

    const getManyCalls = mock.calls.filter((c) => c.method === "getMany");
    expect(getManyCalls.length).toBe(1);
  });

  it("should handle array field values", () => {
    const initial = new Map([["items", jsonBytes([1, 2, 3])]]);
    const { proxy } = createMockProxy({ ref: REF, initialData: initial });
    const instance: Record<string, unknown> = { items: [] };

    loadFields(instance, proxy, ["items"]);
    expect(instance.items).toEqual([1, 2, 3]);
  });

  it("should handle nested object field values", () => {
    const initial = new Map([
      ["config", jsonBytes({ theme: "dark", maxItems: 50 })],
    ]);
    const { proxy } = createMockProxy({ ref: REF, initialData: initial });
    const instance: Record<string, unknown> = { config: {} };

    loadFields(instance, proxy, ["config"]);
    expect(instance.config).toEqual({ theme: "dark", maxItems: 50 });
  });

  it("should skip fields with invalid JSON in storage (keep default)", () => {
    const raw = new TextEncoder().encode("not-valid-json{{{");
    const initial = new Map([["bad", raw]]);
    const { proxy } = createMockProxy({ ref: REF, initialData: initial });
    const instance: Record<string, unknown> = { bad: "fallback" };

    loadFields(instance, proxy, ["bad"]);
    expect(instance.bad).toBe("fallback");
  });
});

// ─── saveChangedFields ────────────────────────────────────

describe("saveChangedFields", () => {
  it("should save only changed fields", () => {
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { a: 10, b: "same" };
    const snapshot = new Map([
      ["a", JSON.stringify(5)], // changed: 5 → 10
      ["b", JSON.stringify("same")], // unchanged
    ]);

    saveChangedFields(instance, mock.proxy, ["a", "b"], snapshot);

    // Only 'a' should have been written
    const setManyCalls = mock.calls.filter((c) => c.method === "setMany");
    expect(setManyCalls.length).toBe(1);
    expect(fromJsonBytes(mock.data.get("a")!)).toBe(10);
    expect(mock.data.has("b")).toBe(false); // not written
  });

  it("should not call setMany if nothing changed", () => {
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { x: 1 };
    const snapshot = new Map([["x", JSON.stringify(1)]]);

    saveChangedFields(instance, mock.proxy, ["x"], snapshot);

    const setManyCalls = mock.calls.filter((c) => c.method === "setMany");
    expect(setManyCalls.length).toBe(0);
  });

  it("should detect in-place array mutation", () => {
    const arr = [1, 2, 3];
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { items: arr };
    const snapshot = new Map([["items", JSON.stringify([1, 2, 3])]]);

    // Mutate in-place
    arr.push(4);

    saveChangedFields(instance, mock.proxy, ["items"], snapshot);

    expect(fromJsonBytes(mock.data.get("items")!)).toEqual([1, 2, 3, 4]);
  });

  it("should detect in-place object mutation", () => {
    const obj = { count: 0 } as Record<string, unknown>;
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { data: obj };
    const snapshot = new Map([["data", JSON.stringify({ count: 0 })]]);

    obj.count = 5;

    saveChangedFields(instance, mock.proxy, ["data"], snapshot);
    expect(fromJsonBytes(mock.data.get("data")!)).toEqual({ count: 5 });
  });

  it("should save all changed fields in a single setMany call", () => {
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { a: 10, b: 20, c: 30 };
    const snapshot = new Map([
      ["a", JSON.stringify(1)],
      ["b", JSON.stringify(2)],
      ["c", JSON.stringify(30)], // unchanged
    ]);

    saveChangedFields(instance, mock.proxy, ["a", "b", "c"], snapshot);

    const setManyCalls = mock.calls.filter((c) => c.method === "setMany");
    expect(setManyCalls.length).toBe(1);
    expect(fromJsonBytes(mock.data.get("a")!)).toBe(10);
    expect(fromJsonBytes(mock.data.get("b")!)).toBe(20);
    expect(mock.data.has("c")).toBe(false);
  });

  it("should handle fields set to undefined", () => {
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = { x: undefined };
    const snapshot = new Map([["x", JSON.stringify(5)]]);

    saveChangedFields(instance, mock.proxy, ["x"], snapshot);
    // undefined serializes to undefined in JSON.stringify, which becomes "undefined" string,
    // but it's different from "5" so it should trigger a save
    expect(mock.data.has("x")).toBe(true);
  });

  it("should handle empty fields list", () => {
    const mock = createMockProxy({ ref: REF });
    const instance: Record<string, unknown> = {};
    const snapshot = new Map<string, string>();

    saveChangedFields(instance, mock.proxy, [], snapshot);

    expect(mock.calls.length).toBe(0);
  });
});
