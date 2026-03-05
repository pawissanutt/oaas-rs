/**
 * Counter — TypeScript OaaS guest for E2E testing.
 *
 * Stateful service that maintains:
 * - count: a numeric counter
 * - history: an array of operation log entries
 *
 * Used by system E2E tests to verify:
 * - TypeScript → WASM compilation pipeline
 * - State persistence (auto load/save around method calls)
 * - Stateless function invocation
 * - Error handling (OaaSError for app-level errors)
 */

import { service, method, OaaSObject, OaaSError } from "@oaas/sdk";

@service("TsCounter", { package: "e2e-ts-test" })
class Counter extends OaaSObject {
  count: number = 0;
  history: string[] = [];

  /**
   * Increment the counter by the given amount (default 1).
   * Returns the new count.
   */
  @method()
  async increment(amount: number = 1): Promise<number> {
    this.count += amount;
    this.history.push(`+${amount}`);
    this.log("info", `Counter incremented by ${amount} → ${this.count}`);
    return this.count;
  }

  /**
   * Get the current count (read-only, still stateful to verify load cycle).
   */
  @method()
  async getCount(): Promise<number> {
    return this.count;
  }

  /**
   * Reset the counter to zero.
   */
  @method()
  async reset(): Promise<number> {
    const old = this.count;
    this.count = 0;
    this.history = [];
    this.log("info", `Counter reset from ${old}`);
    return old;
  }

  /**
   * A stateless echo that returns the payload as-is.
   * Used to verify stateless method invocations.
   */
  @method({ stateless: true })
  async echo(data: any): Promise<any> {
    return data;
  }

  /**
   * Throws an OaaSError to test application error handling.
   */
  @method()
  async failOnPurpose(message: string = "intentional error"): Promise<void> {
    throw new OaaSError(message);
  }
}

export default Counter;
