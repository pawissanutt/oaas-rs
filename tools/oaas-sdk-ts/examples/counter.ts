/**
 * Counter — stateful OaaS service example.
 *
 * Demonstrates:
 * - Typed state fields with auto-persistence
 * - Stateful methods that mutate object fields
 * - Stateless pure-compute methods
 * - Application-level error handling with OaaSError
 * - Cross-object access
 * - Structured logging
 */

import { service, method, getter, setter, OaaSObject, OaaSError } from "../src/index.js";

@service("Counter", { package: "example" })
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
    this.history.push(`Added ${amount}`);
    this.log("info", `Counter incremented by ${amount} → ${this.count}`);
    return this.count;
  }

  /**
   * Decrement the counter. Throws if the result would be negative.
   */
  @method()
  async decrement(amount: number = 1): Promise<number> {
    if (this.count - amount < 0) {
      throw new OaaSError(
        `Cannot decrement by ${amount}: current count is ${this.count}`
      );
    }
    this.count -= amount;
    this.history.push(`Removed ${amount}`);
    return this.count;
  }

  /**
   * Reset the counter to zero and clear history.
   */
  @method()
  async reset(): Promise<void> {
    this.count = 0;
    this.history = [];
    this.log("info", "Counter reset");
  }

  /**
   * Transfer this counter's value to another counter object.
   */
  @method()
  async transfer(targetRef: string): Promise<{ transferred: number }> {
    const target = this.object(targetRef);
    await target.invoke("increment", this.count);
    const transferred = this.count;
    this.count = 0;
    this.history.push(`Transferred ${transferred} to ${targetRef}`);
    this.log("info", `Transferred ${transferred} to ${target.ref.toString()}`);
    return { transferred };
  }

  /**
   * Pure computation — add two numbers without touching state.
   */
  @method({ stateless: true })
  async add(args: { a: number; b: number }): Promise<number> {
    return args.a + args.b;
  }

  @getter("count")
  getCount(): number {
    return this.count;
  }

  @setter("count")
  setCount(value: number): void {
    this.count = value;
  }
}

export default Counter;
