"use client";

import { useState, useEffect } from "react";
import {
    Zap,
    Search,
    FileText,
    Loader2
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";
import { fetchPackages } from "@/lib/api";
import { getScriptSource } from "@/lib/scripts-api";
import { OFunction } from "@/lib/types";
import { toast } from "sonner";

interface FlatFunction {
    key: string;
    package: string;
    version: string;
    type: string;
    desc: string | null;
    boundTo: string | null;
    provisionConfig?: Record<string, unknown> | null;
}

export default function FunctionsPage() {
    const [search, setSearch] = useState("");
    const [typeFilter, setTypeFilter] = useState("All");

    const [functions, setFunctions] = useState<FlatFunction[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Source dialog state
    const [sourceOpen, setSourceOpen] = useState(false);
    const [sourceTitle, setSourceTitle] = useState("");
    const [sourceContent, setSourceContent] = useState("");
    const [sourceLanguage, setSourceLanguage] = useState("");
    const [loadingSource, setLoadingSource] = useState(false);

    useEffect(() => {
        async function loadData() {
            try {
                setLoading(true);
                const pkgs = await fetchPackages();
                const flatFns = pkgs.flatMap(pkg =>
                    pkg.functions.map((fn: OFunction) => {
                        const boundClasses = pkg.classes
                            .filter(c => c.function_bindings.some(fb => fb.function_key === fn.key))
                            .map(c => c.key)
                            .join(", ");

                        return {
                            key: fn.key,
                            package: pkg.name,
                            version: pkg.version || "latest",
                            type: fn.function_type,
                            desc: fn.description,
                            boundTo: boundClasses || null,
                            provisionConfig: fn.provision_config || null,
                        };
                    })
                );
                setFunctions(flatFns);
            } catch (e) {
                setError(e instanceof Error ? e.message : "Failed to load functions");
            } finally {
                setLoading(false);
            }
        }
        loadData();
    }, []);

    const handleViewSource = async (fn: FlatFunction) => {
        setLoadingSource(true);
        setSourceTitle(`${fn.package} / ${fn.key}`);

        try {
            // Try to fetch script source first
            const result = await getScriptSource(fn.package, fn.key);
            setSourceContent(result.source);
            setSourceLanguage(result.language || "typescript");
            setSourceOpen(true);
        } catch {
            // Fall back to showing provision config / function details
            if (fn.provisionConfig) {
                setSourceContent(JSON.stringify(fn.provisionConfig, null, 2));
                setSourceLanguage("json");
                setSourceOpen(true);
            } else {
                setSourceContent(JSON.stringify({
                    key: fn.key,
                    type: fn.type,
                    package: fn.package,
                    description: fn.desc,
                    bound_to: fn.boundTo,
                }, null, 2));
                setSourceLanguage("json");
                setSourceOpen(true);
            }
        } finally {
            setLoadingSource(false);
        }
    };

    const filtered = functions.filter((f) => {
        const matchesSearch = f.key.toLowerCase().includes(search.toLowerCase());
        const matchesType = typeFilter === "All" || f.type === typeFilter;
        return matchesSearch && matchesType;
    });

    return (
        <div className="space-y-6">
            <h1 className="text-3xl font-bold tracking-tight">Functions</h1>

            <div className="flex w-full items-center space-x-2">
                <div className="relative flex-1 max-w-sm">
                    <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input
                        type="search"
                        placeholder="Search functions..."
                        className="pl-8"
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                    />
                </div>
                <select
                    className="flex h-10 w-40 items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                    value={typeFilter}
                    onChange={(e) => setTypeFilter(e.target.value)}
                >
                    <option value="All">All types</option>
                    <option value="BUILTIN">Builtin</option>
                    <option value="CUSTOM">Custom</option>
                    <option value="MACRO">Macro</option>
                    <option value="LOGICAL">Logical</option>
                </select>
            </div>

            <div className="space-y-4">
                {loading ? (
                    <div className="flex flex-col items-center justify-center py-12 text-muted-foreground space-y-4">
                        <Loader2 className="h-8 w-8 animate-spin text-primary" />
                        <p>Loading functions...</p>
                    </div>
                ) : error ? (
                    <div className="text-center py-12 text-destructive">
                        Error: {error}
                    </div>
                ) : filtered.length === 0 ? (
                    <div className="text-center py-12 text-muted-foreground">
                        No functions found
                    </div>
                ) : (
                    filtered.map((fn) => (
                        <Card key={`${fn.package}-${fn.key}`} className="p-6">
                            <div className="flex items-start justify-between">
                                <div>
                                    <div className="flex items-center gap-2 mb-1">
                                        <Zap className="h-5 w-5 text-amber-500" />
                                        <h3 className="text-lg font-semibold">{fn.key}</h3>
                                        <Badge
                                            variant="outline"
                                            className={
                                                fn.type === "CUSTOM" ? "border-blue-200 text-blue-700 dark:text-blue-300" :
                                                    fn.type === "MACRO" ? "border-green-200 text-green-700 dark:text-green-300" :
                                                        fn.type === "BUILTIN" ? "border-gray-200 text-gray-700" : "border-purple-200 text-purple-700"
                                            }
                                        >
                                            {fn.type}
                                        </Badge>
                                    </div>
                                    <div className="text-sm text-muted-foreground mb-2">
                                        Package: <span className="text-foreground">{fn.package} {fn.version}</span>
                                    </div>
                                    {fn.desc && (
                                        <p className="text-sm italic text-muted-foreground mb-3">&quot;{fn.desc}&quot;</p>
                                    )}
                                    {fn.boundTo && (
                                        <div className="text-xs bg-muted inline-block px-2 py-1 rounded border">
                                            Bound to: <span className="font-mono">{fn.boundTo}</span>
                                        </div>
                                    )}
                                </div>
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    title="View Source"
                                    disabled={loadingSource}
                                    onClick={() => handleViewSource(fn)}
                                >
                                    {loadingSource ? (
                                        <Loader2 className="h-4 w-4 animate-spin" />
                                    ) : (
                                        <FileText className="h-4 w-4" />
                                    )}
                                </Button>
                            </div>
                        </Card>
                    ))
                )}
            </div>

            {/* Source / Detail Dialog */}
            <Dialog open={sourceOpen} onOpenChange={setSourceOpen}>
                <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
                    <DialogHeader>
                        <DialogTitle>{sourceTitle}</DialogTitle>
                    </DialogHeader>
                    <div className="flex-1 min-h-0 overflow-auto">
                        <div className="flex items-center gap-2 mb-2">
                            <Badge variant="outline" className="text-xs">{sourceLanguage}</Badge>
                        </div>
                        <pre className="text-sm font-mono bg-muted/50 p-4 rounded-md border whitespace-pre-wrap break-all">
                            {sourceContent}
                        </pre>
                    </div>
                </DialogContent>
            </Dialog>
        </div>
    );
}
