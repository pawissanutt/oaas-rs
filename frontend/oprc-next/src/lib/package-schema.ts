import { z } from "zod";
import type { OPackage } from "./bindings/OPackage";

// ---------------------------------------------------------------------------
// Enum schemas
// ---------------------------------------------------------------------------

export const functionTypeSchema = z.enum([
  "BUILTIN",
  "CUSTOM",
  "MACRO",
  "LOGICAL",
  "WASM",
]);

export const functionAccessModifierSchema = z.enum([
  "PUBLIC",
  "INTERNAL",
  "PRIVATE",
]);

export const consistencyModelSchema = z.enum([
  "NONE",
  "READ_YOUR_WRITE",
  "BOUNDED_STALENESS",
  "STRONG",
]);

// ---------------------------------------------------------------------------
// Sub-schemas
// ---------------------------------------------------------------------------

export const keySpecificationSchema = z.object({
  name: z.string().min(1, "Key spec name is required"),
  key_type: z.string().min(1, "Key spec type is required"),
});

export const provisionConfigSchema = z.object({
  container_image: z.string().nullish(),
  wasm_module_url: z.string().nullish(),
  port: z.number().int().nullable().default(null),
  max_concurrency: z.number().int().min(0).default(0),
  need_http2: z.boolean().default(false),
  cpu_request: z.string().nullable().default(null),
  memory_request: z.string().nullable().default(null),
  cpu_limit: z.string().nullable().default(null),
  memory_limit: z.string().nullable().default(null),
  min_scale: z.number().int().min(0).nullable().default(null),
  max_scale: z.number().int().min(0).nullable().default(null),
});

export const stateSpecificationSchema = z.object({
  key_specs: z.array(keySpecificationSchema).default([]),
  default_provider: z.string().default("memory"),
  consistency_model: consistencyModelSchema.default("NONE"),
  persistent: z.boolean().default(false),
  serialization_format: z.string().default("json"),
});

export const functionBindingSchema = z.object({
  name: z.string().min(1, "Binding name is required"),
  function_key: z.string().min(1, "Function key is required"),
  access_modifier: functionAccessModifierSchema.default("PUBLIC"),
  stateless: z.boolean().default(false),
  parameters: z.array(z.string()).default([]),
});

export const deploymentConditionSchema = z.enum([
  "PENDING",
  "DEPLOYING",
  "RUNNING",
  "DOWN",
  "DELETED",
]);

export const nfrRequirementsSchema = z.object({
  min_throughput_rps: z.number().int().min(1).nullable().default(null),
  availability: z.number().min(0).max(1).nullable().default(null),
  cpu_utilization_target: z.number().min(0).max(1).nullable().default(null),
});

export const odgmDataSpecSchema = z.object({
  collections: z.array(z.string()).default([]),
  partition_count: z.number().int().nullable().default(null),
  replica_count: z.number().int().nullable().default(null),
  shard_type: z.string().nullable().default(null),
  env_node_ids: z.record(z.string(), z.array(z.number())).default({}),
  log: z.string().nullable().default(null),
});

export const classDeploymentSchema = z.object({
  key: z.string().min(1),
  package_name: z.string().min(1),
  class_key: z.string().min(1),
  target_envs: z.array(z.string()).default([]),
  available_envs: z.array(z.string()).default([]),
  nfr_requirements: nfrRequirementsSchema.default({
    min_throughput_rps: null,
    availability: null,
    cpu_utilization_target: null,
  }),
  env_templates: z.record(z.string(), z.string()).default({}),
  functions: z.array(z.any()).default([]),
  condition: deploymentConditionSchema.default("PENDING"),
  odgm: odgmDataSpecSchema.nullable().default(null),
  telemetry: z.any().nullable().default(null),
  status: z.any().nullable().default(null),
  created_at: z.string().nullish(),
  updated_at: z.string().nullish(),
});

export const classSchema = z.object({
  key: z.string().min(1, "Class key is required"),
  description: z.string().nullable().default(null),
  state_spec: stateSpecificationSchema.nullable().default(null),
  function_bindings: z.array(functionBindingSchema).default([]),
});

export const functionSchema = z.object({
  key: z.string().min(1, "Function key is required"),
  function_type: functionTypeSchema.default("CUSTOM"),
  description: z.string().nullable().default(null),
  provision_config: provisionConfigSchema.nullable().default(null),
  config: z.record(z.string(), z.string()).default({}),
});

export const packageMetadataSchema = z.object({
  author: z.string().nullable().default(null),
  description: z.string().nullable().default(null),
  tags: z.array(z.string()).default([]),
  created_at: z.string().nullish(),
  updated_at: z.string().nullish(),
});

// ---------------------------------------------------------------------------
// Root package schema
// ---------------------------------------------------------------------------

export const packageSchema = z
  .object({
    name: z.string().min(1, "Package name is required"),
    version: z.string().nullable().default(null),
    metadata: packageMetadataSchema.default({
      author: null,
      description: null,
      tags: [],
    }),
    classes: z.array(classSchema).default([]),
    functions: z.array(functionSchema).default([]),
    dependencies: z
      .array(z.string().min(1, "Dependency name cannot be empty"))
      .default([]),
    deployments: z.array(classDeploymentSchema).default([]),
  })
  .superRefine((data, ctx) => {
    // Unique class keys
    const classKeys = data.classes.map((c) => c.key);
    const seenClasses = new Set<string>();
    for (let i = 0; i < classKeys.length; i++) {
      if (seenClasses.has(classKeys[i])) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Duplicate class key: "${classKeys[i]}"`,
          path: ["classes", i, "key"],
        });
      }
      seenClasses.add(classKeys[i]);
    }

    // Unique function keys
    const fnKeys = data.functions.map((f) => f.key);
    const seenFns = new Set<string>();
    for (let i = 0; i < fnKeys.length; i++) {
      if (seenFns.has(fnKeys[i])) {
        ctx.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Duplicate function key: "${fnKeys[i]}"`,
          path: ["functions", i, "key"],
        });
      }
      seenFns.add(fnKeys[i]);
    }
  });

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export type PackageFormData = z.infer<typeof packageSchema>;

export function validatePackage(data: unknown):
  | { success: true; data: OPackage }
  | { success: false; errors: z.ZodError } {
  const result = packageSchema.safeParse(data);
  if (result.success) {
    return { success: true, data: result.data as unknown as OPackage };
  }
  return { success: false, errors: result.error };
}

export const DEFAULT_PACKAGE: OPackage = {
  name: "my-new-package",
  version: "0.1.0",
  metadata: { author: null, description: null, tags: [] },
  classes: [],
  functions: [],
  dependencies: [],
  deployments: [],
};

/**
 * Build a human-readable error summary from ZodError.
 * Returns an array of { path: string, message: string }.
 */
export function formatZodErrors(
  error: z.ZodError,
): { path: string; message: string }[] {
  return error.issues.map((issue) => ({
    path: issue.path.join("."),
    message: issue.message,
  }));
}
