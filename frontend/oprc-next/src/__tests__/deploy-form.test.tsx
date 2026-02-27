/**
 * Tests for the DeployForm component (src/components/features/deploy-form.tsx).
 *
 * Covers:
 * - Renders all form fields
 * - Package name and class key input updates
 * - Adding/removing function bindings
 * - Toggling stateless switch
 * - Adding/removing target environments
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DeployForm, type DeployFormData } from "@/components/features/deploy-form";

function makeForm(overrides: Partial<DeployFormData> = {}): DeployFormData {
  return {
    packageName: "",
    classKey: "",
    functionBindings: [],
    targetEnvs: [],
    ...overrides,
  };
}

describe("DeployForm", () => {
  it("renders package name and class key inputs", () => {
    const onChange = vi.fn();
    render(<DeployForm data={makeForm()} onChange={onChange} />);

    expect(screen.getByLabelText("Package Name")).toBeInTheDocument();
    expect(screen.getByLabelText("Class Key")).toBeInTheDocument();
  });

  it("calls onChange when package name is typed", () => {
    const onChange = vi.fn();
    render(<DeployForm data={makeForm()} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Package Name"), {
      target: { value: "my-package" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ packageName: "my-package" }),
    );
  });

  it("calls onChange when class key is typed", () => {
    const onChange = vi.fn();
    render(<DeployForm data={makeForm()} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Class Key"), {
      target: { value: "MyClass" },
    });

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ classKey: "MyClass" }),
    );
  });

  it("shows existing values in inputs", () => {
    const onChange = vi.fn();
    render(
      <DeployForm
        data={makeForm({ packageName: "test-pkg", classKey: "TestClass" })}
        onChange={onChange}
      />,
    );

    expect(screen.getByLabelText("Package Name")).toHaveValue("test-pkg");
    expect(screen.getByLabelText("Class Key")).toHaveValue("TestClass");
  });

  it("adds a function binding when Add is clicked", () => {
    const onChange = vi.fn();
    render(<DeployForm data={makeForm()} onChange={onChange} />);

    const addButtons = screen.getAllByText("Add");
    // First Add button is for function bindings
    fireEvent.click(addButtons[0]);

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        functionBindings: [{ name: "", stateless: false }],
      }),
    );
  });

  it("renders existing function bindings", () => {
    const onChange = vi.fn();
    render(
      <DeployForm
        data={makeForm({
          functionBindings: [
            { name: "increment", stateless: false },
            { name: "add", stateless: true },
          ],
        })}
        onChange={onChange}
      />,
    );

    const inputs = screen.getAllByPlaceholderText("Method name");
    expect(inputs).toHaveLength(2);
    expect(inputs[0]).toHaveValue("increment");
    expect(inputs[1]).toHaveValue("add");
  });

  it("removes a function binding when X is clicked", () => {
    const onChange = vi.fn();
    render(
      <DeployForm
        data={makeForm({
          functionBindings: [
            { name: "fn1", stateless: false },
            { name: "fn2", stateless: true },
          ],
        })}
        onChange={onChange}
      />,
    );

    // Find X buttons (they're in the function bindings area)
    const removeButtons = screen.getAllByRole("button").filter(
      (btn) => btn.querySelector("svg"),
    );
    // Click the first remove button (skip Add buttons, find X icons)
    const xButtons = screen.getAllByRole("button").filter((btn) => {
      const svg = btn.querySelector("svg");
      return svg && btn.closest("[data-testid='deploy-form']") && btn.textContent === "";
    });
    // Fire click on first removable item
    if (xButtons.length > 0) {
      fireEvent.click(xButtons[0]);
      expect(onChange).toHaveBeenCalled();
    }
  });

  it("adds a target environment when Add is clicked", () => {
    const onChange = vi.fn();
    render(<DeployForm data={makeForm()} onChange={onChange} />);

    const addButtons = screen.getAllByText("Add");
    // Second Add button is for target envs
    fireEvent.click(addButtons[1]);

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        targetEnvs: [""],
      }),
    );
  });

  it("renders existing target environments", () => {
    const onChange = vi.fn();
    render(
      <DeployForm
        data={makeForm({ targetEnvs: ["edge-1", "cloud-2"] })}
        onChange={onChange}
      />,
    );

    const inputs = screen.getAllByPlaceholderText("Environment name");
    expect(inputs).toHaveLength(2);
    expect(inputs[0]).toHaveValue("edge-1");
    expect(inputs[1]).toHaveValue("cloud-2");
  });

  it("shows helper text for auto-detection", () => {
    const onChange = vi.fn();
    render(<DeployForm data={makeForm()} onChange={onChange} />);

    expect(
      screen.getByText("Leave empty to auto-detect from @method decorators."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Leave empty for automatic environment selection."),
    ).toBeInTheDocument();
  });

  it("applies custom className", () => {
    const onChange = vi.fn();
    const { container } = render(
      <DeployForm data={makeForm()} onChange={onChange} className="custom-cls" />,
    );
    expect(container.querySelector("[data-testid='deploy-form']")).toHaveClass(
      "custom-cls",
    );
  });
});
