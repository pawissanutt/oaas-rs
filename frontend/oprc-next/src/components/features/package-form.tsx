"use client";

import { useState, useCallback, useEffect, useRef } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Plus, Trash, AlertCircle } from "lucide-react";
import type { OPackage } from "@/lib/bindings/OPackage";
import type { OFunction } from "@/lib/bindings/OFunction";
import type { OClass } from "@/lib/bindings/OClass";
import type { FunctionBinding } from "@/lib/bindings/FunctionBinding";
import type { StateSpecification } from "@/lib/bindings/StateSpecification";
import type { ProvisionConfig } from "@/lib/bindings/ProvisionConfig";
import type { KeySpecification } from "@/lib/bindings/KeySpecification";
import type { FunctionType } from "@/lib/bindings/FunctionType";
import type { FunctionAccessModifier } from "@/lib/bindings/FunctionAccessModifier";
import type { ConsistencyModel } from "@/lib/bindings/ConsistencyModel";
import type { ArtifactListEntry } from "@/lib/scripts-api";
import type { z } from "zod";
import {
  StringListField,
  KeyValueListField,
  CollapsibleSection,
} from "./package-form-fields";
import { formatZodErrors } from "@/lib/package-schema";
import YAML from "yaml";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface PackageFormProps {
  data: OPackage;
  onChange: (data: OPackage) => void;
  artifacts?: ArtifactListEntry[];
  errors?: z.ZodError;
  className?: string;
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const FUNCTION_TYPES: FunctionType[] = [
  "CUSTOM",
  "BUILTIN",
  "MACRO",
  "LOGICAL",
  "WASM",
];

const ACCESS_MODIFIERS: FunctionAccessModifier[] = [
  "PUBLIC",
  "INTERNAL",
  "PRIVATE",
];

const CONSISTENCY_MODELS: ConsistencyModel[] = [
  "NONE",
  "READ_YOUR_WRITE",
  "BOUNDED_STALENESS",
  "STRONG",
];

const DEFAULT_PROVISION_CONFIG: ProvisionConfig = {
  container_image: null,
  wasm_module_url: null,
  port: null,
  max_concurrency: 0,
  need_http2: false,
  cpu_request: null,
  memory_request: null,
  cpu_limit: null,
  memory_limit: null,
  min_scale: null,
  max_scale: null,
};

const DEFAULT_STATE_SPEC: StateSpecification = {
  key_specs: [],
  default_provider: "memory",
  consistency_model: "NONE",
  persistent: false,
  serialization_format: "json",
};

// ---------------------------------------------------------------------------
// Helper: get field errors by path prefix
// ---------------------------------------------------------------------------

function getFieldErrors(
  errors: z.ZodError | undefined,
  pathPrefix: string,
): string[] {
  if (!errors) return [];
  return errors.issues
    .filter((issue) => issue.path.join(".").startsWith(pathPrefix))
    .map((issue) => issue.message);
}

function FieldError({ errors, path }: { errors?: z.ZodError; path: string }) {
  const msgs = getFieldErrors(errors, path);
  if (msgs.length === 0) return null;
  return (
    <div className="flex items-center gap-1 text-xs text-destructive mt-1">
      <AlertCircle className="h-3 w-3" />
      {msgs[0]}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main PackageForm component
// ---------------------------------------------------------------------------

export function PackageForm({
  data,
  onChange,
  artifacts = [],
  errors,
  className,
}: PackageFormProps) {
  const update = useCallback(
    (partial: Partial<OPackage>) => onChange({ ...data, ...partial }),
    [data, onChange],
  );

  const updateMetadata = useCallback(
    (partial: Partial<OPackage["metadata"]>) =>
      update({ metadata: { ...data.metadata, ...partial } }),
    [data.metadata, update],
  );

  // ----- Functions helpers -----
  const addFunction = () => {
    const newFn: OFunction = {
      key: "",
      function_type: "CUSTOM",
      description: null,
      provision_config: { ...DEFAULT_PROVISION_CONFIG },
      config: {},
    };
    update({ functions: [...data.functions, newFn] });
  };

  const removeFunction = (i: number) =>
    update({ functions: data.functions.filter((_, idx) => idx !== i) });

  const updateFunction = (i: number, partial: Partial<OFunction>) => {
    const fns = [...data.functions];
    fns[i] = { ...fns[i], ...partial };
    update({ functions: fns });
  };

  // ----- Classes helpers -----
  const addClass = () => {
    const newCls: OClass = {
      key: "",
      description: null,
      state_spec: null,
      function_bindings: [],
    };
    update({ classes: [...data.classes, newCls] });
  };

  const removeClass = (i: number) =>
    update({ classes: data.classes.filter((_, idx) => idx !== i) });

  const updateClass = (i: number, partial: Partial<OClass>) => {
    const cls = [...data.classes];
    cls[i] = { ...cls[i], ...partial };
    update({ classes: cls });
  };

  return (
    <Tabs
      defaultValue="metadata"
      className={className}
      data-testid="package-form"
    >
      <TabsList className="grid w-full grid-cols-4">
        <TabsTrigger value="metadata">Metadata</TabsTrigger>
        <TabsTrigger value="functions">
          Functions
          {data.functions.length > 0 && (
            <Badge variant="secondary" className="ml-1.5 text-[10px] px-1.5">
              {data.functions.length}
            </Badge>
          )}
        </TabsTrigger>
        <TabsTrigger value="classes">
          Classes
          {data.classes.length > 0 && (
            <Badge variant="secondary" className="ml-1.5 text-[10px] px-1.5">
              {data.classes.length}
            </Badge>
          )}
        </TabsTrigger>
        <TabsTrigger value="code">JSON / YAML</TabsTrigger>
      </TabsList>

      {/* ================================================================ */}
      {/* Metadata Tab                                                     */}
      {/* ================================================================ */}
      <TabsContent value="metadata" className="space-y-4 mt-4">
        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <Label htmlFor="pkg-name">Package Name *</Label>
            <Input
              id="pkg-name"
              placeholder="e.g., my-package"
              value={data.name}
              onChange={(e) => update({ name: e.target.value })}
            />
            <FieldError errors={errors} path="name" />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pkg-version">Version</Label>
            <Input
              id="pkg-version"
              placeholder="e.g., 1.0.0"
              value={data.version ?? ""}
              onChange={(e) =>
                update({ version: e.target.value || null })
              }
            />
          </div>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <Label htmlFor="pkg-author">Author</Label>
            <Input
              id="pkg-author"
              placeholder="e.g., John Doe"
              value={data.metadata?.author ?? ""}
              onChange={(e) =>
                updateMetadata({ author: e.target.value || null })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="pkg-description">Description</Label>
            <Textarea
              id="pkg-description"
              placeholder="Brief package description..."
              value={data.metadata?.description ?? ""}
              onChange={(e) =>
                updateMetadata({ description: e.target.value || null })
              }
              className="min-h-[60px]"
            />
          </div>
        </div>

        <StringListField
          label="Tags"
          values={data.metadata?.tags ?? []}
          onChange={(tags) => updateMetadata({ tags })}
          placeholder="e.g., production"
        />

        <StringListField
          label="Dependencies"
          values={data.dependencies}
          onChange={(dependencies) => update({ dependencies })}
          placeholder="e.g., other-package"
          helperText="Other packages this package depends on."
        />
      </TabsContent>

      {/* ================================================================ */}
      {/* Functions Tab                                                    */}
      {/* ================================================================ */}
      <TabsContent value="functions" className="space-y-3 mt-4">
        <div className="flex items-center justify-between">
          <p className="text-sm text-muted-foreground">
            Define functions that classes can bind to.
          </p>
          <Button variant="outline" size="sm" onClick={addFunction} type="button">
            <Plus className="h-3 w-3 mr-1" /> Add Function
          </Button>
        </div>

        {data.functions.length === 0 && (
          <div className="text-center py-8 text-muted-foreground border border-dashed rounded-md">
            No functions defined. Click &quot;Add Function&quot; to get started.
          </div>
        )}

        {data.functions.map((fn, fi) => (
          <FunctionCard
            key={fi}
            fn={fn}
            index={fi}
            artifacts={artifacts}
            errors={errors}
            onUpdate={(partial) => updateFunction(fi, partial)}
            onRemove={() => removeFunction(fi)}
          />
        ))}
      </TabsContent>

      {/* ================================================================ */}
      {/* Classes Tab                                                      */}
      {/* ================================================================ */}
      <TabsContent value="classes" className="space-y-3 mt-4">
        <div className="flex items-center justify-between">
          <p className="text-sm text-muted-foreground">
            Define classes with state specifications and function bindings.
          </p>
          <Button variant="outline" size="sm" onClick={addClass} type="button">
            <Plus className="h-3 w-3 mr-1" /> Add Class
          </Button>
        </div>

        {data.classes.length === 0 && (
          <div className="text-center py-8 text-muted-foreground border border-dashed rounded-md">
            No classes defined. Click &quot;Add Class&quot; to get started.
          </div>
        )}

        {data.classes.map((cls, ci) => (
          <ClassCard
            key={ci}
            cls={cls}
            index={ci}
            functionKeys={data.functions.map((f) => f.key).filter(Boolean)}
            errors={errors}
            onUpdate={(partial) => updateClass(ci, partial)}
            onRemove={() => removeClass(ci)}
          />
        ))}
      </TabsContent>

      {/* ================================================================ */}
      {/* JSON / YAML Tab                                                  */}
      {/* ================================================================ */}
      <TabsContent value="code" className="mt-4">
        <CodeTab data={data} onChange={onChange} errors={errors} />
      </TabsContent>
    </Tabs>
  );
}

// ===========================================================================
// FunctionCard sub-component
// ===========================================================================

function FunctionCard({
  fn,
  index,
  artifacts,
  errors,
  onUpdate,
  onRemove,
}: {
  fn: OFunction;
  index: number;
  artifacts: ArtifactListEntry[];
  errors?: z.ZodError;
  onUpdate: (partial: Partial<OFunction>) => void;
  onRemove: () => void;
}) {
  const updateProvision = (partial: Partial<ProvisionConfig>) => {
    const current = fn.provision_config ?? { ...DEFAULT_PROVISION_CONFIG };
    onUpdate({ provision_config: { ...current, ...partial } });
  };

  const showProvision =
    fn.function_type === "CUSTOM" || fn.function_type === "WASM";

  return (
    <CollapsibleSection
      title={fn.key || `Function #${index + 1}`}
      defaultOpen={!fn.key}
      actions={
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-destructive hover:text-destructive"
          onClick={onRemove}
          type="button"
        >
          <Trash className="h-3.5 w-3.5" />
        </Button>
      }
    >
      <div className="space-y-3 mt-3">
        {/* Key + Type row */}
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-1.5">
            <Label>Function Key *</Label>
            <Input
              placeholder="e.g., Record.echo"
              value={fn.key}
              onChange={(e) => onUpdate({ key: e.target.value })}
            />
            <FieldError errors={errors} path={`functions.${index}.key`} />
          </div>
          <div className="space-y-1.5">
            <Label>Type</Label>
            <Select
              value={fn.function_type}
              onValueChange={(v) =>
                onUpdate({ function_type: v as FunctionType })
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {FUNCTION_TYPES.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>

        {/* Description */}
        <div className="space-y-1.5">
          <Label>Description</Label>
          <Input
            placeholder="Optional description"
            value={fn.description ?? ""}
            onChange={(e) =>
              onUpdate({ description: e.target.value || null })
            }
          />
        </div>

        {/* Provision Config (only for CUSTOM / WASM) */}
        {showProvision && (
          <CollapsibleSection title="Provision Config" defaultOpen>
            <div className="space-y-3 mt-3">
              {fn.function_type === "CUSTOM" && (
                <div className="space-y-1.5">
                  <Label>Container Image</Label>
                  <Input
                    placeholder="e.g., ghcr.io/org/image:tag"
                    value={fn.provision_config?.container_image ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        container_image: e.target.value || null,
                      })
                    }
                  />
                </div>
              )}

              {fn.function_type === "WASM" && (
                <div className="space-y-1.5">
                  <Label>WASM Module URL</Label>
                  <div className="flex gap-2">
                    <Input
                      placeholder="HTTP, OCI, or file:// URL"
                      value={fn.provision_config?.wasm_module_url ?? ""}
                      onChange={(e) =>
                        updateProvision({
                          wasm_module_url: e.target.value || null,
                        })
                      }
                      className="flex-1"
                    />
                    {artifacts.length > 0 && (
                      <select
                        className="rounded-md border border-input bg-background px-2 py-1 text-xs max-w-[200px]"
                        defaultValue=""
                        onChange={(e) => {
                          if (e.target.value) {
                            updateProvision({
                              wasm_module_url: e.target.value,
                            });
                          }
                          e.target.value = "";
                        }}
                      >
                        <option value="">Pick artifact...</option>
                        {artifacts.map((a) => (
                          <option key={a.id} value={a.url}>
                            {a.id.substring(0, 12)}
                            {a.source_package
                              ? ` (${a.source_package}/${a.source_function})`
                              : ""}
                          </option>
                        ))}
                      </select>
                    )}
                  </div>
                </div>
              )}

              <div className="grid grid-cols-3 gap-3">
                <div className="space-y-1.5">
                  <Label>Port</Label>
                  <Input
                    type="number"
                    placeholder="e.g., 80"
                    value={fn.provision_config?.port ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        port: e.target.value ? Number(e.target.value) : null,
                      })
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Max Concurrency</Label>
                  <Input
                    type="number"
                    placeholder="0 = unlimited"
                    value={fn.provision_config?.max_concurrency ?? 0}
                    onChange={(e) =>
                      updateProvision({
                        max_concurrency: Number(e.target.value) || 0,
                      })
                    }
                  />
                </div>
                <div className="flex items-end gap-2 pb-1">
                  <Switch
                    checked={fn.provision_config?.need_http2 ?? false}
                    onCheckedChange={(checked) =>
                      updateProvision({ need_http2: checked })
                    }
                  />
                  <span className="text-sm text-muted-foreground">HTTP/2</span>
                </div>
              </div>

              {/* Resource limits grid */}
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <Label>CPU Request</Label>
                  <Input
                    placeholder="e.g., 100m"
                    value={fn.provision_config?.cpu_request ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        cpu_request: e.target.value || null,
                      })
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>CPU Limit</Label>
                  <Input
                    placeholder="e.g., 500m"
                    value={fn.provision_config?.cpu_limit ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        cpu_limit: e.target.value || null,
                      })
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Memory Request</Label>
                  <Input
                    placeholder="e.g., 128Mi"
                    value={fn.provision_config?.memory_request ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        memory_request: e.target.value || null,
                      })
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Memory Limit</Label>
                  <Input
                    placeholder="e.g., 256Mi"
                    value={fn.provision_config?.memory_limit ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        memory_limit: e.target.value || null,
                      })
                    }
                  />
                </div>
              </div>

              {/* Scale */}
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <Label>Min Scale</Label>
                  <Input
                    type="number"
                    placeholder="null"
                    value={fn.provision_config?.min_scale ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        min_scale: e.target.value
                          ? Number(e.target.value)
                          : null,
                      })
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Max Scale</Label>
                  <Input
                    type="number"
                    placeholder="null"
                    value={fn.provision_config?.max_scale ?? ""}
                    onChange={(e) =>
                      updateProvision({
                        max_scale: e.target.value
                          ? Number(e.target.value)
                          : null,
                      })
                    }
                  />
                </div>
              </div>
            </div>
          </CollapsibleSection>
        )}

        {/* Config (env vars) */}
        <KeyValueListField
          label="Config (env vars)"
          entries={fn.config}
          onChange={(config) => onUpdate({ config })}
          keyPlaceholder="e.g., HTTP_PORT"
          valuePlaceholder="e.g., 80"
          helperText="Key-value environment variables for the function runtime."
        />
      </div>
    </CollapsibleSection>
  );
}

