/**
 * Extract an OPackage template from TypeScript source code using @service/@method decorators.
 *
 * Parses decorator annotations via regex to produce a package JSON template
 * that can be used when creating a new package.
 */

export interface ExtractedMethod {
  name: string;
  stateless: boolean;
}

export interface ExtractedTemplate {
  packageName: string;
  className: string;
  methods: ExtractedMethod[];
  fields: string[];
}

/**
 * Parse @service and @method decorators from TypeScript source code.
 * Returns extracted metadata or null if no @service decorator found.
 */
export function extractTemplateFromSource(
  source: string,
): ExtractedTemplate | null {
  // Match @service("ClassName", { package: "pkg-name" })
  // or @service("ClassName") 
  const serviceMatch = source.match(
    /@service\(\s*["']([^"']+)["'](?:\s*,\s*\{[^}]*package\s*:\s*["']([^"']+)["'][^}]*\})?\s*\)/,
  );
  if (!serviceMatch) return null;

  const className = serviceMatch[1];
  const packageName = serviceMatch[2] || "default";

  // Match @method({ stateless: true/false }) or @method()
  const methods: ExtractedMethod[] = [];
  const methodRegex =
    /@method\(\s*(?:\{[^}]*stateless\s*:\s*(true|false)[^}]*\})?\s*\)\s*\n\s*(?:async\s+)?(\w+)\s*\(/g;
  let match;
  while ((match = methodRegex.exec(source)) !== null) {
    methods.push({
      name: match[2],
      stateless: match[1] === "true",
    });
  }

  // Extract class field declarations (lines like: fieldName: type = default;)
  // Match after "extends OaaSObject {" until first @method or method declaration
  const classBodyMatch = source.match(
    /extends\s+OaaSObject\s*\{([\s\S]*?)(?:@method|@getter|@setter|(?:async\s+)?\w+\s*\()/,
  );
  const fields: string[] = [];
  if (classBodyMatch) {
    const fieldRegex = /^\s+(\w+)\s*(?::\s*\w[^=]*?)?\s*=/gm;
    let fieldMatch;
    while ((fieldMatch = fieldRegex.exec(classBodyMatch[1])) !== null) {
      fields.push(fieldMatch[1]);
    }
  }

  return { packageName, className, methods, fields };
}

/**
 * Generate an OPackage JSON template from extracted decorator metadata.
 * The artifact_url is left as a placeholder to be filled in later.
 *
 * Follows the OaaS key naming convention:
 * - Function keys: `ClassName.methodName` (e.g., `Record.echo`)
 * - WASM function key: `ClassName.wasm`
 * - Class keys: plain class name (e.g., `Record`)
 */
export function generatePackageJson(
  template: ExtractedTemplate,
  artifactUrl?: string,
): object {
  const wasmFnKey = `${template.className}.wasm`;

  const functionBindings = template.methods.map((m) => ({
    name: m.name,
    function_key: wasmFnKey,
    access_modifier: "PUBLIC",
    stateless: m.stateless,
    parameters: [],
  }));

  return {
    name: template.packageName,
    version: "1.0.0",
    classes: [
      {
        key: template.className,
        description: `Script class: ${template.className}`,
        state_spec: {
          consistency_model: "NONE",
        },
        function_bindings: functionBindings,
      },
    ],
    functions: [
      {
        key: wasmFnKey,
        function_type: "WASM",
        description: `Auto-generated WASM function for ${template.className}`,
        provision_config: {
          wasm_module_url: artifactUrl || "<select-artifact>",
        },
        config: {},
      },
    ],
    dependencies: [],
    deployments: [],
    metadata: {
      tags: [],
    },
  };
}
