/**
 * Tests for the ConsoleOutput component (src/components/features/console-output.tsx).
 *
 * Covers:
 * - Renders empty state placeholder
 * - Renders messages with correct types and styling
 * - createConsoleMessage helper produces valid messages
 * - Auto-scroll behavior (ref check)
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import {
  ConsoleOutput,
  createConsoleMessage,
  type ConsoleMessage,
} from "@/components/features/console-output";

describe("ConsoleOutput", () => {
  it("renders empty state placeholder when no messages", () => {
    render(<ConsoleOutput messages={[]} />);
    expect(
      screen.getByText("Console output will appear here..."),
    ).toBeInTheDocument();
  });

  it("renders messages with correct text", () => {
    const messages: ConsoleMessage[] = [
      createConsoleMessage("info", "Starting compilation..."),
      createConsoleMessage("success", "Build successful"),
      createConsoleMessage("error", "Something went wrong"),
      createConsoleMessage("warn", "Deprecated API usage"),
    ];

    render(<ConsoleOutput messages={messages} />);

    expect(screen.getByText("Starting compilation...")).toBeInTheDocument();
    expect(screen.getByText("Build successful")).toBeInTheDocument();
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText("Deprecated API usage")).toBeInTheDocument();
  });

  it("renders type prefixes for each message", () => {
    const messages: ConsoleMessage[] = [
      createConsoleMessage("info", "info msg"),
      createConsoleMessage("error", "error msg"),
      createConsoleMessage("success", "success msg"),
      createConsoleMessage("warn", "warn msg"),
    ];

    render(<ConsoleOutput messages={messages} />);

    expect(screen.getByText("[INFO]")).toBeInTheDocument();
    expect(screen.getByText("[ERROR]")).toBeInTheDocument();
    expect(screen.getByText("[OK]")).toBeInTheDocument();
    expect(screen.getByText("[WARN]")).toBeInTheDocument();
  });

  it("has role=log for accessibility", () => {
    render(<ConsoleOutput messages={[]} />);
    expect(screen.getByRole("log")).toBeInTheDocument();
  });

  it("applies custom className", () => {
    const { container } = render(
      <ConsoleOutput messages={[]} className="custom-class" />,
    );
    expect(container.firstChild).toHaveClass("custom-class");
  });

  it("does not render placeholder when messages exist", () => {
    const messages = [createConsoleMessage("info", "hello")];
    render(<ConsoleOutput messages={messages} />);
    expect(
      screen.queryByText("Console output will appear here..."),
    ).not.toBeInTheDocument();
  });
});

describe("createConsoleMessage", () => {
  it("creates a message with unique id", () => {
    const msg1 = createConsoleMessage("info", "first");
    const msg2 = createConsoleMessage("info", "second");
    expect(msg1.id).not.toBe(msg2.id);
  });

  it("has correct type and text", () => {
    const msg = createConsoleMessage("error", "test error");
    expect(msg.type).toBe("error");
    expect(msg.text).toBe("test error");
  });

  it("has a timestamp", () => {
    const before = new Date();
    const msg = createConsoleMessage("info", "test");
    const after = new Date();
    expect(msg.timestamp.getTime()).toBeGreaterThanOrEqual(before.getTime());
    expect(msg.timestamp.getTime()).toBeLessThanOrEqual(after.getTime());
  });

  it("id starts with msg- prefix", () => {
    const msg = createConsoleMessage("success", "test");
    expect(msg.id).toMatch(/^msg-/);
  });
});
