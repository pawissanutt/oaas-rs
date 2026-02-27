"use client";

import { useState, useEffect, useCallback } from "react";
import {
  Code,
  Play,
  Rocket,
  FileText,
  Plus,
  Loader2,
  ChevronDown,
  ChevronUp,
  RefreshCw,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScriptEditor } from "@/components/features/script-editor";
import {
  ConsoleOutput,
  createConsoleMessage,
  type ConsoleMessage,
} from "@/components/features/console-output";
import { DeployForm, type DeployFormData } from "@/components/features/deploy-form";
import {
  compileScript,
  deployScript,
  listScripts,
  getScriptSource,
  type ScriptInfo,
} from "@/lib/scripts-api";
import {
  DEFAULT_SCRIPT_TEMPLATE,
  SCRIPT_TEMPLATES,
} from "@/lib/oaas-sdk-types";

export default function ScriptsPage() {
  // ----- State -----
  const [source, setSource] = useState(DEFAULT_SCRIPT_TEMPLATE);
  const [scripts, setScripts] = useState<ScriptInfo[]>([]);
  const [selectedScript, setSelectedScript] = useState<ScriptInfo | null>(null);
  const [loadingScripts, setLoadingScripts] = useState(true);
  const [compiling, setCompiling] = useState(false);
  const [deploying, setDeploying] = useState(false);
  const [consoleMessages, setConsoleMessages] = useState<ConsoleMessage[]>([]);
  const [showConsole, setShowConsole] = useState(true);
  const [deployForm, setDeployForm] = useState<DeployFormData>({
    packageName: "",
    classKey: "",
    functionBindings: [],
    targetEnvs: [],
  });

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
      setDeployForm((prev) => ({
        ...prev,
        packageName: script.package,
        classKey: script.key,
      }));
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
    setDeployForm({
      packageName: "",
      classKey: "",
      functionBindings: [],
      targetEnvs: [],
    });
    log("info", `New script from template: ${SCRIPT_TEMPLATES[templateIndex].name}`);
  };

  // ----- Compile -----
  const handleCompile = async () => {
    setCompiling(true);
    log("info", "Compiling script...");
    try {
      const result = await compileScript(source);
      if (result.success) {
        const sizeKB = result.wasm_size
          ? ` (${(result.wasm_size / 1024).toFixed(1)} KB)`
          : "";
        log("success", `Compilation successful${sizeKB}`);
      } else {
        log("error", "Compilation failed:");
        for (const err of result.errors) {
          log("error", `  ${err}`);
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log("error", `Compile request failed: ${msg}`);
    } finally {
      setCompiling(false);
    }
  };

  // ----- Deploy -----
  const handleDeploy = async () => {
    if (!deployForm.packageName.trim()) {
      log("error", "Package name is required for deployment");
      return;
    }
    if (!deployForm.classKey.trim()) {
      log("error", "Class key is required for deployment");
      return;
    }

    setDeploying(true);
    log("info", `Deploying to ${deployForm.packageName}/${deployForm.classKey}...`);
    try {
      const result = await deployScript({
        source,
        language: "typescript",
        package_name: deployForm.packageName,
        class_key: deployForm.classKey,
        function_bindings: deployForm.functionBindings.filter((b) => b.name.trim()),
        target_envs: deployForm.targetEnvs.filter((e) => e.trim()),
      });

      if (result.success) {
        log("success", `Deployment successful!`);
        if (result.deployment_key) {
          log("info", `  Deployment key: ${result.deployment_key}`);
        }
        if (result.artifact_url) {
          log("info", `  Artifact URL: ${result.artifact_url}`);
        }
        if (result.message) {
          log("info", `  ${result.message}`);
        }
        // Refresh script list
        await loadScripts();
      } else {
        log("error", "Deployment failed:");
        for (const err of result.errors) {
          log("error", `  ${err}`);
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      log("error", `Deploy request failed: ${msg}`);
    } finally {
      setDeploying(false);
    }
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
            onClick={handleCompile}
            disabled={compiling || deploying}
          >
            {compiling ? (
              <Loader2 className="h-4 w-4 mr-1 animate-spin" />
            ) : (
              <Play className="h-4 w-4 mr-1" />
            )}
            Compile
          </Button>
          <Button
            size="sm"
            onClick={handleDeploy}
            disabled={compiling || deploying}
          >
            {deploying ? (
              <Loader2 className="h-4 w-4 mr-1 animate-spin" />
            ) : (
              <Rocket className="h-4 w-4 mr-1" />
            )}
            Deploy
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
                No scripts deployed yet
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

        {/* Right panel: deploy configuration */}
        <div className="w-72 border-l shrink-0 overflow-y-auto">
          <Card className="border-0 rounded-none shadow-none">
            <CardHeader className="pb-3">
              <CardTitle className="text-sm">Deploy Configuration</CardTitle>
            </CardHeader>
            <CardContent>
              <DeployForm data={deployForm} onChange={setDeployForm} />
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
