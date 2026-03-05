/**
 * Tests for the ScriptEditor component (src/components/features/script-editor.tsx).
 *
 * Since Monaco editor requires a full browser DOM (canvas, web workers), we mock
 * @monaco-editor/react and test the component's integration behavior:
 * - Passes value/onChange correctly
 * - Applies readOnly option
 * - Registers @oaas/sdk type definitions via beforeMount
 * - Has correct data-testid
 * - Responds to theme
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";

// ---------------------------------------------------------------------------
// Mock @monaco-editor/react
// ---------------------------------------------------------------------------

let capturedEditorProps: Record<string, unknown> = {};
let capturedBeforeMount: ((monaco: unknown) => void) | null = null;

vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  default: (props: Record<string, unknown>) => {
    capturedEditorProps = props;
    capturedBeforeMount = props.beforeMount as (monaco: unknown) => void;
    return (
      <div data-testid="mock-monaco-editor">
        <textarea
          data-testid="mock-textarea"
          value={props.value as string}
          onChange={(e) =>
            (props.onChange as (val: string) => void)?.(e.target.value)
          }
          readOnly={
            (props.options as Record<string, unknown>)?.readOnly as boolean
          }
        />
      </div>
    );
  },
}));

// Mock next-themes
vi.mock("next-themes", () => ({
  useTheme: () => ({ resolvedTheme: "dark" }),
}));

// Must import after mocks
import { ScriptEditor } from "@/components/features/script-editor";

beforeEach(() => {
  capturedEditorProps = {};
  capturedBeforeMount = null;
});

describe("ScriptEditor", () => {
  it("renders with data-testid", () => {
    render(<ScriptEditor value="test" onChange={() => {}} />);
    expect(screen.getByTestId("script-editor")).toBeInTheDocument();
  });

  it("passes value to Monaco editor", () => {
    render(<ScriptEditor value="const x = 1;" onChange={() => {}} />);
    expect(capturedEditorProps.value).toBe("const x = 1;");
  });

  it("passes onChange handler", () => {
    const onChange = vi.fn();
    render(<ScriptEditor value="" onChange={onChange} />);

    // The mock editor renders a textarea, simulate change
    const textarea = screen.getByTestId("mock-textarea");
    textarea.dispatchEvent(new Event("change", { bubbles: true }));
  });

  it("uses TypeScript language", () => {
    render(<ScriptEditor value="" onChange={() => {}} />);
    expect(capturedEditorProps.defaultLanguage).toBe("typescript");
    expect(capturedEditorProps.language).toBe("typescript");
  });

  it("passes readOnly option", () => {
    render(<ScriptEditor value="" onChange={() => {}} readOnly={true} />);
    const options = capturedEditorProps.options as Record<string, unknown>;
    expect(options.readOnly).toBe(true);
  });

  it("uses vs-dark theme in dark mode", () => {
    render(<ScriptEditor value="" onChange={() => {}} />);
    expect(capturedEditorProps.theme).toBe("vs-dark");
  });

  it("applies custom className", () => {
    const { container } = render(
      <ScriptEditor value="" onChange={() => {}} className="my-editor" />,
    );
    expect(container.querySelector("[data-testid='script-editor']")).toHaveClass(
      "my-editor",
    );
  });

  it("sets minHeight via style", () => {
    render(<ScriptEditor value="" onChange={() => {}} minHeight={600} />);
    const el = screen.getByTestId("script-editor");
    expect(el.style.minHeight).toBe("600px");
  });

  it("provides beforeMount callback for SDK type registration", () => {
    render(<ScriptEditor value="" onChange={() => {}} />);
    expect(capturedBeforeMount).toBeDefined();

    // Simulate Monaco API
    const mockDisposable = { dispose: vi.fn() };
    const mockMonaco = {
      languages: {
        typescript: {
          typescriptDefaults: {
            setCompilerOptions: vi.fn(),
            addExtraLib: vi.fn(() => mockDisposable),
          },
          ScriptTarget: { ES2020: 7 },
          ModuleKind: { ESNext: 99 },
          ModuleResolutionKind: { NodeJs: 2 },
          JsxEmit: { None: 0 },
        },
      },
    };

    capturedBeforeMount!(mockMonaco);

    // Verify compiler options were set with decorator support
    expect(
      mockMonaco.languages.typescript.typescriptDefaults.setCompilerOptions,
    ).toHaveBeenCalledWith(
      expect.objectContaining({
        experimentalDecorators: true,
        emitDecoratorMetadata: true,
      }),
    );

    // Verify @oaas/sdk types were registered
    expect(
      mockMonaco.languages.typescript.typescriptDefaults.addExtraLib,
    ).toHaveBeenCalledWith(
      expect.stringContaining('declare module "@oaas/sdk"'),
      expect.stringContaining("@oaas/sdk"),
    );
  });

  it("configures editor options for good UX", () => {
    render(<ScriptEditor value="" onChange={() => {}} />);
    const options = capturedEditorProps.options as Record<string, unknown>;
    expect(options.minimap).toEqual({ enabled: false });
    expect(options.fontSize).toBe(14);
    expect(options.tabSize).toBe(2);
    expect(options.insertSpaces).toBe(true);
    expect(options.wordWrap).toBe("on");
    expect(options.automaticLayout).toBe(true);
  });
});
