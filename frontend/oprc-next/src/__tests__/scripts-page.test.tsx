/**
 * Tests for the Scripts page (src/app/scripts/page.tsx).
 *
 * Covers:
 * - Page renders with header, sidebar, editor, and deploy panel
 * - Compile button triggers API call and shows console output
 * - Deploy button validates required fields
 * - Template selection changes editor content
 * - Script list loads and displays items
 * - Console toggle works
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

// Mock Monaco editor
vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  default: (props: Record<string, unknown>) => (
    <div data-testid="mock-monaco-editor">
      <textarea
        data-testid="mock-editor-textarea"
        value={props.value as string}
        onChange={(e) =>
          (props.onChange as (val: string) => void)?.(e.target.value)
        }
      />
    </div>
  ),
}));

// Mock next-themes
vi.mock("next-themes", () => ({
  useTheme: () => ({ resolvedTheme: "dark" }),
}));

// Mock scripts-api
const mockCompileScript = vi.fn();
const mockDeployScript = vi.fn();
const mockListScripts = vi.fn();
const mockGetScriptSource = vi.fn();

vi.mock("@/lib/scripts-api", () => ({
  compileScript: (...args: unknown[]) => mockCompileScript(...args),
  deployScript: (...args: unknown[]) => mockDeployScript(...args),
  listScripts: (...args: unknown[]) => mockListScripts(...args),
  getScriptSource: (...args: unknown[]) => mockGetScriptSource(...args),
}));

import ScriptsPage from "@/app/scripts/page";

beforeEach(() => {
  vi.clearAllMocks();
  mockListScripts.mockResolvedValue([]);
});

describe("ScriptsPage", () => {
  it("renders page header with title", async () => {
    render(<ScriptsPage />);
    expect(screen.getByText("Scripts")).toBeInTheDocument();
  });

  it("renders Compile and Deploy buttons", async () => {
    render(<ScriptsPage />);
    expect(screen.getByText("Compile")).toBeInTheDocument();
    expect(screen.getByText("Deploy")).toBeInTheDocument();
  });

  it("renders the Monaco editor", async () => {
    render(<ScriptsPage />);
    expect(screen.getByTestId("mock-monaco-editor")).toBeInTheDocument();
  });

  it("renders the deploy configuration panel", async () => {
    render(<ScriptsPage />);
    expect(screen.getByText("Deploy Configuration")).toBeInTheDocument();
    expect(screen.getByLabelText("Package Name")).toBeInTheDocument();
    expect(screen.getByLabelText("Class Key")).toBeInTheDocument();
  });

  it("renders templates in sidebar", async () => {
    render(<ScriptsPage />);
    expect(screen.getByText("Templates")).toBeInTheDocument();
    expect(screen.getByText("Blank Service")).toBeInTheDocument();
    expect(screen.getByText("Counter")).toBeInTheDocument();
    expect(screen.getByText("Greeting")).toBeInTheDocument();
  });

  it("renders console panel", async () => {
    render(<ScriptsPage />);
    expect(screen.getByTestId("console-output")).toBeInTheDocument();
  });

  it("loads script list on mount", async () => {
    mockListScripts.mockResolvedValue([
      { key: "counter", package: "example", hasSource: true },
    ]);

    render(<ScriptsPage />);

    await waitFor(() => {
      expect(mockListScripts).toHaveBeenCalledTimes(1);
    });
  });

  it("displays existing scripts in sidebar", async () => {
    mockListScripts.mockResolvedValue([
      { key: "counter", package: "example", hasSource: true },
      { key: "greeting", package: "example", hasSource: true },
    ]);

    render(<ScriptsPage />);

    await waitFor(() => {
      expect(screen.getByText("counter")).toBeInTheDocument();
      expect(screen.getByText("greeting")).toBeInTheDocument();
    });
  });

  it("calls compileScript when Compile button is clicked", async () => {
    mockCompileScript.mockResolvedValue({
      success: true,
      wasm_size: 1024,
      errors: [],
    });

    render(<ScriptsPage />);

    await act(async () => {
      fireEvent.click(screen.getByText("Compile"));
    });

    await waitFor(() => {
      expect(mockCompileScript).toHaveBeenCalledTimes(1);
    });
  });

  it("shows compilation success in console", async () => {
    mockCompileScript.mockResolvedValue({
      success: true,
      wasm_size: 10240,
      errors: [],
    });

    render(<ScriptsPage />);

    await act(async () => {
      fireEvent.click(screen.getByText("Compile"));
    });

    await waitFor(() => {
      expect(screen.getByText(/Compilation successful/)).toBeInTheDocument();
    });
  });

  it("shows compilation errors in console", async () => {
    mockCompileScript.mockResolvedValue({
      success: false,
      errors: ["Type error at line 5"],
    });

    render(<ScriptsPage />);

    await act(async () => {
      fireEvent.click(screen.getByText("Compile"));
    });

    await waitFor(() => {
      expect(screen.getByText("Compilation failed:")).toBeInTheDocument();
      expect(screen.getByText(/Type error at line 5/)).toBeInTheDocument();
    });
  });

  it("shows validation error when deploying without package name", async () => {
    render(<ScriptsPage />);

    await act(async () => {
      fireEvent.click(screen.getByText("Deploy"));
    });

    await waitFor(() => {
      expect(
        screen.getByText("Package name is required for deployment"),
      ).toBeInTheDocument();
    });

    expect(mockDeployScript).not.toHaveBeenCalled();
  });

  it("shows validation error when deploying without class key", async () => {
    render(<ScriptsPage />);

    // Set package name first
    fireEvent.change(screen.getByLabelText("Package Name"), {
      target: { value: "my-pkg" },
    });

    await act(async () => {
      fireEvent.click(screen.getByText("Deploy"));
    });

    await waitFor(() => {
      expect(
        screen.getByText("Class key is required for deployment"),
      ).toBeInTheDocument();
    });
  });

  it("calls deployScript with correct config when both fields are set", async () => {
    mockDeployScript.mockResolvedValue({
      success: true,
      deployment_key: "pkg/Cls",
      errors: [],
    });
    // Need scripts refresh after deploy
    mockListScripts.mockResolvedValue([]);

    render(<ScriptsPage />);

    // Fill in required fields
    fireEvent.change(screen.getByLabelText("Package Name"), {
      target: { value: "my-pkg" },
    });
    fireEvent.change(screen.getByLabelText("Class Key"), {
      target: { value: "MyClass" },
    });

    await act(async () => {
      fireEvent.click(screen.getByText("Deploy"));
    });

    await waitFor(() => {
      expect(mockDeployScript).toHaveBeenCalledWith(
        expect.objectContaining({
          language: "typescript",
          package_name: "my-pkg",
          class_key: "MyClass",
        }),
      );
    });
  });

  it("shows deployment success message in console", async () => {
    mockDeployScript.mockResolvedValue({
      success: true,
      deployment_key: "pkg/Cls",
      artifact_url: "http://pm/artifact/abc",
      message: "All good",
      errors: [],
    });
    mockListScripts.mockResolvedValue([]);

    render(<ScriptsPage />);

    fireEvent.change(screen.getByLabelText("Package Name"), {
      target: { value: "pkg" },
    });
    fireEvent.change(screen.getByLabelText("Class Key"), {
      target: { value: "Cls" },
    });

    await act(async () => {
      fireEvent.click(screen.getByText("Deploy"));
    });

    await waitFor(() => {
      expect(
        screen.getByText("Deployment successful!"),
      ).toBeInTheDocument();
    });
  });

  it("loads script source when a script is clicked in sidebar", async () => {
    mockListScripts.mockResolvedValue([
      { key: "counter", package: "example", hasSource: true },
    ]);
    mockGetScriptSource.mockResolvedValue({
      package: "example",
      function: "counter",
      source: "class Counter {}",
      language: "typescript",
    });

    render(<ScriptsPage />);

    // Wait for scripts to load
    await waitFor(() => {
      expect(screen.getByText("counter")).toBeInTheDocument();
    });

    await act(async () => {
      fireEvent.click(screen.getByText("counter"));
    });

    await waitFor(() => {
      expect(mockGetScriptSource).toHaveBeenCalledWith("example", "counter");
    });
  });

  it("changes editor content when a template is selected", async () => {
    render(<ScriptsPage />);

    await act(async () => {
      fireEvent.click(screen.getByText("Counter"));
    });

    // The editor textarea should now have counter template content
    const textarea = screen.getByTestId("mock-editor-textarea");
    expect((textarea as HTMLTextAreaElement).value).toContain("Counter");
  });

  it("toggles console visibility", async () => {
    render(<ScriptsPage />);

    // Console should be visible initially
    expect(screen.getByTestId("console-output")).toBeInTheDocument();

    // Click the console toggle (the button that shows "Console (N)")
    const consoleToggles = screen.getAllByText(/Console \(/);
    fireEvent.click(consoleToggles[0]);

    // Console should be hidden
    expect(screen.queryByTestId("console-output")).not.toBeInTheDocument();

    // Click again to show
    const consoleToggles2 = screen.getAllByText(/Console \(/);
    fireEvent.click(consoleToggles2[0]);

    expect(screen.getByTestId("console-output")).toBeInTheDocument();
  });

  it("shows 'No scripts deployed yet' when list is empty", async () => {
    mockListScripts.mockResolvedValue([]);

    render(<ScriptsPage />);

    await waitFor(() => {
      expect(screen.getByText("No scripts deployed yet")).toBeInTheDocument();
    });
  });
});
