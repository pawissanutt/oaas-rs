"use client";

import { useCallback, useEffect, useRef } from "react";
import Editor, { type OnMount, type Monaco } from "@monaco-editor/react";
import { useTheme } from "next-themes";
import { OAAS_SDK_TYPE_DEFINITIONS } from "@/lib/oaas-sdk-types";

/**
 * Props for the ScriptEditor component.
 */
export interface ScriptEditorProps {
  /** Current source code value. */
  value: string;
  /** Called when the source code changes. */
  onChange: (value: string) => void;
  /** If true, the editor is read-only. */
  readOnly?: boolean;
  /** Minimum height of the editor in pixels. */
  minHeight?: number;
  /** Additional CSS class name. */
  className?: string;
}

/**
 * Monaco-based TypeScript editor with @oaas/sdk IntelliSense support.
 *
 * Registers the SDK type definitions as extra Monaco libs so users get
 * autocomplete, hover docs, and error highlighting for OaaS decorators
 * and base classes.
 */
export function ScriptEditor({
  value,
  onChange,
  readOnly = false,
  minHeight = 400,
  className,
}: ScriptEditorProps) {
  const { resolvedTheme } = useTheme();
  const monacoRef = useRef<Monaco | null>(null);
  const disposableRef = useRef<{ dispose: () => void } | null>(null);

  // Register @oaas/sdk types when Monaco mounts
  const handleBeforeMount = useCallback((monaco: Monaco) => {
    monacoRef.current = monaco;

    // Configure TypeScript compiler options for decorators
    monaco.languages.typescript.typescriptDefaults.setCompilerOptions({
      target: monaco.languages.typescript.ScriptTarget.ES2020,
      module: monaco.languages.typescript.ModuleKind.ESNext,
      moduleResolution:
        monaco.languages.typescript.ModuleResolutionKind.NodeJs,
      allowNonTsExtensions: true,
      experimentalDecorators: true,
      emitDecoratorMetadata: true,
      strict: true,
      jsx: monaco.languages.typescript.JsxEmit.None,
      esModuleInterop: true,
    });

    // Register @oaas/sdk type definitions as extra lib
    disposableRef.current =
      monaco.languages.typescript.typescriptDefaults.addExtraLib(
        OAAS_SDK_TYPE_DEFINITIONS,
        "file:///node_modules/@oaas/sdk/index.d.ts",
      );
  }, []);

  const handleMount: OnMount = useCallback((_editor, _monaco) => {
    // Editor is mounted; focus is handled by the parent if needed
  }, []);

  // Clean up disposable on unmount
  useEffect(() => {
    return () => {
      disposableRef.current?.dispose();
    };
  }, []);

  const monacoTheme = resolvedTheme === "dark" ? "vs-dark" : "light";

  return (
    <div className={className} style={{ minHeight }} data-testid="script-editor">
      <Editor
        height="100%"
        defaultLanguage="typescript"
        language="typescript"
        theme={monacoTheme}
        value={value}
        onChange={(val) => onChange(val ?? "")}
        beforeMount={handleBeforeMount}
        onMount={handleMount}
        options={{
          readOnly,
          minimap: { enabled: false },
          fontSize: 14,
          lineNumbers: "on",
          wordWrap: "on",
          scrollBeyondLastLine: false,
          automaticLayout: true,
          tabSize: 2,
          insertSpaces: true,
          formatOnPaste: true,
          formatOnType: true,
          suggestOnTriggerCharacters: true,
          quickSuggestions: true,
          parameterHints: { enabled: true },
          padding: { top: 8, bottom: 8 },
        }}
      />
    </div>
  );
}
