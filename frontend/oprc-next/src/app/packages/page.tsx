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
    Rocket,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
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
import { listArtifacts, type ArtifactListEntry } from "@/lib/scripts-api";
import { OPackage } from "@/lib/bindings/OPackage";
import { toast } from "sonner";
import { PackageForm } from "@/components/features/package-form";
import { validatePackage, DEFAULT_PACKAGE } from "@/lib/package-schema";
import type { z } from "zod";
import { useRouter } from "next/navigation";

export default function PackagesPage() {
    const router = useRouter();
    const [search, setSearch] = useState("");
    const [packages, setPackages] = useState<OPackage[]>([]);
    const [loading, setLoading] = useState(true);

    // Create dialog state
    const [createOpen, setCreateOpen] = useState(false);
    const [packageData, setPackageData] = useState<OPackage>({ ...DEFAULT_PACKAGE });
    const [validationErrors, setValidationErrors] = useState<z.ZodError | undefined>();
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

    // Artifact picker state
    const [artifacts, setArtifacts] = useState<ArtifactListEntry[]>([]);
    const [loadingArtifacts, setLoadingArtifacts] = useState(false);

    const loadPackages = useCallback(async () => {
        setLoading(true);
        const data = await fetchPackages();
        setPackages(data);
        setLoading(false);
    }, []);

    useEffect(() => {
        loadPackages();
    }, [loadPackages]);

    // Load artifacts for the picker when create dialog opens
    useEffect(() => {
        if (createOpen) {
            setLoadingArtifacts(true);
            listArtifacts()
                .then(setArtifacts)
                .finally(() => setLoadingArtifacts(false));
        }
    }, [createOpen]);

    // Check for script template in sessionStorage on mount
    useEffect(() => {
        const stored = sessionStorage.getItem("oprc-package-template");
        if (stored) {
            try {
                const parsed = JSON.parse(stored);
                setPackageData({
                    ...DEFAULT_PACKAGE,
                    ...parsed,
                    metadata: { ...DEFAULT_PACKAGE.metadata, ...(parsed.metadata ?? {}) },
                });
            } catch {
                // fallback: ignore invalid JSON
            }
            setCreateOpen(true);
            sessionStorage.removeItem("oprc-package-template");
            toast.info("Package template loaded from Scripts page");
        }
    }, []);

    const filtered = packages.filter((p) =>
        p.name.toLowerCase().includes(search.toLowerCase())
    );

    const handleCreate = async () => {
        setCreating(true);
        setValidationErrors(undefined);
        try {
            const result = validatePackage(packageData);
            if (!result.success) {
                setValidationErrors(result.errors);
                const msgs = result.errors.issues.map(i => i.message).slice(0, 3);
                toast.error(`Validation failed: ${msgs.join("; ")}`);
                setCreating(false);
                return;
            }

            const pkg = result.data;

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

    const handleDeployClass = (packageName: string, classKey: string) => {
        sessionStorage.setItem(
            "oprc-deploy-prefill",
            JSON.stringify({ package_name: packageName, class_key: classKey })
        );
        router.push("/deployments");
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
                    <DialogContent className="max-w-4xl h-[85vh] flex flex-col">
                        <DialogHeader>
                            <DialogTitle>Create Package</DialogTitle>
                            <DialogDescription>
                                Define your package using the form tabs or edit raw JSON / YAML.
                            </DialogDescription>
                        </DialogHeader>
                        <div className="flex-1 min-h-0 overflow-y-auto py-4">
                            <PackageForm
                                data={packageData}
                                onChange={(d) => {
                                    setPackageData(d);
                                    setValidationErrors(undefined);
                                }}
                                artifacts={artifacts}
                                errors={validationErrors}
                            />
                        </div>
                        <div className="flex items-center justify-between pt-2 border-t">
                            <div className="flex items-center space-x-2">
                                <Switch
                                    id="deploy-all"
                                    checked={alsoDeployAll}
                                    onCheckedChange={setAlsoDeployAll}
                                />
                                <Label htmlFor="deploy-all" className="text-sm">
                                    Also deploy all classes
                                </Label>
                            </div>
                            <div className="flex items-center gap-2">
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
                            </div>
                        </div>
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
                                                    <div className="font-medium flex items-center justify-between">
                                                        <div className="flex items-center gap-2">
                                                            <div className="w-2 h-2 rounded-full bg-cyan-500" />
                                                            {cls.key}
                                                        </div>
                                                        <Button
                                                            variant="outline"
                                                            size="sm"
                                                            className="h-7 text-xs"
                                                            onClick={() => handleDeployClass(pkg.name, cls.key)}
                                                        >
                                                            <Rocket className="h-3 w-3 mr-1" />
                                                            Deploy
                                                        </Button>
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
