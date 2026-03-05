/**
 * OaaSError — thrown by user methods to signal an application-level error.
 *
 * When caught by the SDK shim, this produces a `status: "app-error"` response.
 * Any other thrown error produces `status: "system-error"`.
 */
export class OaaSError extends Error {
  /** A unique tag to identify OaaSError instances. */
  readonly __oaasError = true;

  constructor(message: string) {
    super(message);
    this.name = "OaaSError";
    // Fix prototype chain for instanceof checks.
    Object.setPrototypeOf(this, OaaSError.prototype);
  }
}

/**
 * Type guard to check if an error is an OaaSError.
 * Works across module boundaries where instanceof might fail.
 */
export function isOaaSError(err: unknown): err is OaaSError {
  return (
    err instanceof OaaSError ||
    (typeof err === "object" &&
      err !== null &&
      "__oaasError" in err &&
      (err as Record<string, unknown>).__oaasError === true)
  );
}
