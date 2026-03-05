/**
 * ObjectProxy — high-level wrapper around the WIT `object-proxy` resource.
 *
 * Provides typed field access with JSON serialization/deserialization.
 * All methods are synchronous under the hood (WIT calls are blocking),
 * but declared as Promise-returning for API consistency with the Python SDK
 * and to match user expectations in async methods.
 */

import { ObjectRef } from "./object_ref.js";
import type { HostObjectProxy, FieldEntry } from "./types.js";

/** Text encoder/decoder for JSON serialization. */
const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class ObjectProxy {
  /** The underlying WIT object-proxy resource handle. */
  private readonly _host: HostObjectProxy;

  /** Cached ObjectRef for this proxy. */
  private _ref: ObjectRef | null = null;

  constructor(hostProxy: HostObjectProxy) {
    this._host = hostProxy;
  }

  /**
   * The identity of the object this proxy points to.
   */
  get ref(): ObjectRef {
    if (this._ref === null) {
      this._ref = ObjectRef.fromData(this._host.ref());
    }
    return this._ref;
  }

  /**
   * Read a single field by key. Returns null if the field doesn't exist.
   * The value is deserialized from JSON.
   */
  async get<T = unknown>(key: string): Promise<T | null> {
    const bytes = this._host.get(key);
    if (bytes === null) {
      return null;
    }
    return JSON.parse(decoder.decode(bytes)) as T;
  }

  /**
   * Read a single field as raw bytes. Returns null if the field doesn't exist.
   */
  async getRaw(key: string): Promise<Uint8Array | null> {
    return this._host.get(key);
  }

  /**
   * Read multiple fields by key (batch). Returns a record of key → value.
   * Missing fields are excluded from the result.
   */
  async getMany<T = unknown>(
    ...keys: string[]
  ): Promise<Record<string, T>> {
    const entries = this._host.getMany(keys);
    const result: Record<string, T> = {};
    for (const entry of entries) {
      result[entry.key] = JSON.parse(decoder.decode(entry.value)) as T;
    }
    return result;
  }

  /**
   * Write a single field. The value is serialized to JSON.
   */
  async set<T = unknown>(key: string, value: T): Promise<void> {
    const bytes = encoder.encode(JSON.stringify(value));
    this._host.set(key, bytes);
  }

  /**
   * Write a single field as raw bytes.
   */
  async setRaw(key: string, value: Uint8Array): Promise<void> {
    this._host.set(key, value);
  }

  /**
   * Write multiple fields (batch). Values are serialized to JSON.
   */
  async setMany(entries: Record<string, unknown>): Promise<void> {
    const fieldEntries: FieldEntry[] = Object.entries(entries).map(
      ([key, value]) => ({
        key,
        value: encoder.encode(JSON.stringify(value)),
      })
    );
    this._host.setMany(fieldEntries);
  }

  /**
   * Delete a single field.
   */
  async delete(key: string): Promise<void> {
    this._host.delete(key);
  }

  /**
   * Read the full object (all entries).
   * Returns a record of key → deserialized JSON value.
   */
  async getAll(): Promise<Record<string, unknown>> {
    const objData = this._host.getAll();
    const result: Record<string, unknown> = {};
    for (const entry of objData.entries) {
      try {
        result[entry.key] = JSON.parse(decoder.decode(entry.value.data));
      } catch {
        // If not valid JSON, store raw bytes as-is
        result[entry.key] = entry.value.data;
      }
    }
    return result;
  }

  /**
   * Invoke a method on this object.
   * Payload is serialized to JSON; response is deserialized from JSON.
   */
  async invoke<TPayload = unknown, TResult = unknown>(
    fnName: string,
    payload?: TPayload
  ): Promise<TResult | null> {
    const payloadBytes =
      payload !== undefined
        ? encoder.encode(JSON.stringify(payload))
        : null;
    const resultBytes = this._host.invoke(fnName, payloadBytes);
    if (resultBytes === null) {
      return null;
    }
    return JSON.parse(decoder.decode(resultBytes)) as TResult;
  }

  /**
   * String representation: "cls/partition/objectId".
   */
  toString(): string {
    return this.ref.toString();
  }
}
