"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import {
  Code,
  Hammer,
  FileText,
  Plus,
  Loader2,
  ChevronDown,
  ChevronUp,
  RefreshCw,
  FlaskConical,
  PackagePlus,
  Copy,
  Check,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { ScriptEditor } from "@/components/features/script-editor";
import {
  ConsoleOutput,
  createConsoleMessage,
  type ConsoleMessage,
} from "@/components/features/console-output";
import {
  buildScript,
  listScripts,
  getScriptSource,
  type ScriptInfo,
} from "@/lib/scripts-api";
import {
  DEFAULT_SCRIPT_TEMPLATE,
  SCRIPT_TEMPLATES,
} from "@/lib/oaas-sdk-types";
import {
  extractTemplateFromSource,
  generatePackageJson,
} from "@/lib/script-template";
import {
  runTest,
  extractMethodNames,
  type TestResult,
} from "@/lib/script-test-runner";
import { toast } from "sonner";

type RightTab = "build" | "template" | "test";

export default function ScriptsPage() {
  // ----- State -----
  const [source, setSource] = useState(DEFAULT_SCRIPT_TEMPLATE);
  const [scripts, setScripts] = useState<ScriptInfo[]>([]);
  const [selectedScript, setSelectedScript] = useState<ScriptInfo | null>(null);
  const [loadingScripts, setLoadingScripts] = useState(true);
  const [building, setBuilding] = useState(false);
  const [consoleMessages, setConsoleMessages] = useState<ConsoleMessage[]>([]);
  const [showConsole, setShowConsole] = useState(true);
  const [rightTab, setRightTab] = useState<RightTab>("build");

  // Test state
  const [testing, setTesting] = useState(false);
  const [testMethodName, setTestMethodName] = useState("");
  const [testPayload, setTestPayload] = useState("");
  const [testInitialState, setTestInitialState] = useState("{}");
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [carryState, setCarryState] = useState(false);

  // Template copy feedback
  const [copied, setCopied] = useState(false);

  // ----- Derived -----
  const extractedTemplate = useMemo(
    () => extractTemplateFromSource(source),
    [source],
  );
  const packageJson = useMemo(() => {
    if (!extractedTemplate) return null;
    return generatePackageJson(extractedTemplate);
  }, [extractedTemplate]);
  const availableMethods = useMemo(
    () => extractMethodNames(source),
    [source],
  );

  // Auto-select first method for testing
  useEffect(() => {
    if (availableMethods.length > 0 && !availableMethods.includes(testMethodName)) {
      setTestMethodName(availableMethods[0]);
    }
  }, [availableMethods]); // eslint-disable-line react-hooks/exhaustive-deps

  // ----- Console helpers -----
  const log = useCallback(
    (type: "info" | "error" | "success" | "warn", text: string) => {
      setConsoleMessages((prev) => [...prev, createConsoleMessage(type, text)]);
    },
    [],
  );

  const clearConsole = useCallback(() => {
    setConsoleMessages([]);
  }, []);

  // ----- Load existing scripts -----
  useEffect(() => {
    loadScripts();
  }, []);

  const loadScripts = async () => {
    setLoadingScripts(true);
    try {
      const data = await listScripts();
      setScripts(data);
    } catch (e) {
      console.error("Failed to list scripts:", e);
    } finally {
      setLoadingScripts(false);
    }
  };

  // ----- Select a script to re-edit -----
  const handleSelectScript = async (script: ScriptInfo) => {
    setSelectedScript(script);
    log("info", `Loading source for ${script.package}/${script.key}...`);
    try {
      const result = await getScriptSource(script.package, script.key);
      setSource(result.source);
      log("success", `Loaded ${script.package}/${script.key}`);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log("error", `Failed to load source: ${msg}`);
    }
  };

  // ----- New script -----
  const handleNewScript = (templateIndex: number = 0) => {
    setSelectedScript(null);
    setSource(SCRIPT_TEMPLATES[templateIndex].template);
    setTestResult(null);
    log("info", `New script from template: ${SCRIPT_TEMPLATES[templateIndex].name}`);
  };

  // ----- Build (compile + store, no deploy) -----
  const handleBuild = async () => {
    if (!extractedTemplate) {
      log("error", "No @service decorator detected — cannot determine package name and class");
      return;
    }
    const pkgName = extractedTemplate.packageName;
    const clsKey = extractedTemplate.className;

    setBuilding(true);
    log("info", `Building artifact for ${pkgName}/${clsKey}...`);
    try {
      const result = await buildScript({
        source,
        language: "typescript",
        package_name: pkgName,
        class_key: clsKey,
      });

      if (result.success) {
        const sizeKB = result.wasm_size
          ? ` (${(result.wasm_size / 1024).toFixed(1)} KB)`
          : "";
        log("success", `Build successful!${sizeKB}`);
        if (result.artifact_id) {
          log("info", `  Artifact ID: ${result.artifact_id}`);
        }
        if (result.artifact_url) {
          log("info", `  Artifact URL: ${result.artifact_url}`);
        }
        if (result.source_stored) {
          log("info", "  Source code stored");
        }
        toast.success("Build successful", {
          description: `Artifact: ${result.artifact_id?.substring(0, 12)}...`,
        });
        await loadScripts();
      } else {
        log("error", "Build failed:");
        for (const err of result.errors) {
          log("error", `  ${err}`);
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log("error", `Build request failed: ${msg}`);
    } finally {
      setBuilding(false);
    }
  };

  // ----- Test (in-browser) -----
  const handleTest = async () => {
    if (!testMethodName) {
      log("error", "Select a method to test");
      return;
    }

    setTesting(true);
    setTestResult(null);
    log("info", `Testing method: ${testMethodName}...`);

    try {
      let payload: unknown;
      try {
        payload = testPayload.trim() ? JSON.parse(testPayload) : undefined;
      } catch {
        log("error", "Invalid JSON payload");
        setTesting(false);
        return;
      }

      let initialState: Record<string, unknown> = {};
      try {
        if (testInitialState.trim()) {
          initialState = JSON.parse(testInitialState);
        }
      } catch {
        log("error", "Invalid JSON initial state");
        setTesting(false);
        return;
      }

      const result = await runTest({
        source,
        methodName: testMethodName,
        payload,
        initialState,
      });

      setTestResult(result);

      for (const entry of result.logs) {
        log(entry.level === "debug" ? "info" : entry.level, `[sdk] ${entry.message}`);
      }

      if (result.success) {
        log(
          "success",
          `Test passed (${result.durationMs.toFixed(1)}ms)`,
        );
        if (result.result !== undefined) {
          log("info", `  Return: ${JSON.stringify(result.result)}`);
        }
        if (result.finalState && Object.keys(result.finalState).length > 0) {
          log("info", `  State: ${JSON.stringify(result.finalState)}`);
          if (carryState) {
            setTestInitialState(JSON.stringify(result.finalState, null, 2));
          }
        }
      } else {
        log("error", `Test failed: ${result.error}`);
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log("error", `Test error: ${msg}`);
    } finally {
      setTesting(false);
    }
  };

  // ----- Copy template to clipboard -----
  const handleCopyTemplate = async () => {
    if (!packageJson) return;
    await navigator.clipboard.writeText(JSON.stringify(packageJson, null, 2));
    setCopied(true);
    toast.success("Package template copied to clipboard");
    setTimeout(() => setCopied(false), 2000);
  };

  // ----- Use template in New Package -----
  const handleUseInNewPackage = () => {
    if (!packageJson) return;
    sessionStorage.setItem(
      "oprc-package-template",
      JSON.stringify(packageJson, null, 2),
    );
    toast.success("Template saved — go to Packages page to create");
  };

  // ----- Render -----
  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b shrink-0">
        <div className="flex items-center gap-2">
          <Code className="h-5 w-5" />
          <h1 className="text-xl font-bold tracking-tight">Scripts</h1>
          {selectedScript && (
            <Badge variant="secondary">
              {selectedScript.package}/{selectedScript.key}
            </Badge>
          )}
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleTest}
            disabled={testing || !testMethodName}
          >
            {testing ? (
              <Loader2 className="h-4 w-4 mr-1 animate-spin" />
            ) : (
              <FlaskConical className="h-4 w-4 mr-1" />
            )}
            Test
          </Button>
          <Button
            size="sm"
            onClick={handleBuild}
            disabled={building || !extractedTemplate}
          >
            {building ? (
              <Loader2 className="h-4 w-4 mr-1 animate-spin" />
            ) : (
              <Hammer className="h-4 w-4 mr-1" />
            )}
            Build
          </Button>
        </div>
      </div>

      {/* Main area: sidebar + editor + config */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Left sidebar: script list */}
        <div className="w-56 border-r flex flex-col shrink-0 overflow-hidden">
          <div className="p-3 border-b flex items-center justify-between">
            <span className="text-sm font-medium">Functions</span>
            <div className="flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                onClick={() => loadScripts()}
                title="Refresh"
              >
                <RefreshCw className="h-3 w-3" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                onClick={() => handleNewScript(0)}
                title="New Script"
              >
                <Plus className="h-3 w-3" />
              </Button>
            </div>
          </div>

          {/* Template shortcuts */}
          <div className="p-2 border-b space-y-1">
            <p className="text-xs text-muted-foreground px-1 mb-1">Templates</p>
            {SCRIPT_TEMPLATES.map((tmpl, i) => (
              <button
                key={tmpl.name}
                className="w-full text-left text-xs px-2 py-1.5 rounded hover:bg-accent transition-colors"
                onClick={() => handleNewScript(i)}
                title={tmpl.description}
              >
                <FileText className="h-3 w-3 inline mr-1.5 text-muted-foreground" />
                {tmpl.name}
              </button>
            ))}
          </div>

          {/* Existing scripts */}
          <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
            {loadingScripts ? (
              <div className="flex justify-center py-8">
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              </div>
            ) : scripts.length === 0 ? (
              <p className="text-xs text-muted-foreground p-2 text-center">
                No scripts yet
              </p>
            ) : (
              scripts.map((script) => (
                <button
                  key={`${script.package}/${script.key}`}
                  className={`w-full text-left text-xs px-2 py-1.5 rounded transition-colors ${
                    selectedScript?.key === script.key &&
                    selectedScript?.package === script.package
                      ? "bg-accent text-accent-foreground"
                      : "hover:bg-accent/50"
                  }`}
                  onClick={() => handleSelectScript(script)}
                >
                  <Code className="h-3 w-3 inline mr-1.5 text-muted-foreground" />
                  <span className="text-muted-foreground">
                    {script.package}/
                  </span>
                  {script.key}
                </button>
              ))
            )}
          </div>
        </div>

        {/* Center: editor */}
        <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
          <div className="flex-1 min-h-0">
            <ScriptEditor
              value={source}
              onChange={setSource}
              className="h-full"
            />
          </div>

          {/* Bottom: console toggle + output */}
          <div className="border-t shrink-0">
            <button
              className="w-full flex items-center justify-between px-3 py-1.5 text-xs font-medium hover:bg-accent/50 transition-colors"
              onClick={() => setShowConsole(!showConsole)}
            >
              <span>Console ({consoleMessages.length})</span>
              <div className="flex items-center gap-2">
                <button
                  className="text-muted-foreground hover:text-foreground text-xs"
                  onClick={(e) => {
                    e.stopPropagation();
                    clearConsole();
                  }}
                >
                  Clear
                </button>
                {showConsole ? (
                  <ChevronDown className="h-3 w-3" />
                ) : (
                  <ChevronUp className="h-3 w-3" />
                )}
              </div>
            </button>
            {showConsole && (
              <ConsoleOutput
                messages={consoleMessages}
                className="h-40 border-t"
              />
            )}
          </div>
        </div>

        {/* Right panel: tabbed (Build / Template / Test) */}
        <div className="w-80 border-l shrink-0 flex flex-col overflow-hidden">
          {/* Tab bar */}
          <div className="flex border-b shrink-0">
            {(
              [
                { key: "build", label: "Build", icon: Hammer },
                { key: "template", label: "Template", icon: PackagePlus },
                { key: "test", label: "Test", icon: FlaskConical },
              ] as const
            ).map((tab) => (
              <button
                key={tab.key}
                className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-2.5 text-xs font-medium transition-colors ${
                  rightTab === tab.key
                    ? "border-b-2 border-primary text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                onClick={() => setRightTab(tab.key)}
              >
                <tab.icon className="h-3.5 w-3.5" />
                {tab.label}
              </button>
            ))}
          </div>

          {/* Tab content */}
          <div className="flex-1 overflow-y-auto p-4">
            {/* ---- Build Config Tab ---- */}
            {rightTab === "build" && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-sm font-semibold mb-3">Build Configuration</h3>
                  <p className="text-xs text-muted-foreground mb-4">
                    Compile your script into a WASM artifact. The artifact can
                    later be referenced when creating a package.
                  </p>
                </div>
                {extractedTemplate ? (
                  <div className="p-3 bg-muted/50 rounded-md text-xs space-y-1">
                    <p className="font-medium text-muted-foreground">
                      Detected from decorators:
                    </p>
                    <p>Package: <code className="font-mono">{extractedTemplate.packageName}</code></p>
                    <p>Class: <code className="font-mono">{extractedTemplate.className}</code></p>
                    <p>Methods: {extractedTemplate.methods.map((m) => m.name).join(", ") || "none"}</p>
                  </div>
                ) : (
                  <div className="p-3 bg-muted/50 rounded-md text-xs text-muted-foreground">
                    Add <code>@service("name")</code> to your class to enable building.
                  </div>
                )}
                <Button
                  className="w-full"
                  onClick={handleBuild}
                  disabled={building || !extractedTemplate}
                >
                  {building ? (
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  ) : (
                    <Hammer className="h-4 w-4 mr-2" />
                  )}
                  Build Artifact
                </Button>
              </div>
            )}

            {/* ---- Template Tab ---- */}
            {rightTab === "template" && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-sm font-semibold mb-1">Package Template</h3>
                  <p className="text-xs text-muted-foreground mb-3">
                    Auto-generated from <code>@service</code> and{" "}
                    <code>@method</code> decorators in your script.
                  </p>
                </div>
                {packageJson ? (
                  <>
                    <pre className="p-3 bg-muted/50 rounded-md text-xs font-mono overflow-auto max-h-80 whitespace-pre-wrap">
                      {JSON.stringify(packageJson, null, 2)}
                    </pre>
                    <div className="flex gap-2">
                      <Button
                        variant="outline"
                        size="sm"
                        className="flex-1"
                        onClick={handleCopyTemplate}
                      >
                        {copied ? (
                          <Check className="h-3.5 w-3.5 mr-1.5" />
                        ) : (
                          <Copy className="h-3.5 w-3.5 mr-1.5" />
                        )}
                        {copied ? "Copied!" : "Copy JSON"}
                      </Button>
                      <Button
                        size="sm"
                        className="flex-1"
                        onClick={handleUseInNewPackage}
                      >
                        <PackagePlus className="h-3.5 w-3.5 mr-1.5" />
                        Use in Package
                      </Button>
                    </div>
                  </>
                ) : (
                  <div className="text-center py-8">
                    <p className="text-sm text-muted-foreground">
                      No <code>@service</code> decorator detected.
                    </p>
                    <p className="text-xs text-muted-foreground mt-1">
                      Add <code>@service(&quot;Name&quot;)</code> to your class to
                      generate a template.
                    </p>
                  </div>
                )}
              </div>
            )}

            {/* ---- Test Tab ---- */}
            {rightTab === "test" && (
              <div className="space-y-4">
                <div>
                  <h3 className="text-sm font-semibold mb-1">Test Runner</h3>
                  <p className="text-xs text-muted-foreground mb-3">
                    Run your script methods using the real SDK on the compiler
                    service. Results are consistent with WASM deployment.
                  </p>
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="test-method">Method</Label>
                  {availableMethods.length > 0 ? (
                    <select
                      id="test-method"
                      className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm"
                      value={testMethodName}
                      onChange={(e) => setTestMethodName(e.target.value)}
                    >
                      {availableMethods.map((m) => (
                        <option key={m} value={m}>
                          {m}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      No @method decorators found in source.
                    </p>
                  )}
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="test-payload">Payload (JSON)</Label>
                  <p className="text-xs text-muted-foreground">
                    Passed as the 1st argument. Use <code>10</code> for <code>fn(amount)</code>,{" "}
                    <code>{'{"a":1}'}</code> for <code>fn(args)</code>.
                  </p>
                  <textarea
                    id="test-payload"
                    className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono resize-none h-20"
                    value={testPayload}
                    onChange={(e) => setTestPayload(e.target.value)}
                    placeholder="10"
                    spellCheck={false}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label htmlFor="test-state">Initial State (JSON)</Label>
                  <textarea
                    id="test-state"
                    className="w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono resize-none h-20"
                    value={testInitialState}
                    onChange={(e) => setTestInitialState(e.target.value)}
                    placeholder='{"count": 0}'
                    spellCheck={false}
                  />
                </div>
                <label className="flex items-center gap-2 text-xs cursor-pointer">
                  <input
                    type="checkbox"
                    checked={carryState}
                    onChange={(e) => setCarryState(e.target.checked)}
                    className="rounded border-input"
                  />
                  Carry final state to next run
                </label>
                <Button
                  className="w-full"
                  variant="outline"
                  onClick={handleTest}
                  disabled={testing || !testMethodName}
                >
                  {testing ? (
                    <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                  ) : (
                    <FlaskConical className="h-4 w-4 mr-2" />
                  )}
                  Run Test
                </Button>
                {testResult && (
                  <div
                    className={`p-3 rounded-md text-xs space-y-2 ${
                      testResult.success
                        ? "bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800"
                        : "bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="font-semibold">
                        {testResult.success ? "PASS" : "FAIL"}
                      </span>
                      <span className="text-muted-foreground">
                        {testResult.durationMs.toFixed(1)}ms
                      </span>
                    </div>
                    {testResult.success && testResult.result !== undefined && (
                      <div>
                        <p className="text-muted-foreground mb-0.5">Return:</p>
                        <pre className="font-mono bg-background/50 p-1.5 rounded overflow-auto">
                          {JSON.stringify(testResult.result, null, 2)}
                        </pre>
                      </div>
                    )}
                    {testResult.success &&
                      testResult.finalState &&
                      Object.keys(testResult.finalState).length > 0 && (
                        <div>
                          <p className="text-muted-foreground mb-0.5">Final State:</p>
                          <pre className="font-mono bg-background/50 p-1.5 rounded overflow-auto">
                            {JSON.stringify(testResult.finalState, null, 2)}
                          </pre>
                        </div>
                      )}
                    {testResult.error && (
                      <pre className="font-mono text-red-600 dark:text-red-400 whitespace-pre-wrap">
                        {testResult.error}
                      </pre>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
