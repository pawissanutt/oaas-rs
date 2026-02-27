/**
 * Scripts API module — client functions for the PM script endpoints.
 *
 * Endpoints:
 * - POST /api/v1/scripts/compile  — compile only (validation)
 * - POST /api/v1/scripts/deploy   — compile → store → deploy
 * - GET  /api/v1/scripts/{package}/{function} — fetch stored source
 */

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";
const API_V1 = `${API_BASE}/api/v1`;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CompileResult {
  success: boolean;
  errors: string[];
  wasm_size?: number;
}

export interface ScriptFunctionBinding {
  name: string;
  stateless: boolean;
}

export interface DeployConfig {
  source: string;
  language: string;
  package_name: string;
  class_key: string;
  function_bindings: ScriptFunctionBinding[];
  target_envs: string[];
}

export interface DeployResult {
  success: boolean;
  deployment_key?: string;
  artifact_id?: string;
  artifact_url?: string;
  errors: string[];
  message?: string;
}

export interface ScriptInfo {
  key: string;
  package: string;
  hasSource: boolean;
}

export interface ScriptSourceResponse {
  package: string;
  function: string;
  source: string;
  language: string;
}

// ---------------------------------------------------------------------------
// API Functions
// ---------------------------------------------------------------------------

/**
 * Compile a script without storing — validation only.
 * Returns compilation result with errors (if any) and WASM size on success.
 */
export async function compileScript(
  source: string,
  language: string = "typescript",
): Promise<CompileResult> {
  const res = await fetch(`${API_V1}/scripts/compile`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ source, language }),
  });

  const data = await res.json();

  return {
    success: data.success ?? false,
    errors: data.errors ?? [],
    wasm_size: data.wasm_size,
  };
}

/**
 * Compile, store, and deploy a script in one step.
 */
export async function deployScript(config: DeployConfig): Promise<DeployResult> {
  const res = await fetch(`${API_V1}/scripts/deploy`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(config),
  });

  const data = await res.json();

  return {
    success: data.success ?? false,
    deployment_key: data.deployment_key,
    artifact_id: data.artifact_id,
    artifact_url: data.artifact_url,
    errors: data.errors ?? [],
    message: data.message,
  };
}

/**
 * Fetch the stored TypeScript source code for a previously deployed script.
 */
export async function getScriptSource(
  packageName: string,
  functionName: string,
): Promise<ScriptSourceResponse> {
  const res = await fetch(
    `${API_V1}/scripts/${encodeURIComponent(packageName)}/${encodeURIComponent(functionName)}`,
  );

  if (!res.ok) {
    throw new Error(
      `Failed to fetch script source: ${res.status} ${res.statusText}`,
    );
  }

  return await res.json();
}

/**
 * List scripts that have WASM functions (i.e., function_type includes "WASM").
 * Derived from existing package data.
 */
export async function listScripts(): Promise<ScriptInfo[]> {
  try {
    const res = await fetch(`${API_V1}/packages`);
    if (!res.ok) throw new Error(`Failed to fetch packages: ${res.status}`);

    const packages = await res.json();
    const scripts: ScriptInfo[] = [];

    for (const pkg of packages) {
      for (const fn of pkg.functions ?? []) {
        // Functions from the scripting pipeline have provision_config with wasm_module_url
        const hasWasm =
          fn.provision_config?.wasm_module_url != null ||
          fn.function_type === "WASM";
        if (hasWasm) {
          scripts.push({
            key: fn.key,
            package: pkg.name,
            hasSource: true, // We assume source is stored for WASM functions
          });
        }
      }
    }

    return scripts;
  } catch (e) {
    console.error("Failed to list scripts:", e);
    return [];
  }
}
