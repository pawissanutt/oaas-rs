/**
 * @oaas/sdk — OaaS TypeScript SDK
 *
 * OOP scripting layer for the OaaS WASM runtime.
 * Users write TypeScript classes extending OaaSObject,
 * decorate methods with @service/@method, and the SDK handles
 * state management, method dispatch, and host communication.
 */

// Core classes
export { ObjectRef } from "./object_ref.js";
export { ObjectProxy } from "./object_proxy.js";
export { OaaSObject, OAAS_CONTEXT } from "./oaas_object.js";
export type { OaaSObjectContext } from "./oaas_object.js";
export { OaaSError, isOaaSError } from "./errors.js";

// Decorators
export { service, method, getter, setter } from "./decorators.js";

// Decorator metadata access (for tooling / introspection)
export {
  getServiceMetadata,
  getMethodMetadata,
  getGetterMetadata,
  getSetterMetadata,
} from "./decorators.js";
export type {
  ServiceMetadata,
  MethodMetadata,
  GetterMetadata,
  SetterMetadata,
} from "./decorators.js";

// Dispatch shim (used by ComponentizeJS entry point)
export { registerService, handleInvoke, headersToRecord } from "./dispatch.js";

// State management (internal, but exported for testing)
export { discoverFields, loadFields, saveChangedFields } from "./state.js";

// Package metadata extraction
export {
  extractPackageMetadata,
  packageMetadataToJson,
} from "./metadata.js";
export type {
  OFunctionBinding,
  OClassDescriptor,
  OPackageDescriptor,
} from "./metadata.js";

// WIT types (re-export for advanced usage)
export type {
  ResponseStatus,
  InvocationResponse,
  KeyValue,
  ObjectRefData,
  FieldEntry,
  OdgmError,
  LogLevel,
  ValType,
  ValData,
  ObjectMeta,
  Entry,
  ObjData,
  HostObjectProxy,
  HostObjectContext,
} from "./types.js";
