/**
 * ObjectRef — unified object identity: (class, partition, objectId).
 *
 * Corresponds to WIT `object-ref` record. Provides convenience constructors
 * and a string form "cls/partition/objectId" for human-readable usage.
 */

import type { ObjectRefData } from "./types.js";

export class ObjectRef {
  /** Class name (must not contain '/'). */
  readonly cls: string;
  /** Partition ID (unsigned 32-bit integer). */
  readonly partitionId: number;
  /** Object ID (must not contain '/'). */
  readonly objectId: string;

  private constructor(cls: string, partitionId: number, objectId: string) {
    this.cls = cls;
    this.partitionId = partitionId;
    this.objectId = objectId;
    Object.freeze(this);
  }

  /**
   * Create an ObjectRef from individual components.
   * @throws Error if cls or objectId contains '/'.
   * @throws Error if partitionId is not a non-negative integer.
   */
  static from(cls: string, partitionId: number, objectId: string): ObjectRef {
    if (cls.includes("/")) {
      throw new Error(
        `Class name must not contain '/': "${cls}"`
      );
    }
    if (objectId.includes("/")) {
      throw new Error(
        `Object ID must not contain '/': "${objectId}"`
      );
    }
    if (!Number.isInteger(partitionId) || partitionId < 0) {
      throw new Error(
        `Partition ID must be a non-negative integer, got: ${partitionId}`
      );
    }
    if (partitionId > 0xffffffff) {
      throw new Error(
        `Partition ID must fit in u32, got: ${partitionId}`
      );
    }
    return new ObjectRef(cls, partitionId, objectId);
  }

  /**
   * Parse the string form "cls/partition/objectId".
   * Uses the first and second '/' as split points; the remainder is the objectId.
   * This allows objectId to be an arbitrary string (without '/').
   *
   * @throws Error if the format is invalid.
   */
  static parse(str: string): ObjectRef {
    const firstSlash = str.indexOf("/");
    if (firstSlash === -1) {
      throw new Error(
        `Invalid object-ref string (missing first '/'): "${str}"`
      );
    }
    const secondSlash = str.indexOf("/", firstSlash + 1);
    if (secondSlash === -1) {
      throw new Error(
        `Invalid object-ref string (missing second '/'): "${str}"`
      );
    }

    const cls = str.substring(0, firstSlash);
    const partStr = str.substring(firstSlash + 1, secondSlash);
    const objectId = str.substring(secondSlash + 1);

    if (cls.length === 0) {
      throw new Error(`Invalid object-ref string (empty class): "${str}"`);
    }
    if (objectId.length === 0) {
      throw new Error(`Invalid object-ref string (empty object ID): "${str}"`);
    }

    const partitionId = parseInt(partStr, 10);
    if (isNaN(partitionId) || partitionId < 0) {
      throw new Error(
        `Invalid partition ID in object-ref string: "${partStr}"`
      );
    }

    return new ObjectRef(cls, partitionId, objectId);
  }

  /**
   * Convert from the raw WIT-level ObjectRefData.
   */
  static fromData(data: ObjectRefData): ObjectRef {
    return new ObjectRef(data.cls, data.partitionId, data.objectId);
  }

  /**
   * Convert to the raw WIT-level ObjectRefData.
   */
  toData(): ObjectRefData {
    return {
      cls: this.cls,
      partitionId: this.partitionId,
      objectId: this.objectId,
    };
  }

  /**
   * String representation: "cls/partition/objectId".
   */
  toString(): string {
    return `${this.cls}/${this.partitionId}/${this.objectId}`;
  }

  /**
   * Check equality with another ObjectRef.
   */
  equals(other: ObjectRef): boolean {
    return (
      this.cls === other.cls &&
      this.partitionId === other.partitionId &&
      this.objectId === other.objectId
    );
  }
}
