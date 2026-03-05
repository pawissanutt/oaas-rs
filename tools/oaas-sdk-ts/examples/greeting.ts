/**
 * Greeting — stateless OaaS service example.
 *
 * Demonstrates:
 * - Stateless methods (no object state, pure compute)
 * - Multiple method exports
 * - Simple JSON input/output
 */

import { service, method, OaaSObject } from "../src/index.js";

@service("Greeting", { package: "example" })
class Greeting extends OaaSObject {
  /**
   * Generate a greeting message.
   */
  @method({ stateless: true })
  async greet(name: string): Promise<string> {
    return `Hello, ${name}! Welcome to OaaS.`;
  }

  /**
   * Generate a farewell message.
   */
  @method({ stateless: true })
  async farewell(name: string): Promise<string> {
    return `Goodbye, ${name}! See you next time.`;
  }

  /**
   * Generate a personalized greeting with language support.
   */
  @method({ stateless: true })
  async greetLocalized(args: {
    name: string;
    language?: string;
  }): Promise<string> {
    const lang = args.language ?? "en";
    switch (lang) {
      case "en":
        return `Hello, ${args.name}!`;
      case "es":
        return `¡Hola, ${args.name}!`;
      case "fr":
        return `Bonjour, ${args.name}!`;
      case "de":
        return `Hallo, ${args.name}!`;
      case "ja":
        return `こんにちは、${args.name}さん！`;
      default:
        return `Hello, ${args.name}! (${lang})`;
    }
  }

  /**
   * Echo back the input — useful for testing.
   */
  @method({ stateless: true })
  async echo(input: unknown): Promise<unknown> {
    return input;
  }
}

export default Greeting;
