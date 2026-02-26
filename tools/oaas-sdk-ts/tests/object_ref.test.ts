/**
 * ObjectRef Tests — comprehensive tests for the unified object identity class.
 */

import { describe, it, expect } from "vitest";
import { ObjectRef } from "../src/object_ref.js";

describe("ObjectRef", () => {
  // ─── ObjectRef.from() ──────────────────────────────────

  describe("from()", () => {
    it("should create an ObjectRef with valid components", () => {
      const ref = ObjectRef.from("Counter", 42, "obj-1");
      expect(ref.cls).toBe("Counter");
      expect(ref.partitionId).toBe(42);
      expect(ref.objectId).toBe("obj-1");
    });

    it("should accept partitionId = 0", () => {
      const ref = ObjectRef.from("Cls", 0, "id");
      expect(ref.partitionId).toBe(0);
    });

    it("should accept max u32 partitionId", () => {
      const ref = ObjectRef.from("Cls", 0xffffffff, "id");
      expect(ref.partitionId).toBe(0xffffffff);
    });

    it("should throw if class name contains '/'", () => {
      expect(() => ObjectRef.from("My/Class", 0, "id")).toThrow(
        "Class name must not contain '/'"
      );
    });

    it("should throw if objectId contains '/'", () => {
      expect(() => ObjectRef.from("Cls", 0, "bad/id")).toThrow(
        "Object ID must not contain '/'"
      );
    });

    it("should throw if partitionId is negative", () => {
      expect(() => ObjectRef.from("Cls", -1, "id")).toThrow(
        "non-negative integer"
      );
    });

    it("should throw if partitionId is not an integer", () => {
      expect(() => ObjectRef.from("Cls", 3.14, "id")).toThrow(
        "non-negative integer"
      );
    });

    it("should throw if partitionId exceeds u32 max", () => {
      expect(() => ObjectRef.from("Cls", 0x100000000, "id")).toThrow(
        "fit in u32"
      );
    });

    it("should throw if partitionId is NaN", () => {
      expect(() => ObjectRef.from("Cls", NaN, "id")).toThrow(
        "non-negative integer"
      );
    });

    it("should throw if partitionId is Infinity", () => {
      expect(() => ObjectRef.from("Cls", Infinity, "id")).toThrow(
        "non-negative integer"
      );
    });
  });

  // ─── ObjectRef.parse() ─────────────────────────────────

  describe("parse()", () => {
    it("should parse a valid ref string", () => {
      const ref = ObjectRef.parse("Counter/42/obj-1");
      expect(ref.cls).toBe("Counter");
      expect(ref.partitionId).toBe(42);
      expect(ref.objectId).toBe("obj-1");
    });

    it("should parse partitionId = 0", () => {
      const ref = ObjectRef.parse("Cls/0/id");
      expect(ref.partitionId).toBe(0);
    });

    it("should handle objectId with special characters (no /)", () => {
      const ref = ObjectRef.parse("Cls/1/uuid-abc-123");
      expect(ref.objectId).toBe("uuid-abc-123");
    });

    it("should throw on missing first '/'", () => {
      expect(() => ObjectRef.parse("NoSlash")).toThrow("missing first '/'");
    });

    it("should throw on missing second '/'", () => {
      expect(() => ObjectRef.parse("Cls/42")).toThrow("missing second '/'");
    });

    it("should throw on empty class", () => {
      expect(() => ObjectRef.parse("/42/id")).toThrow("empty class");
    });

    it("should throw on empty object ID", () => {
      expect(() => ObjectRef.parse("Cls/42/")).toThrow("empty object ID");
    });

    it("should throw on non-numeric partition ID", () => {
      expect(() => ObjectRef.parse("Cls/abc/id")).toThrow(
        "Invalid partition ID"
      );
    });

    it("should throw on negative partition ID", () => {
      expect(() => ObjectRef.parse("Cls/-1/id")).toThrow(
        "Invalid partition ID"
      );
    });
  });

  // ─── toString() / roundtrip ────────────────────────────

  describe("toString()", () => {
    it("should produce the canonical string form", () => {
      const ref = ObjectRef.from("Counter", 42, "obj-1");
      expect(ref.toString()).toBe("Counter/42/obj-1");
    });

    it("should roundtrip through parse", () => {
      const original = ObjectRef.from("MyClass", 100, "test-obj");
      const parsed = ObjectRef.parse(original.toString());
      expect(parsed.cls).toBe(original.cls);
      expect(parsed.partitionId).toBe(original.partitionId);
      expect(parsed.objectId).toBe(original.objectId);
    });
  });

  // ─── fromData() / toData() ────────────────────────────

  describe("fromData() / toData()", () => {
    it("should convert from ObjectRefData", () => {
      const ref = ObjectRef.fromData({
        cls: "Foo",
        partitionId: 7,
        objectId: "bar",
      });
      expect(ref.cls).toBe("Foo");
      expect(ref.partitionId).toBe(7);
      expect(ref.objectId).toBe("bar");
    });

    it("should roundtrip through toData/fromData", () => {
      const original = ObjectRef.from("Test", 99, "myobj");
      const data = original.toData();
      const restored = ObjectRef.fromData(data);
      expect(restored.cls).toBe(original.cls);
      expect(restored.partitionId).toBe(original.partitionId);
      expect(restored.objectId).toBe(original.objectId);
    });

    it("toData should produce a plain object", () => {
      const ref = ObjectRef.from("Cls", 1, "id");
      const data = ref.toData();
      expect(data).toEqual({ cls: "Cls", partitionId: 1, objectId: "id" });
    });
  });

  // ─── equals() ─────────────────────────────────────────

  describe("equals()", () => {
    it("should return true for identical refs", () => {
      const a = ObjectRef.from("Cls", 1, "id");
      const b = ObjectRef.from("Cls", 1, "id");
      expect(a.equals(b)).toBe(true);
    });

    it("should return false for different class", () => {
      const a = ObjectRef.from("Cls1", 1, "id");
      const b = ObjectRef.from("Cls2", 1, "id");
      expect(a.equals(b)).toBe(false);
    });

    it("should return false for different partition", () => {
      const a = ObjectRef.from("Cls", 1, "id");
      const b = ObjectRef.from("Cls", 2, "id");
      expect(a.equals(b)).toBe(false);
    });

    it("should return false for different objectId", () => {
      const a = ObjectRef.from("Cls", 1, "id1");
      const b = ObjectRef.from("Cls", 1, "id2");
      expect(a.equals(b)).toBe(false);
    });
  });

  // ─── Immutability ─────────────────────────────────────

  describe("immutability", () => {
    it("should have readonly cls", () => {
      const ref = ObjectRef.from("Cls", 1, "id");
      // @ts-expect-error — testing runtime immutability
      expect(() => { ref.cls = "Other"; }).toThrow();
    });

    it("should have readonly partitionId", () => {
      const ref = ObjectRef.from("Cls", 1, "id");
      // @ts-expect-error — testing runtime immutability
      expect(() => { ref.partitionId = 2; }).toThrow();
    });

    it("should have readonly objectId", () => {
      const ref = ObjectRef.from("Cls", 1, "id");
      // @ts-expect-error — testing runtime immutability
      expect(() => { ref.objectId = "other"; }).toThrow();
    });
  });
});
