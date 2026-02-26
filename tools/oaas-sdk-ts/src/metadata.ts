/**
 * Package Metadata Extraction — generates OPackage-compatible metadata
 * from @service / @method decorator metadata.
 *
 * Used at compile time to produce the package descriptor that the PM
 * uses to register the class and its function bindings.
 */

import {
  getServiceMetadata,
  getMethodMetadata,
  getGetterMetadata,
  getSetterMetadata,
} from "./decorators.js";
import type {
  ServiceMetadata,
  MethodMetadata,
  GetterMetadata,
  SetterMetadata,
} from "./decorators.js";
import { OaaSObject } from "./oaas_object.js";
import { discoverFields } from "./state.js";

// ─── Package Descriptor Types ──────────────────────────────

export interface OFunctionBinding {
  /** Function name (matches the @method decorated method name). */
  name: string;
  /** Whether this function is stateless. */
  stateless: boolean;
  /** Optional timeout in milliseconds. */
  timeout?: number;
}

export interface OClassDescriptor {
  /** The class name (from @service). */
  name: string;
  /** Declared state fields with their default types. */
  stateFields: string[];
  /** Function bindings (from @method decorators). */
  functions: OFunctionBinding[];
  /** Getter accessors (from @getter decorators). */
  getters: Array<{ method: string; field: string }>;
  /** Setter accessors (from @setter decorators). */
  setters: Array<{ method: string; field: string }>;
}

export interface OPackageDescriptor {
  /** Package name (from @service opts.package). */
  name: string;
  /** Classes defined in this package. */
  classes: OClassDescriptor[];
}

// ─── Extraction ────────────────────────────────────────────

/**
 * Extract package metadata from a decorated OaaSObject subclass.
 *
 * @param cls - The class constructor decorated with @service.
 * @returns The package descriptor, or null if the class has no @service metadata.
 */
export function extractPackageMetadata(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cls: new (...args: any[]) => OaaSObject
): OPackageDescriptor | null {
  const serviceMeta: ServiceMetadata | undefined = getServiceMetadata(cls);
  if (!serviceMeta) {
    return null;
  }

  const proto = cls.prototype as object;
  const methodMeta: Map<string, MethodMetadata> = getMethodMetadata(proto);
  const getterMeta: Map<string, GetterMetadata> = getGetterMetadata(proto);
  const setterMeta: Map<string, SetterMetadata> = getSetterMetadata(proto);
  const fields = discoverFields(cls);

  const functions: OFunctionBinding[] = [];
  for (const [, meta] of methodMeta) {
    functions.push({
      name: meta.name,
      stateless: meta.stateless,
      timeout: meta.timeout,
    });
  }

  const getters = Array.from(getterMeta.values()).map((g) => ({
    method: g.methodName,
    field: g.field,
  }));

  const setters = Array.from(setterMeta.values()).map((s) => ({
    method: s.methodName,
    field: s.field,
  }));

  const classDescriptor: OClassDescriptor = {
    name: serviceMeta.name,
    stateFields: fields,
    functions,
    getters,
    setters,
  };

  return {
    name: serviceMeta.package ?? serviceMeta.name,
    classes: [classDescriptor],
  };
}

/**
 * Serialize package metadata to JSON string.
 */
export function packageMetadataToJson(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  cls: new (...args: any[]) => OaaSObject
): string | null {
  const meta = extractPackageMetadata(cls);
  if (!meta) {
    return null;
  }
  return JSON.stringify(meta, null, 2);
}
