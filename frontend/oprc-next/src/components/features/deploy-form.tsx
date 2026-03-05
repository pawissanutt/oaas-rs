"use client";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Plus, X } from "lucide-react";
import type { ScriptFunctionBinding } from "@/lib/scripts-api";

export interface DeployFormData {
  packageName: string;
  classKey: string;
  functionBindings: ScriptFunctionBinding[];
  targetEnvs: string[];
}

export interface DeployFormProps {
  data: DeployFormData;
  onChange: (data: DeployFormData) => void;
  className?: string;
}

/**
 * Configuration form for script deployment.
 * Allows setting package name, class key, function bindings, and target envs.
 */
export function DeployForm({ data, onChange, className }: DeployFormProps) {
  const update = (partial: Partial<DeployFormData>) =>
    onChange({ ...data, ...partial });

  const addBinding = () => {
    update({
      functionBindings: [
        ...data.functionBindings,
        { name: "", stateless: false },
      ],
    });
  };

  const removeBinding = (index: number) => {
    update({
      functionBindings: data.functionBindings.filter((_, i) => i !== index),
    });
  };

  const updateBinding = (
    index: number,
    partial: Partial<ScriptFunctionBinding>,
  ) => {
    const bindings = [...data.functionBindings];
    bindings[index] = { ...bindings[index], ...partial };
    update({ functionBindings: bindings });
  };

  const addEnv = () => {
    update({ targetEnvs: [...data.targetEnvs, ""] });
  };

  const removeEnv = (index: number) => {
    update({
      targetEnvs: data.targetEnvs.filter((_, i) => i !== index),
    });
  };

  const updateEnv = (index: number, value: string) => {
    const envs = [...data.targetEnvs];
    envs[index] = value;
    update({ targetEnvs: envs });
  };

  return (
    <div className={`space-y-4 ${className ?? ""}`} data-testid="deploy-form">
      {/* Package Name */}
      <div className="space-y-1.5">
        <Label htmlFor="package-name">Package Name</Label>
        <Input
          id="package-name"
          placeholder="e.g., my-package"
          value={data.packageName}
          onChange={(e) => update({ packageName: e.target.value })}
        />
      </div>

      {/* Class Key */}
      <div className="space-y-1.5">
        <Label htmlFor="class-key">Class Key</Label>
        <Input
          id="class-key"
          placeholder="e.g., MyService"
          value={data.classKey}
          onChange={(e) => update({ classKey: e.target.value })}
        />
      </div>

      {/* Function Bindings */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Function Bindings</Label>
          <Button variant="ghost" size="sm" onClick={addBinding} type="button">
            <Plus className="h-3 w-3 mr-1" /> Add
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Leave empty to auto-detect from @method decorators.
        </p>
        {data.functionBindings.map((binding, i) => (
          <div key={i} className="flex items-center gap-2">
            <Input
              placeholder="Method name"
              value={binding.name}
              onChange={(e) => updateBinding(i, { name: e.target.value })}
              className="flex-1"
            />
            <div className="flex items-center gap-1.5">
              <Switch
                checked={binding.stateless}
                onCheckedChange={(checked) =>
                  updateBinding(i, { stateless: checked })
                }
              />
              <span className="text-xs text-muted-foreground whitespace-nowrap">
                Stateless
              </span>
            </div>
            <Button
              variant="ghost"
              size="icon"
              onClick={() => removeBinding(i)}
              type="button"
              className="h-8 w-8"
            >
              <X className="h-3 w-3" />
            </Button>
          </div>
        ))}
      </div>

      {/* Target Environments */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Target Environments</Label>
          <Button variant="ghost" size="sm" onClick={addEnv} type="button">
            <Plus className="h-3 w-3 mr-1" /> Add
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Leave empty for automatic environment selection.
        </p>
        {data.targetEnvs.map((env, i) => (
          <div key={i} className="flex items-center gap-2">
            <Input
              placeholder="Environment name"
              value={env}
              onChange={(e) => updateEnv(i, e.target.value)}
              className="flex-1"
            />
            <Button
              variant="ghost"
              size="icon"
              onClick={() => removeEnv(i)}
              type="button"
              className="h-8 w-8"
            >
              <X className="h-3 w-3" />
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}
