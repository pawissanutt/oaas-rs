"use client";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Plus, X, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";

// ---------------------------------------------------------------------------
// StringListField — Add/Remove string list (for tags, dependencies, etc.)
// ---------------------------------------------------------------------------

export interface StringListFieldProps {
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
  placeholder?: string;
  helperText?: string;
}

export function StringListField({
  label,
  values,
  onChange,
  placeholder,
  helperText,
}: StringListFieldProps) {
  const add = () => onChange([...values, ""]);
  const remove = (i: number) => onChange(values.filter((_, idx) => idx !== i));
  const update = (i: number, v: string) => {
    const next = [...values];
    next[i] = v;
    onChange(next);
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label>{label}</Label>
        <Button variant="ghost" size="sm" onClick={add} type="button">
          <Plus className="h-3 w-3 mr-1" /> Add
        </Button>
      </div>
      {helperText && (
        <p className="text-xs text-muted-foreground">{helperText}</p>
      )}
      {values.map((v, i) => (
        <div key={i} className="flex items-center gap-2">
          <Input
            placeholder={placeholder}
            value={v}
            onChange={(e) => update(i, e.target.value)}
            className="flex-1"
          />
          <Button
            variant="ghost"
            size="icon"
            onClick={() => remove(i)}
            type="button"
            className="h-8 w-8 shrink-0"
          >
            <X className="h-3 w-3" />
          </Button>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// KeyValueListField — Add/Remove key-value pairs (for config / env vars)
// ---------------------------------------------------------------------------

export interface KeyValueListFieldProps {
  label: string;
  entries: Record<string, string>;
  onChange: (entries: Record<string, string>) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  helperText?: string;
}

export function KeyValueListField({
  label,
  entries,
  onChange,
  keyPlaceholder = "Key",
  valuePlaceholder = "Value",
  helperText,
}: KeyValueListFieldProps) {
  // Keep as ordered array for editing, convert back to Record on change
  const pairs = Object.entries(entries);

  const add = () => {
    onChange({ ...entries, "": "" });
  };

  const remove = (key: string) => {
    const next = { ...entries };
    delete next[key];
    onChange(next);
  };

  const updatePair = (oldKey: string, newKey: string, newValue: string) => {
    // Rebuild the record preserving order, replacing the old key
    const next: Record<string, string> = {};
    for (const [k, v] of pairs) {
      if (k === oldKey) {
        next[newKey] = newValue;
      } else {
        next[k] = v;
      }
    }
    onChange(next);
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <Label>{label}</Label>
        <Button variant="ghost" size="sm" onClick={add} type="button">
          <Plus className="h-3 w-3 mr-1" /> Add
        </Button>
      </div>
      {helperText && (
        <p className="text-xs text-muted-foreground">{helperText}</p>
      )}
      {pairs.map(([k, v], i) => (
        <div key={i} className="flex items-center gap-2">
          <Input
            placeholder={keyPlaceholder}
            value={k}
            onChange={(e) => updatePair(k, e.target.value, v)}
            className="flex-1"
          />
          <Input
            placeholder={valuePlaceholder}
            value={v}
            onChange={(e) => updatePair(k, k, e.target.value)}
            className="flex-1"
          />
          <Button
            variant="ghost"
            size="icon"
            onClick={() => remove(k)}
            type="button"
            className="h-8 w-8 shrink-0"
          >
            <X className="h-3 w-3" />
          </Button>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// CollapsibleSection — Styled <details>/<summary> wrapper
// ---------------------------------------------------------------------------

export interface CollapsibleSectionProps {
  title: string;
  defaultOpen?: boolean;
  children: React.ReactNode;
  actions?: React.ReactNode;
}

export function CollapsibleSection({
  title,
  defaultOpen = false,
  children,
  actions,
}: CollapsibleSectionProps) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div className="border border-border rounded-md">
      <button
        type="button"
        className="flex items-center justify-between w-full p-3 text-sm font-medium hover:bg-muted/50 transition-colors text-left"
        onClick={() => setOpen(!open)}
      >
        <div className="flex items-center gap-2">
          {open ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
          {title}
        </div>
        {actions && (
          <div onClick={(e) => e.stopPropagation()}>{actions}</div>
        )}
      </button>
      {open && <div className="p-3 pt-0 border-t border-border/50">{children}</div>}
    </div>
  );
}
