/**
 * State Management — transparent load/save of object fields around method calls.
 *
 * Before calling a user method:
 *   1. Read all declared fields via proxy.getMany()
 *   2. Deserialize JSON → set on `this`
 *   3. Snapshot each field value (JSON.stringify) for later diff
 *
 * After the method returns:
 *   1. Re-serialize each field
 *   2. Compare with snapshot → find changed fields
 *   3. Write changes via proxy.setMany()
 *
 * Stateless methods skip the entire load/save cycle.
 */

import type { FieldEntry, HostObjectProxy } from "./types.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/**
 * Discover the declared fields on an OaaSObject subclass instance.
 * Instantiates the class to get default values, then returns Object.keys().
 * This is called once at module init time.
 */
export function discoverFields(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cls: new (...args: any[]) => object
): string[] {
  try {
    const instance = new cls();
    // Object.keys returns own enumerable properties — exactly the fields
    // with default values that the user declared.
    return Object.keys(instance);
  } catch {
    // If the constructor throws (e.g., requires arguments), return empty.
    // The shim will operate without auto-persisted fields.
    return [];
  }
}

/**
 * Load all declared fields from the proxy into the instance.
 * Returns a snapshot map (field → JSON string) for later diffing.
 */
export function loadFields(
  instance: Record<string, unknown>,
  proxy: HostObjectProxy,
  fields: string[]
): Map<string, string> {
  const snapshot = new Map<string, string>();

  if (fields.length === 0) {
    return snapshot;
  }

  const entries = proxy.getMany(fields);
  const entryMap = new Map<string, Uint8Array>();
  for (const entry of entries) {
    entryMap.set(entry.key, entry.value);
  }

  for (const field of fields) {
    const bytes = entryMap.get(field);
    if (bytes !== undefined) {
      try {
        const value = JSON.parse(decoder.decode(bytes));
        instance[field] = value;
      } catch {
        // If the stored value isn't valid JSON, leave the default in place.
      }
    }
    // Snapshot the current value (whether loaded or default).
    snapshot.set(field, JSON.stringify(instance[field]));
  }

  return snapshot;
}

/**
 * Save changed fields back to the proxy.
 * Compares current field values against the pre-call snapshot.
 * Only writes fields whose JSON serialization has changed.
 */
export function saveChangedFields(
  instance: Record<string, unknown>,
  proxy: HostObjectProxy,
  fields: string[],
  snapshot: Map<string, string>
): void {
  const changed: FieldEntry[] = [];

  for (const field of fields) {
    const currentJson = JSON.stringify(instance[field]);
    const previousJson = snapshot.get(field);

    if (currentJson !== previousJson) {
      changed.push({
        key: field,
        value: encoder.encode(currentJson),
      });
    }
  }

  if (changed.length > 0) {
    proxy.setMany(changed);
  }
}
