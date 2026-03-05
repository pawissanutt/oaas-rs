/**
 * Tests for the PackageForm component.
 *
 * Covers:
 * - Renders all four tabs
 * - Metadata inputs work
 * - Adding/removing functions
 * - Adding/removing classes
 * - Function key dropdown populates from defined functions
 * - JSON/YAML tab content
 * - Validation errors display
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PackageForm } from "@/components/features/package-form";
import { DEFAULT_PACKAGE } from "@/lib/package-schema";
import type { OPackage } from "@/lib/bindings/OPackage";

function makePkg(overrides: Partial<OPackage> = {}): OPackage {
  return {
    ...DEFAULT_PACKAGE,
    ...overrides,
    metadata: { ...DEFAULT_PACKAGE.metadata, ...(overrides.metadata ?? {}) },
  };
}

describe("PackageForm", () => {
  it("renders all four tabs", () => {
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    expect(screen.getByRole("tab", { name: /metadata/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /functions/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /classes/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /json.*yaml/i })).toBeInTheDocument();
  });

  it("renders package name and version inputs in metadata tab", () => {
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    expect(screen.getByLabelText("Package Name *")).toBeInTheDocument();
    expect(screen.getByLabelText("Version")).toBeInTheDocument();
  });

  it("calls onChange when package name is typed", () => {
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Package Name *"), {
      target: { value: "test-pkg" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ name: "test-pkg" }),
    );
  });

  it("calls onChange when version is typed", () => {
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Version"), {
      target: { value: "2.0.0" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ version: "2.0.0" }),
    );
  });

  it("shows metadata fields (author, description)", () => {
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    expect(screen.getByLabelText("Author")).toBeInTheDocument();
    expect(screen.getByLabelText("Description")).toBeInTheDocument();
  });

  it("shows empty state for functions tab", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /functions/i }));

    expect(screen.getByText(/no functions defined/i)).toBeInTheDocument();
  });

  it("adds a function when 'Add Function' is clicked", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /functions/i }));
    await user.click(screen.getByRole("button", { name: /add function/i }));

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        functions: expect.arrayContaining([
          expect.objectContaining({ key: "", function_type: "CUSTOM" }),
        ]),
      }),
    );
  });

  it("shows empty state for classes tab", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /classes/i }));

    expect(screen.getByText(/no classes defined/i)).toBeInTheDocument();
  });

  it("adds a class when 'Add Class' is clicked", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /classes/i }));
    await user.click(screen.getByRole("button", { name: /add class/i }));

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        classes: expect.arrayContaining([
          expect.objectContaining({ key: "" }),
        ]),
      }),
    );
  });

  it("shows badge counts for functions and classes tabs", () => {
    const onChange = vi.fn();
    const pkg = makePkg({
      functions: [
        { key: "fn1", function_type: "CUSTOM", description: null, provision_config: null, config: {} },
        { key: "fn2", function_type: "WASM", description: null, provision_config: null, config: {} },
      ],
      classes: [
        { key: "cls1", description: null, state_spec: null, function_bindings: [] },
      ],
    });
    render(<PackageForm data={pkg} onChange={onChange} />);

    const functionsTab = screen.getByRole("tab", { name: /functions/i });
    expect(within(functionsTab).getByText("2")).toBeInTheDocument();

    const classesTab = screen.getByRole("tab", { name: /classes/i });
    expect(within(classesTab).getByText("1")).toBeInTheDocument();
  });

  it("renders JSON/YAML tab with format switcher", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /json.*yaml/i }));

    // Format switcher buttons should be visible
    expect(screen.getByRole("button", { name: "JSON" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "YAML" })).toBeInTheDocument();
  });

  it("syncs form data to JSON tab content", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const pkg = makePkg({ name: "sync-test", version: "3.0" });
    render(<PackageForm data={pkg} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /json.*yaml/i }));

    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    expect(textarea.value).toContain('"sync-test"');
  });

  it("updates form when JSON is edited in code tab", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /json.*yaml/i }));

    const textarea = screen.getByRole("textbox");
    const newJson = JSON.stringify({ ...DEFAULT_PACKAGE, name: "json-edited" }, null, 2);
    fireEvent.change(textarea, { target: { value: newJson } });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ name: "json-edited" }),
    );
  });

  it("shows parse error for invalid JSON", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg()} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /json.*yaml/i }));

    const textarea = screen.getByRole("textbox");
    fireEvent.change(textarea, { target: { value: "{ invalid json" } });

    expect(screen.getAllByText(/Invalid JSON/i).length).toBeGreaterThan(0);
  });

  it("switches between JSON and YAML formats", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<PackageForm data={makePkg({ name: "format-test" })} onChange={onChange} />);

    await user.click(screen.getByRole("tab", { name: /json.*yaml/i }));

    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    expect(textarea.value).toContain('"format-test"');

    // Switch to YAML
    await user.click(screen.getByRole("button", { name: "YAML" }));
    expect(textarea.value).toContain("name: format-test");
  });
});
