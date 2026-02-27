"use client";

import { useState, useEffect, useCallback } from "react";
import {
    Rocket,
    Search,
    Plus,
    Trash,
    FileText,
    Activity,
    Server,
    Cpu,
    Loader2,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
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
import { fetchDeployments, fetchPackages, fetchEnvironments, createDeployment, deleteDeployment } from "@/lib/api";
import { OClassDeployment } from "@/lib/bindings/OClassDeployment";
import { OPackage } from "@/lib/bindings/OPackage";
import { ClusterInfo } from "@/lib/types";
import { toast } from "sonner";

export default function DeploymentsPage() {
    const [search, setSearch] = useState("");
    const [deployments, setDeployments] = useState<OClassDeployment[]>([]);
    const [loading, setLoading] = useState(true);

    // Create dialog state
    const [createOpen, setCreateOpen] = useState(false);
    const [packages, setPackagesState] = useState<OPackage[]>([]);
    const [environments, setEnvironments] = useState<ClusterInfo[]>([]);
    const [formPkg, setFormPkg] = useState("");
    const [formClass, setFormClass] = useState("");
    const [formKey, setFormKey] = useState("");
    const [formEnvs, setFormEnvs] = useState<string[]>([]);
    const [formThroughput, setFormThroughput] = useState("");
    const [formAvailability, setFormAvailability] = useState("");
    const [formCpu, setFormCpu] = useState("");
    const [creating, setCreating] = useState(false);

    // View raw dialog state
    const [rawDialogOpen, setRawDialogOpen] = useState(false);
    const [rawData, setRawData] = useState<unknown>(null);
    const [rawTitle, setRawTitle] = useState("");

    // Delete confirm dialog state
    const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
    const [deleting, setDeleting] = useState(false);

    const loadDeployments = useCallback(async () => {
        setLoading(true);
        const data = await fetchDeployments();
        setDeployments(data);
        setLoading(false);
    }, []);

    useEffect(() => {
        loadDeployments();
    }, [loadDeployments]);

    // Load packages and environments when create dialog opens
    useEffect(() => {
        if (createOpen) {
            Promise.all([fetchPackages(), fetchEnvironments()]).then(([pkgs, envs]) => {
                setPackagesState(pkgs);
                setEnvironments(envs);
                if (pkgs.length > 0) {
                    setFormPkg(pkgs[0].name);
                    if (pkgs[0].classes.length > 0) {
                        const cls = pkgs[0].classes[0].key;
                        setFormClass(cls);
                        setFormKey(`${pkgs[0].name}.${cls}`);
                    }
                }
            });
        }
    }, [createOpen]);

    // Auto-update key when package/class changes
    useEffect(() => {
        if (formPkg && formClass) {
            setFormKey(`${formPkg}.${formClass}`);
        }
    }, [formPkg, formClass]);

    const selectedPkg = packages.find(p => p.name === formPkg);
    const availableClasses = selectedPkg?.classes || [];

    const filtered = deployments.filter((d) =>
        d.key.toLowerCase().includes(search.toLowerCase())
    );

    const handleCreate = async () => {
        if (!formKey || !formPkg || !formClass) {
            toast.error("Key, package, and class are required");
            return;
        }
        setCreating(true);
        try {
            const dep: Partial<OClassDeployment> = {
                key: formKey,
                package_name: formPkg,
                class_key: formClass,
                target_envs: formEnvs.length > 0 ? formEnvs : undefined,
                nfr_requirements: {
                    min_throughput_rps: formThroughput ? Number(formThroughput) : null,
                    availability: formAvailability ? Number(formAvailability) : null,
                    cpu_utilization_target: formCpu ? Number(formCpu) : null,
                },
            };
            await createDeployment(dep);
            toast.success(`Deployment "${formKey}" created`);
            setCreateOpen(false);
            // Reset form
            setFormPkg("");
            setFormClass("");
            setFormKey("");
            setFormEnvs([]);
            setFormThroughput("");
            setFormAvailability("");
            setFormCpu("");
            await loadDeployments();
        } catch (e) {
            toast.error(`Failed to create deployment: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setCreating(false);
        }
    };

    const handleViewRaw = (deploy: OClassDeployment) => {
        setRawTitle(`Deployment: ${deploy.key}`);
        setRawData(deploy);
        setRawDialogOpen(true);
    };

    const handleDelete = async () => {
        if (!deleteTarget) return;
        setDeleting(true);
        try {
            const result = await deleteDeployment(deleteTarget);
            toast.success(`Deployment "${deleteTarget}" deleted from ${result.deleted_envs?.length || 0} environment(s)`);
            setDeleteTarget(null);
            await loadDeployments();
        } catch (e) {
            toast.error(`Failed to delete: ${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setDeleting(false);
        }
    };

    const toggleEnv = (envName: string) => {
        setFormEnvs(prev =>
            prev.includes(envName) ? prev.filter(e => e !== envName) : [...prev, envName]
        );
    };

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Deployments</h1>
                <Dialog open={createOpen} onOpenChange={setCreateOpen}>
                    <DialogTrigger asChild>
                        <Button>
                            <Plus className="mr-2 h-4 w-4" /> New Deployment
                        </Button>
                    </DialogTrigger>
                    <DialogContent className="max-w-lg">
                        <DialogHeader>
                            <DialogTitle>Create Deployment</DialogTitle>
                            <DialogDescription>
                                Deploy a class from a package to one or more environments.
                            </DialogDescription>
                        </DialogHeader>
                        <div className="grid gap-4 py-4">
                            <div className="grid gap-2">
                                <Label>Package</Label>
                                <select
                                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                                    value={formPkg}
                                    onChange={(e) => {
                                        setFormPkg(e.target.value);
                                        const pkg = packages.find(p => p.name === e.target.value);
                                        if (pkg && pkg.classes.length > 0) {
                                            setFormClass(pkg.classes[0].key);
                                        } else {
                                            setFormClass("");
                                        }
                                    }}
                                >
                                    <option value="" disabled>Select package</option>
                                    {packages.map(p => (
                                        <option key={p.name} value={p.name}>{p.name}</option>
                                    ))}
                                </select>
                            </div>
                            <div className="grid gap-2">
                                <Label>Class</Label>
                                <select
                                    className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-ring"
                                    value={formClass}
                                    onChange={(e) => setFormClass(e.target.value)}
                                >
                                    <option value="" disabled>Select class</option>
                                    {availableClasses.map(c => (
                                        <option key={c.key} value={c.key}>{c.key}</option>
                                    ))}
                                </select>
                            </div>
                            <div className="grid gap-2">
                                <Label>Deployment Key</Label>
                                <Input
                                    value={formKey}
                                    onChange={(e) => setFormKey(e.target.value)}
                                    placeholder="package-name.class-key"
                                />
                            </div>
                            <div className="grid gap-2">
                                <Label>Target Environments</Label>
                                <div className="flex flex-wrap gap-2">
                                    {environments.length === 0 ? (
                                        <span className="text-sm text-muted-foreground">No environments available (auto-select)</span>
                                    ) : (
                                        environments.map(env => (
                                            <Badge
                                                key={env.name}
                                                variant={formEnvs.includes(env.name) ? "default" : "outline"}
                                                className="cursor-pointer"
                                                onClick={() => toggleEnv(env.name)}
                                            >
                                                {env.name}
                                            </Badge>
                                        ))
                                    )}
                                </div>
                                <p className="text-xs text-muted-foreground">Leave empty for automatic environment selection</p>
                            </div>
                            <div className="grid gap-2">
                                <Label>NFR Requirements (optional)</Label>
                                <div className="grid grid-cols-3 gap-2">
                                    <div>
                                        <Label className="text-xs">Min TPS</Label>
                                        <Input
                                            type="number"
                                            placeholder="e.g. 100"
                                            value={formThroughput}
                                            onChange={(e) => setFormThroughput(e.target.value)}
                                        />
                                    </div>
                                    <div>
                                        <Label className="text-xs">Availability %</Label>
                                        <Input
                                            type="number"
                                            placeholder="e.g. 99.9"
                                            value={formAvailability}
                                            onChange={(e) => setFormAvailability(e.target.value)}
                                        />
                                    </div>
                                    <div>
                                        <Label className="text-xs">CPU Target %</Label>
                                        <Input
                                            type="number"
                                            placeholder="e.g. 70"
                                            value={formCpu}
                                            onChange={(e) => setFormCpu(e.target.value)}
                                        />
                                    </div>
                                </div>
                            </div>
                        </div>
                        <DialogFooter>
                            <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creating}>
                                Cancel
                            </Button>
                            <Button onClick={handleCreate} disabled={creating}>
                                {creating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                                Create
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
                        placeholder="Search deployments..."
                        className="pl-8"
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                    />
                </div>
            </div>

            <div className="space-y-4">
                {loading ? (
                    <div className="text-center py-12 text-muted-foreground">Loading deployments...</div>
                ) : filtered.length === 0 ? (
                    <div className="text-center py-12 text-muted-foreground">
                        No deployments found
                    </div>
                ) : (
                    filtered.map((deploy) => (
                        <Card key={deploy.key} className="overflow-hidden">
                            <div className="flex flex-col sm:flex-row border-l-4 border-l-transparent data-[status=Running]:border-l-green-500 data-[status=Deploying]:border-l-yellow-500 data-[status=Error]:border-l-red-500" data-status={deploy.condition}>
                                <div className="flex-1 p-6">
                                    <div className="flex items-start justify-between">
                                        <div>
                                            <div className="flex items-center gap-2 mb-1">
                                                <h3 className="text-lg font-semibold flex items-center gap-2">
                                                    <Rocket className="h-5 w-5 text-muted-foreground" />
                                                    {deploy.key}
                                                </h3>
                                                <Badge
                                                    variant={
                                                        deploy.condition === "RUNNING" ? "success" :
                                                            deploy.condition === "DEPLOYING" ? "warning" : "destructive"
                                                    }
                                                >
                                                    {deploy.condition}
                                                </Badge>
                                            </div>
                                            <div className="text-sm text-muted-foreground mb-4">
                                                Package: <span className="text-foreground">{deploy.package_name}</span> • Class: <span className="text-foreground">{deploy.class_key}</span>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                title="View Raw"
                                                onClick={() => handleViewRaw(deploy)}
                                            >
                                                <FileText className="h-4 w-4" />
                                            </Button>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                className="text-destructive hover:text-destructive"
                                                title="Delete"
                                                onClick={() => setDeleteTarget(deploy.key)}
                                            >
                                                <Trash className="h-4 w-4" />
                                            </Button>
                                        </div>
                                    </div>

                                    {deploy.nfr_requirements && (
                                        <div className="bg-muted/40 rounded-md p-3 mb-4 text-sm grid sm:grid-cols-3 gap-2">
                                            {deploy.nfr_requirements.min_throughput_rps && (
                                                <div className="flex items-center gap-2 text-muted-foreground">
                                                    <Activity className="h-4 w-4" />
                                                    TPS: <span className="text-foreground font-mono">{deploy.nfr_requirements.min_throughput_rps}</span>
                                                </div>
                                            )}
                                            {deploy.nfr_requirements.availability && (
                                                <div className="flex items-center gap-2 text-muted-foreground">
                                                    <Server className="h-4 w-4" />
                                                    Avail: <span className="text-foreground font-mono">{deploy.nfr_requirements.availability}%</span>
                                                </div>
                                            )}
                                            {deploy.nfr_requirements.cpu_utilization_target && (
                                                <div className="flex items-center gap-2 text-muted-foreground">
                                                    <Cpu className="h-4 w-4" />
                                                    CPU: <span className="text-foreground font-mono">{deploy.nfr_requirements.cpu_utilization_target}%</span>
                                                </div>
                                            )}
                                        </div>
                                    )}

                                    {deploy.status?.last_error && (
                                        <div className="bg-destructive/10 text-destructive text-sm p-3 rounded-md mb-4 border border-destructive/20">
                                            Error: {deploy.status.last_error}
                                        </div>
                                    )}

                                    <div className="flex items-center justify-between text-sm">
                                        <div className="flex items-center gap-2">
                                            <span className="text-muted-foreground">Selected Environments:</span>
                                            <div className="flex gap-2">
                                                {deploy.target_envs && deploy.target_envs.map(env => (
                                                    <Badge key={env} variant="outline" className="font-normal">{env}</Badge>
                                                ))}
                                            </div>
                                        </div>
                                        <span className="text-muted-foreground text-xs">
                                            Last reconciled: {deploy.updated_at ? new Date(deploy.updated_at).toLocaleString() : "N/A"}
                                        </span>
                                    </div>
                                </div>
                            </div>
                        </Card>
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
                title="Delete Deployment"
                description={`Are you sure you want to delete deployment "${deleteTarget}"? This will remove it from all environments. This action cannot be undone.`}
                confirmLabel="Delete"
                variant="destructive"
                loading={deleting}
                onConfirm={handleDelete}
            />
        </div>
    );
}
