/**
 * OaaSError Tests — error class and type guard.
 */

import { describe, it, expect } from "vitest";
import { OaaSError, isOaaSError } from "../src/errors.js";

describe("OaaSError", () => {
  it("should be an instance of Error", () => {
    const err = new OaaSError("test");
    expect(err).toBeInstanceOf(Error);
  });

  it("should be an instance of OaaSError", () => {
    const err = new OaaSError("test");
    expect(err).toBeInstanceOf(OaaSError);
  });

  it("should have name = 'OaaSError'", () => {
    const err = new OaaSError("something failed");
    expect(err.name).toBe("OaaSError");
  });

  it("should preserve the error message", () => {
    const err = new OaaSError("custom message");
    expect(err.message).toBe("custom message");
  });

  it("should have __oaasError = true", () => {
    const err = new OaaSError("test");
    expect(err.__oaasError).toBe(true);
  });

  it("should have a stack trace", () => {
    const err = new OaaSError("test");
    expect(err.stack).toBeDefined();
    expect(err.stack).toContain("OaaSError");
  });
});

describe("isOaaSError", () => {
  it("should return true for OaaSError instances", () => {
    const err = new OaaSError("test");
    expect(isOaaSError(err)).toBe(true);
  });

  it("should return false for plain Error", () => {
    const err = new Error("test");
    expect(isOaaSError(err)).toBe(false);
  });

  it("should return false for null", () => {
    expect(isOaaSError(null)).toBe(false);
  });

  it("should return false for undefined", () => {
    expect(isOaaSError(undefined)).toBe(false);
  });

  it("should return false for strings", () => {
    expect(isOaaSError("error")).toBe(false);
  });

  it("should return false for numbers", () => {
    expect(isOaaSError(42)).toBe(false);
  });

  it("should detect duck-typed OaaSError (cross-module)", () => {
    const fakeErr = { __oaasError: true, message: "fake", name: "OaaSError" };
    expect(isOaaSError(fakeErr)).toBe(true);
  });

  it("should reject objects with __oaasError = false", () => {
    const fakeErr = { __oaasError: false, message: "fake" };
    expect(isOaaSError(fakeErr)).toBe(false);
  });

  it("should reject objects without __oaasError", () => {
    const fakeErr = { message: "fake", name: "OaaSError" };
    expect(isOaaSError(fakeErr)).toBe(false);
  });
});
