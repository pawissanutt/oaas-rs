"use client";

import { useState, useEffect, useCallback } from "react";
import {
    Package,
    Search,
    Trash,
    FileText,
    Tags,
    Zap,
    Plus,
    Loader2,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
    DialogFooter,
    DialogDescription,
} from "@/components/ui/dialog";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { RawJsonDialog } from "@/components/ui/raw-json-dialog";
import { fetchPackages, fetchPackage, createPackage, deletePackage, createDeployment } from "@/lib/api";
import { OPackage } from "@/lib/bindings/OPackage";
import { toast } from "sonner";

export default function PackagesPage() {
    const [search, setSearch] = useState("");
    const [packages, setPackages] = useState<OPackage[]>([]);
    const [loading, setLoading] = useState(true);

    // Create dialog state
    const [createOpen, setCreateOpen] = useState(false);
    const [createContent, setCreateContent] = useState(`{
  "name": "my-new-package",
  "version": "0.1.0",
  "classes": [],
  "functions": [],
  "dependencies": [],
  "deployments": [],
  "metadata": {}
}`);
    const [alsoDeployAll, setAlsoDeployAll] = useState(false);
    const [creating, setCreating] = useState(false);

    // View raw dialog state
    const [rawDialogOpen, setRawDialogOpen] = useState(false);
    const [rawData, setRawData] = useState<unknown>(null);
    const [rawTitle, setRawTitle] = useState("");
    const [loadingRaw, setLoadingRaw] = useState(false);

    // Delete confirm dialog state
    const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
    const [deleting, setDeleting] = useState(false);

    const loadPackages = useCallback(async () => {
        setLoading(true);
        const data = await fetchPackages();
        setPackages(data);
        setLoading(false);
    }, []);

    useEffect(() => {
        loadPackages();
    }, [loadPackages]);

    const filtered = packages.filter((p) =>
        p.name.toLowerCase().includes(search.toLowerCase())
    );

    const handleCreate = async () => {
        setCreating(true);
        try {
            const parsed = JSON.parse(createContent);

            const pkg: OPackage = {
                name: parsed.name || "",
                version: parsed.version || null,
                metadata: parsed.metadata || {},
                classes: parsed.classes || [],
                functions: parsed.functions || [],
                dependencies: parsed.dependencies || [],
                deployments: parsed.deployments || [],
            };

            if (!pkg.name) {
                toast.error("Package name is required");
                setCreating(false);
                return;
            }

            await createPackage(pkg);
            toast.success(`Package "${pkg.name}" created successfully`);

            // Optionally deploy all classes
            if (alsoDeployAll && pkg.classes.length > 0) {
                let deployedCount = 0;
                for (const cls of pkg.classes) {
                    try {
                        await createDeployment({
                            key: `${pkg.name}.${cls.key}`,
                            package_name: pkg.name,
                            class_key: cls.key,
                        });
                        deployedCount++;
                    } catch (e) {
                        toast.error(`Failed to deploy class "${cls.key}": ${e instanceof Error ? e.message : String(e)}`);
                    }
                }
                if (deployedCount > 0) {
                    toast.success(`Deployed ${deployedCount} class(es)`);
                }
            }

            setCreateOpen(false);
            await loadPackages();
        } catch (e) {
            toast.error(`Failed to create package: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setCreating(false);
        }
    };

    const handleViewRaw = async (name: string) => {
        setLoadingRaw(true);
        setRawTitle(`Package: ${name}`);
        try {
            const data = await fetchPackage(name);
            setRawData(data);
            setRawDialogOpen(true);
        } catch (e) {
            toast.error(`Failed to fetch package: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setLoadingRaw(false);
        }
    };

    const handleDelete = async () => {
        if (!deleteTarget) return;
        setDeleting(true);
        try {
            await deletePackage(deleteTarget);
            toast.success(`Package "${deleteTarget}" deleted`);
            setDeleteTarget(null);
            await loadPackages();
        } catch (e) {
            toast.error(`Failed to delete: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setDeleting(false);
        }
    };

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Packages</h1>
                <Dialog open={createOpen} onOpenChange={setCreateOpen}>
                    <DialogTrigger asChild>
                        <Button>
                            <Plus className="mr-2 h-4 w-4" /> New Package
                        </Button>
                    </DialogTrigger>
                    <DialogContent className="max-w-2xl h-[80vh] flex flex-col">
                        <DialogHeader>
                            <DialogTitle>Create Package</DialogTitle>
                            <DialogDescription>
                                Define your package structure in JSON format.
                            </DialogDescription>
                        </DialogHeader>
                        <div className="flex-1 min-h-0 flex flex-col gap-4 py-4">
                            <div className="flex-1 relative border rounded-md overflow-hidden">
                                <textarea
                                    className="w-full h-full p-4 font-mono text-sm resize-none bg-muted/30 focus:outline-none focus:ring-2 focus:ring-ring rounded-md"
                                    value={createContent}
                                    onChange={(e) => setCreateContent(e.target.value)}
                                    spellCheck={false}
                                />
                            </div>
                            <div className="flex items-center space-x-2">
                                <input
                                    type="checkbox"
                                    id="deploy"
                                    className="rounded border-gray-300"
                                    checked={alsoDeployAll}
                                    onChange={(e) => setAlsoDeployAll(e.target.checked)}
                                />
                                <label htmlFor="deploy" className="text-sm font-medium">
                                    Also deploy all classes
                                </label>
                            </div>
                        </div>
                        <DialogFooter>
                            <Button
                                variant="outline"
                                onClick={() => setCreateOpen(false)}
                                disabled={creating}
                            >
                                Cancel
                            </Button>
                            <Button onClick={handleCreate} disabled={creating}>
                                {creating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                                Apply
                            </Button>
                        </DialogFooter>
                    </DialogContent>
                </Dialog>
            </div>

            <div className="flex w-full items-center space-x-2">
                <div className="relative flex-1 max-w-sm">
                    <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input
                        type="search"
                        placeholder="Search packages..."
                        className="pl-8"
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                    />
                </div>
            </div>

            <div className="space-y-4">
                {loading ? (
                    <div className="text-center py-12 text-muted-foreground">Loading packages...</div>
                ) : filtered.length === 0 ? (
                    <div className="text-center py-12 text-muted-foreground">
                        No packages found
                    </div>
                ) : (
                    filtered.map((pkg) => (
                        <details
                            key={pkg.name}
                            className="group border border-border rounded-lg bg-card open:ring-1 open:ring-ring/20 transition-all"
                        >
                            <summary className="flex items-center justify-between p-4 cursor-pointer hover:bg-muted/50 transition-colors list-none">
                                <div className="flex items-center gap-4">
                                    <div className="p-2 bg-indigo-100 dark:bg-indigo-900/30 rounded-full">
                                        <Package className="h-5 w-5 text-indigo-600 dark:text-indigo-400" />
                                    </div>
                                    <div>
                                        <div className="flex items-center gap-2">
                                            <span className="font-semibold text-lg">{pkg.name}</span>
                                            {pkg.version && (
                                                <Badge variant="secondary" className="font-mono">{pkg.version}</Badge>
                                            )}
                                        </div>
                                        <div className="text-sm text-muted-foreground flex items-center gap-2 mt-1">
                                            <Tags className="h-3 w-3" /> {pkg.classes.length} classes
                                            <span className="text-border">|</span>
                                            <Zap className="h-3 w-3" /> {pkg.functions.length} functions
                                        </div>
                                    </div>
                                </div>
                                <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        title="View Raw"
                                        disabled={loadingRaw}
                                        onClick={(e) => {
                                            e.preventDefault();
                                            handleViewRaw(pkg.name);
                                        }}
                                    >
                                        <FileText className="h-4 w-4" />
                                    </Button>
                                    <Button
                                        variant="ghost"
                                        size="icon"
                                        className="text-destructive hover:text-destructive"
                                        title="Delete"
                                        onClick={(e) => {
                                            e.preventDefault();
                                            setDeleteTarget(pkg.name);
                                        }}
                                    >
                                        <Trash className="h-4 w-4" />
                                    </Button>
                                </div>
                            </summary>

                            <div className="px-4 pb-4 pt-0 border-t border-border/50">
                                <div className="mt-4 grid md:grid-cols-2 gap-6">
                                    {/* Classes Column */}
                                    <div>
                                        <h4 className="text-sm font-semibold mb-3 flex items-center gap-2">
                                            <Tags className="h-4 w-4" /> Classes
                                        </h4>
                                        <div className="space-y-2">
                                            {pkg.classes.map((cls) => (
                                                <div key={cls.key} className="p-3 bg-muted/50 rounded-md border border-border">
                                                    <div className="font-medium flex items-center gap-2">
                                                        <div className="w-2 h-2 rounded-full bg-cyan-500" />
                                                        {cls.key}
                                                    </div>
                                                    <div className="text-xs text-muted-foreground mt-1 ml-4">
                                                        Functions: {cls.function_bindings.map(fb => fb.name).join(", ")}
                                                        {cls.description && <div className="mt-1">{cls.description}</div>}
                                                    </div>
                                                </div>
                                            ))}
                                        </div>
                                    </div>

                                    {/* Functions Column */}
                                    <div>
                                        <h4 className="text-sm font-semibold mb-3 flex items-center gap-2">
                                            <Zap className="h-4 w-4" /> Functions
                                        </h4>
                                        <div className="space-y-2">
                                            {pkg.functions.map((fn) => (
                                                <div key={fn.key} className="p-3 bg-muted/50 rounded-md border border-border flex items-center justify-between">
                                                    <div className="font-medium flex items-center gap-2 font-mono text-sm">
                                                        <Zap className="h-3 w-3 text-amber-500" />
                                                        {fn.key}
                                                    </div>
                                                    <Badge variant="outline" className={
                                                        fn.function_type === "CUSTOM" ? "border-blue-200 text-blue-700 dark:text-blue-300" :
                                                            fn.function_type === "MACRO" ? "border-green-200 text-green-700 dark:text-green-300" :
                                                                fn.function_type === "BUILTIN" ? "border-gray-200 text-gray-700" : "border-purple-200 text-purple-700"
                                                    }>
                                                        {fn.function_type}
                                                    </Badge>
                                                </div>
                                            ))}
                                        </div>
                                    </div>
                                </div>
                                {pkg.metadata?.description && (
                                    <div className="mt-4 pt-4 border-t border-border/50 text-sm text-muted-foreground">
                                        {pkg.metadata.description}
                                    </div>
                                )}
                            </div>
                        </details>
                    ))
                )}
            </div>

            {/* View Raw Dialog */}
            <RawJsonDialog
                open={rawDialogOpen}
                onOpenChange={setRawDialogOpen}
                title={rawTitle}
                data={rawData}
            />

            {/* Delete Confirm Dialog */}
            <ConfirmDialog
                open={deleteTarget !== null}
                onOpenChange={(open) => { if (!open) setDeleteTarget(null); }}
                title="Delete Package"
                description={`Are you sure you want to delete package "${deleteTarget}"? This will also delete all associated deployments and artifacts. This action cannot be undone.`}
                confirmLabel="Delete"
                variant="destructive"
                loading={deleting}
                onConfirm={handleDelete}
            />
        </div>
    );
}