// ===========================================================================
// ClassCard sub-component
// ===========================================================================

function ClassCard({
  cls,
  index,
  functionKeys,
  errors,
  onUpdate,
  onRemove,
}: {
  cls: OClass;
  index: number;
  functionKeys: string[];
  errors?: z.ZodError;
  onUpdate: (partial: Partial<OClass>) => void;
  onRemove: () => void;
}) {
  // ----- State Spec helpers -----
  const stateSpec = cls.state_spec;

  const enableStateSpec = () =>
    onUpdate({ state_spec: { ...DEFAULT_STATE_SPEC } });

  const disableStateSpec = () => onUpdate({ state_spec: null });

  const updateStateSpec = (partial: Partial<StateSpecification>) =>
    onUpdate({ state_spec: { ...(stateSpec ?? DEFAULT_STATE_SPEC), ...partial } });

  // ----- Function Bindings helpers -----
  const addBinding = () => {
    const newBinding: FunctionBinding = {
      name: "",
      function_key: "",
      access_modifier: "PUBLIC",
      stateless: false,
      parameters: [],
    };
    onUpdate({
      function_bindings: [...cls.function_bindings, newBinding],
    });
  };

  const removeBinding = (i: number) =>
    onUpdate({
      function_bindings: cls.function_bindings.filter((_, idx) => idx !== i),
    });

  const updateBinding = (i: number, partial: Partial<FunctionBinding>) => {
    const bindings = [...cls.function_bindings];
    bindings[i] = { ...bindings[i], ...partial };
    onUpdate({ function_bindings: bindings });
  };

  // ----- Key Specs helpers -----
  const addKeySpec = () => {
    const specs = [...(stateSpec?.key_specs ?? []), { name: "", key_type: "" }];
    updateStateSpec({ key_specs: specs });
  };

  const removeKeySpec = (i: number) => {
    const specs = (stateSpec?.key_specs ?? []).filter((_, idx) => idx !== i);
    updateStateSpec({ key_specs: specs });
  };

  const updateKeySpec = (i: number, partial: Partial<KeySpecification>) => {
    const specs = [...(stateSpec?.key_specs ?? [])];
    specs[i] = { ...specs[i], ...partial };
    updateStateSpec({ key_specs: specs });
  };

  return (
    <CollapsibleSection
      title={cls.key || `Class #${index + 1}`}
      defaultOpen={!cls.key}
      actions={
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-destructive hover:text-destructive"
          onClick={onRemove}
          type="button"
        >
          <Trash className="h-3.5 w-3.5" />
        </Button>
      }
    >
      <div className="space-y-3 mt-3">
        {/* Key + Description */}
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-1.5">
            <Label>Class Key *</Label>
            <Input
              placeholder="e.g., Record"
              value={cls.key}
              onChange={(e) => onUpdate({ key: e.target.value })}
            />
            <FieldError errors={errors} path={`classes.${index}.key`} />
          </div>
          <div className="space-y-1.5">
            <Label>Description</Label>
            <Input
              placeholder="Optional description"
              value={cls.description ?? ""}
              onChange={(e) =>
                onUpdate({ description: e.target.value || null })
              }
            />
          </div>
        </div>

        {/* State Specification */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label>State Specification</Label>
            <div className="flex items-center gap-2">
              <Switch
                checked={stateSpec !== null}
                onCheckedChange={(checked) =>
                  checked ? enableStateSpec() : disableStateSpec()
                }
              />
              <span className="text-xs text-muted-foreground">
                {stateSpec ? "Enabled" : "Disabled"}
              </span>
            </div>
          </div>

          {stateSpec && (
            <div className="p-3 border border-border/50 rounded-md space-y-3 bg-muted/20">
              <div className="grid grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <Label>Default Provider</Label>
                  <Input
                    placeholder="memory"
                    value={stateSpec.default_provider ?? ""}
                    onChange={(e) =>
                      updateStateSpec({
                        default_provider: e.target.value || undefined,
                      })
                    }
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>Consistency Model</Label>
                  <Select
                    value={stateSpec.consistency_model}
                    onValueChange={(v) =>
                      updateStateSpec({
                        consistency_model: v as ConsistencyModel,
                      })
                    }
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {CONSISTENCY_MODELS.map((m) => (
                        <SelectItem key={m} value={m}>
                          {m}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div className="flex items-center gap-2">
                  <Switch
                    checked={stateSpec.persistent}
                    onCheckedChange={(checked) =>
                      updateStateSpec({ persistent: checked })
                    }
                  />
                  <span className="text-sm">Persistent</span>
                </div>
                <div className="space-y-1.5">
                  <Label>Serialization Format</Label>
                  <Input
                    placeholder="json"
                    value={stateSpec.serialization_format}
                    onChange={(e) =>
                      updateStateSpec({
                        serialization_format: e.target.value || "json",
                      })
                    }
                  />
                </div>
              </div>

              {/* Key Specs */}
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <Label>Key Specs</Label>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={addKeySpec}
                    type="button"
                  >
                    <Plus className="h-3 w-3 mr-1" /> Add
                  </Button>
                </div>
                {(stateSpec.key_specs ?? []).map((ks, ki) => (
                  <div key={ki} className="flex items-center gap-2">
                    <Input
                      placeholder="Name"
                      value={ks.name}
                      onChange={(e) =>
                        updateKeySpec(ki, { name: e.target.value })
                      }
                      className="flex-1"
                    />
                    <Input
                      placeholder="Type"
                      value={ks.key_type}
                      onChange={(e) =>
                        updateKeySpec(ki, { key_type: e.target.value })
                      }
                      className="flex-1"
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => removeKeySpec(ki)}
                      type="button"
                      className="h-8 w-8 shrink-0"
                    >
                      <Trash className="h-3 w-3" />
                    </Button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Function Bindings */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label>Function Bindings</Label>
            <Button
              variant="ghost"
              size="sm"
              onClick={addBinding}
              type="button"
            >
              <Plus className="h-3 w-3 mr-1" /> Add Binding
            </Button>
          </div>

          {cls.function_bindings.map((binding, bi) => (
            <div
              key={bi}
              className="p-3 border border-border/50 rounded-md space-y-2 bg-muted/20"
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium text-muted-foreground">
                  Binding #{bi + 1}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  onClick={() => removeBinding(bi)}
                  type="button"
                  className="h-6 w-6"
                >
                  <Trash className="h-3 w-3" />
                </Button>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1">
                  <Label className="text-xs">Name *</Label>
                  <Input
                    placeholder="e.g., echo"
                    value={binding.name}
                    onChange={(e) =>
                      updateBinding(bi, { name: e.target.value })
                    }
                    className="h-8 text-sm"
                  />
                  <FieldError
                    errors={errors}
                    path={`classes.${index}.function_bindings.${bi}.name`}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">Function Key *</Label>
                  {functionKeys.length > 0 ? (
                    <Select
                      value={binding.function_key}
                      onValueChange={(v) =>
                        updateBinding(bi, { function_key: v })
                      }
                    >
                      <SelectTrigger className="h-8 text-sm">
                        <SelectValue placeholder="Select function..." />
                      </SelectTrigger>
                      <SelectContent>
                        {functionKeys.map((fk) => (
                          <SelectItem key={fk} value={fk}>
                            {fk}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  ) : (
                    <Input
                      placeholder="e.g., Record.echo"
                      value={binding.function_key}
                      onChange={(e) =>
                        updateBinding(bi, { function_key: e.target.value })
                      }
                      className="h-8 text-sm"
                    />
                  )}
                  <FieldError
                    errors={errors}
                    path={`classes.${index}.function_bindings.${bi}.function_key`}
                  />
                </div>
              </div>

              <div className="flex items-center gap-4">
                <div className="space-y-1">
                  <Label className="text-xs">Access</Label>
                  <Select
                    value={binding.access_modifier}
                    onValueChange={(v) =>
                      updateBinding(bi, {
                        access_modifier: v as FunctionAccessModifier,
                      })
                    }
                  >
                    <SelectTrigger className="h-8 text-sm w-[130px]">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {ACCESS_MODIFIERS.map((m) => (
                        <SelectItem key={m} value={m}>
                          {m}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex items-center gap-1.5 pt-5">
                  <Switch
                    checked={binding.stateless}
                    onCheckedChange={(checked) =>
                      updateBinding(bi, { stateless: checked })
                    }
                  />
                  <span className="text-xs text-muted-foreground">
                    Stateless
                  </span>
                </div>
              </div>

              <StringListField
                label="Parameters"
                values={binding.parameters}
                onChange={(parameters) => updateBinding(bi, { parameters })}
                placeholder="Parameter name"
              />
            </div>
          ))}
        </div>
      </div>
    </CollapsibleSection>
  );
}

// ===========================================================================
// CodeTab — bidirectional JSON / YAML editor
// ===========================================================================

type CodeFormat = "json" | "yaml";

function CodeTab({
  data,
  onChange,
  errors,
}: {
  data: OPackage;
  onChange: (data: OPackage) => void;
  errors?: z.ZodError;
}) {
  const [format, setFormat] = useState<CodeFormat>("json");
  const [text, setText] = useState("");
  const [parseError, setParseError] = useState<string | null>(null);
  const suppressSync = useRef(false);

  // Serialize form data → text when format or data changes (unless suppressed)
  useEffect(() => {
    if (suppressSync.current) {
      suppressSync.current = false;
      return;
    }
    try {
      // Strip deployments from the output (they are managed separately)
      const clean = { ...data };
      if (format === "json") {
        setText(JSON.stringify(clean, null, 2));
      } else {
        setText(YAML.stringify(clean, { indent: 2, lineWidth: 120 }));
      }
      setParseError(null);
    } catch {
      // keep current text if serialization somehow fails
    }
  }, [data, format]);

  const handleTextChange = useCallback(
    (newText: string) => {
      setText(newText);
      try {
        let parsed: unknown;
        if (format === "json") {
          parsed = JSON.parse(newText);
        } else {
          parsed = YAML.parse(newText);
        }
        setParseError(null);
        // Suppress the next effect so we don't overwrite the user's cursor
        suppressSync.current = true;
        onChange(parsed as OPackage);
      } catch (e) {
        setParseError(
          `Invalid ${format.toUpperCase()}: ${e instanceof Error ? e.message : String(e)}`,
        );
      }
    },
    [format, onChange],
  );

  const handleFormatSwitch = (newFormat: CodeFormat) => {
    // Convert current text to new format
    try {
      let parsed: unknown;
      if (format === "json") {
        parsed = JSON.parse(text);
      } else {
        parsed = YAML.parse(text);
      }
      setFormat(newFormat);
      if (newFormat === "json") {
        setText(JSON.stringify(parsed, null, 2));
      } else {
        setText(YAML.stringify(parsed, { indent: 2, lineWidth: 120 }));
      }
      setParseError(null);
    } catch {
      // If current text is invalid, just switch the mode
      setFormat(newFormat);
    }
  };

  const formattedErrors = errors ? formatZodErrors(errors) : [];

  return (
    <div className="space-y-3">
      {/* Format switcher */}
      <div className="flex items-center gap-2">
        <Label className="text-xs text-muted-foreground">Format:</Label>
        <div className="inline-flex rounded-md border border-input">
          <button
            type="button"
            className={`px-3 py-1 text-xs font-medium rounded-l-md transition-colors ${
              format === "json"
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted"
            }`}
            onClick={() => handleFormatSwitch("json")}
          >
            JSON
          </button>
          <button
            type="button"
            className={`px-3 py-1 text-xs font-medium rounded-r-md transition-colors ${
              format === "yaml"
                ? "bg-primary text-primary-foreground"
                : "hover:bg-muted"
            }`}
            onClick={() => handleFormatSwitch("yaml")}
          >
            YAML
          </button>
        </div>
      </div>

      {/* Editor */}
      <div className="relative border rounded-md overflow-hidden">
        <textarea
          className="w-full h-[400px] p-4 font-mono text-sm resize-none bg-muted/30 focus:outline-none focus:ring-2 focus:ring-ring rounded-md"
          value={text}
          onChange={(e) => handleTextChange(e.target.value)}
          spellCheck={false}
        />
      </div>

      {/* Parse error */}
      {parseError && (
        <div className="flex items-center gap-2 p-2 text-xs text-destructive bg-destructive/10 rounded-md">
          <AlertCircle className="h-3.5 w-3.5 shrink-0" />
          {parseError}
        </div>
      )}

      {/* Validation errors */}
      {formattedErrors.length > 0 && (
        <div className="space-y-1">
          <Label className="text-xs text-destructive">Validation Errors:</Label>
          {formattedErrors.map((err, i) => (
            <div
              key={i}
              className="flex items-start gap-2 text-xs text-destructive"
            >
              <AlertCircle className="h-3 w-3 mt-0.5 shrink-0" />
              <span>
                <span className="font-mono">{err.path}</span>: {err.message}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
