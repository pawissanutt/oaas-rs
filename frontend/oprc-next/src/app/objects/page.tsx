"use client";

import { useState, useEffect, useCallback } from "react";
import {
    Search,
    Trash,
    Plus,
    FileEdit,
    Play,
    Box,
    Loader2,
    Eye,
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogFooter,
    DialogDescription,
} from "@/components/ui/dialog";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { RawJsonDialog } from "@/components/ui/raw-json-dialog";
import {
    DropdownMenu,
    DropdownMenuTrigger,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu";
import {
    fetchDeployments,
    fetchObjects,
    fetchObject,
    invokeFunction,
    createOrUpdateObject,
    deleteObject as deleteObjectApi,
} from "@/lib/api";
import { OClassDeployment } from "@/lib/bindings/OClassDeployment";
import { OObject } from "@/lib/types";
import { toast } from "sonner";

// Object ID validation: lowercase, max 160 chars, [a-z0-9._:-]
const OBJ_ID_REGEX = /^[a-z0-9._:\-]+$/;

export default function ObjectsPage() {
    const [deployments, setDeployments] = useState<OClassDeployment[]>([]);
    const [selectedClass, setSelectedClass] = useState<string>("");
    const [selectedPartition, setSelectedPartition] = useState<number>(0);
    const [objects, setObjects] = useState<OObject[]>([]);
    const [loadingObjects, setLoadingObjects] = useState(false);

    // Selection state
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [search, setSearch] = useState("");

    // Invocation state
    const [invokeFn, setInvokeFn] = useState("echo");
    const [invokePayload, setInvokePayload] = useState("{}");
    const [invoking, setInvoking] = useState(false);
    const [invokeResult, setInvokeResult] = useState<unknown>(null);

    // Create object dialog
    const [createOpen, setCreateOpen] = useState(false);
    const [newObjId, setNewObjId] = useState("");
    const [newObjEntries, setNewObjEntries] = useState("{}");
    const [creatingObj, setCreatingObj] = useState(false);

    // Edit object dialog
    const [editOpen, setEditOpen] = useState(false);
    const [editObjId, setEditObjId] = useState("");
    const [editObjEntries, setEditObjEntries] = useState("{}");
    const [editingObj, setEditingObj] = useState(false);
    const [loadingEdit, setLoadingEdit] = useState(false);

    // Delete confirm
    const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
    const [deletingObj, setDeletingObj] = useState(false);

    // View raw
    const [rawDialogOpen, setRawDialogOpen] = useState(false);
    const [rawData, setRawData] = useState<unknown>(null);
    const [rawTitle, setRawTitle] = useState("");

    // Initial load
    useEffect(() => {
        fetchDeployments().then(deps => {
            setDeployments(deps);
            if (deps.length > 0) {
                setSelectedClass(deps[0].class_key);
                setSelectedPartition(0);
            }
        });
    }, []);

    const loadObjects = useCallback(async () => {
        if (!selectedClass) return;

        setLoadingObjects(true);
        try {
            const objs = await fetchObjects(selectedClass, selectedPartition);
            setObjects(objs);
            if (objs.length > 0) {
                setSelectedId(objs[0].id);
            } else {
                setSelectedId(null);
            }
        } catch (err) {
            console.error(err);
            setObjects([]);
        } finally {
            setLoadingObjects(false);
        }
    }, [selectedClass, selectedPartition]);

    // Fetch objects when class/partition changes
    useEffect(() => {
        loadObjects();
    }, [loadObjects]);

    const handleInvoke = async () => {
        if (!selectedId || !selectedClass) return;

        setInvoking(true);
        setInvokeResult(null);
        try {
            const payload = JSON.parse(invokePayload);
            const res = await invokeFunction(selectedClass, selectedPartition, selectedId, invokeFn, payload);
            setInvokeResult(res);
        } catch (e) {
            console.error(e);
            setInvokeResult({ error: String(e) });
        } finally {
            setInvoking(false);
        }
    };

    const handleCreateObject = async () => {
        if (!newObjId) {
            toast.error("Object ID is required");
            return;
        }
        if (!OBJ_ID_REGEX.test(newObjId)) {
            toast.error("Object ID must be lowercase alphanumeric with . _ : - only");
            return;
        }
        if (newObjId.length > 160) {
            toast.error("Object ID must be at most 160 characters");
            return;
        }

        setCreatingObj(true);
        try {
            const entries = JSON.parse(newObjEntries);
            await createOrUpdateObject(selectedClass, selectedPartition, newObjId, { entries });
            toast.success(`Object "${newObjId}" created`);
            setCreateOpen(false);
            setNewObjId("");
            setNewObjEntries("{}");
            await loadObjects();
        } catch (e) {
            toast.error(`${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setCreatingObj(false);
        }
    };

    const handleEditObject = async (objId: string) => {
        setEditObjId(objId);
        setLoadingEdit(true);
        setEditOpen(true);
        try {
            const data = await fetchObject(selectedClass, selectedPartition, objId);
            setEditObjEntries(JSON.stringify((data as Record<string, unknown>)?.entries || {}, null, 2));
        } catch (e) {
            toast.error(`Failed to load object: ${e instanceof Error ? e.message : String(e)}`);
            setEditObjEntries("{}");
        } finally {
            setLoadingEdit(false);
        }
    };

    const handleSaveEdit = async () => {
        setEditingObj(true);
        try {
            const entries = JSON.parse(editObjEntries);
            await createOrUpdateObject(selectedClass, selectedPartition, editObjId, { entries });
            toast.success(`Object "${editObjId}" updated`);
            setEditOpen(false);
            await loadObjects();
        } catch (e) {
            toast.error(`${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setEditingObj(false);
        }
    };

    const handleDeleteObject = async () => {
        if (!deleteTarget) return;
        setDeletingObj(true);
        try {
            await deleteObjectApi(selectedClass, selectedPartition, deleteTarget);
            toast.success(`Object "${deleteTarget}" deleted`);
            setDeleteTarget(null);
            await loadObjects();
        } catch (e) {
            toast.error(`${e instanceof Error ? e.message : String(e)}`);
        } finally {
            setDeletingObj(false);
        }
    };

    const handleViewRaw = async (objId: string) => {
        try {
            const data = await fetchObject(selectedClass, selectedPartition, objId);
            setRawTitle(`Object: ${objId}`);
            setRawData(data);
            setRawDialogOpen(true);
        } catch (e) {
            toast.error(`Failed to load object: ${e instanceof Error ? e.message : String(e)}`);
        }
    };

    // Derived state
    const uniqueClasses = Array.from(new Set(deployments.map(d => d.class_key)));
    const currentDeployment = deployments.find(d => d.class_key === selectedClass);
    const maxPartitions = currentDeployment?.odgm?.partition_count ?? 1;

    const filteredObjects = objects.filter((o) =>
        o.id.toLowerCase().includes(search.toLowerCase())
    );

    const checkSelectedObject = objects.find(o => o.id === selectedId);

    return (
        <div className="flex flex-col h-[calc(100vh-8rem)] gap-4">
            {/* 1. Filter Bar */}
            <Card className="flex-none p-4">
                <div className="flex flex-col sm:flex-row gap-4 items-center">
                    <div className="flex-1 w-full sm:w-auto relative">
                        <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                        <Input
                            placeholder="Filter objects by ID..."
                            className="pl-8"
                            value={search}
                            onChange={(e) => setSearch(e.target.value)}
                        />
                    </div>
                    <div className="flex items-center gap-2 w-full sm:w-auto">
                        <select
                            className="flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                            value={selectedClass}
                            onChange={(e) => {
                                setSelectedClass(e.target.value);
                                setSelectedPartition(0);
                            }}
                        >
                            <option value="" disabled>Select Class</option>
                            {uniqueClasses.map(cls => (
                                <option key={cls} value={cls}>{cls}</option>
                            ))}
                        </select>
                        <select
                            className="flex h-10 w-24 items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
                            value={selectedPartition}
                            onChange={(e) => setSelectedPartition(Number(e.target.value))}
                        >
                            {Array.from({ length: maxPartitions }).map((_, i) => (
                                <option key={i} value={i}>P-{i}</option>
                            ))}
                        </select>
                        <Button onClick={() => setCreateOpen(true)} disabled={!selectedClass}>
                            <Plus className="mr-2 h-4 w-4" /> New
                        </Button>
                    </div>
                </div>
            </Card>

            <div className="flex-1 flex gap-4 min-h-0">
                {/* 2. Object List */}
                <Card className="w-1/3 flex flex-col min-w-[250px]">
                    <CardHeader className="py-4 border-b border-border">
                        <CardTitle className="text-sm font-medium">Object List</CardTitle>
                    </CardHeader>
                    <div className="flex-1 overflow-auto p-2 space-y-2">
                        {loadingObjects ? (
                            <div className="flex items-center justify-center py-8">
                                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                            </div>
                        ) : filteredObjects.length === 0 ? (
                            <div className="text-center py-8 text-sm text-muted-foreground">
                                No objects found.
                            </div>
                        ) : (
                            filteredObjects.map(obj => (
                                <div
                                    key={obj.id}
                                    onClick={() => setSelectedId(obj.id)}
                                    className={`
                                        p-3 rounded-md cursor-pointer border transition-colors flex items-center justify-between group
                                        ${selectedId === obj.id
                                            ? "bg-accent border-primary/50"
                                            : "hover:bg-muted border-transparent hover:border-border"
                                        }
                                    `}
                                >
                                    <div className="flex flex-col gap-1 items-start overflow-hidden">
                                        <div className="font-medium flex items-center gap-2 text-sm">
                                            <Box className="h-4 w-4 text-muted-foreground" />
                                            {obj.id}
                                        </div>
                                        <div className="text-xs text-muted-foreground">
                                            {selectedClass} • P{selectedPartition}
                                        </div>
                                    </div>
                                    <DropdownMenu>
                                        <DropdownMenuTrigger asChild>
                                            <Button
                                                variant="ghost"
                                                size="icon"
                                                className="opacity-0 group-hover:opacity-100 h-8 w-8"
                                                onClick={(e) => e.stopPropagation()}
                                            >
                                                <svg width="15" height="15" viewBox="0 0 15 15" fill="none" xmlns="http://www.w3.org/2000/svg" className="h-4 w-4">
                                                    <path d="M3.625 7.5C3.625 8.12132 3.12132 8.625 2.5 8.625C1.87868 8.625 1.375 8.12132 1.375 7.5C1.375 6.87868 1.87868 6.375 2.5 6.375C3.12132 6.375 3.625 6.87868 3.625 7.5ZM8.625 7.5C8.625 8.12132 8.12132 8.625 7.5 8.625C6.87868 8.625 6.375 8.12132 6.375 7.5C6.375 6.87868 6.87868 6.375 7.5 6.375C8.12132 6.375 8.625 6.87868 8.625 7.5ZM13.625 7.5C13.625 8.12132 13.1213 8.625 12.5 8.625C11.8787 8.625 11.375 8.12132 11.375 7.5C11.375 6.87868 11.8787 6.375 12.5 6.375C13.1213 6.375 13.625 6.87868 13.625 7.5Z" fill="currentColor" />
                                                </svg>
                                            </Button>
                                        </DropdownMenuTrigger>
                                        <DropdownMenuContent align="end">
                                            <DropdownMenuItem onClick={() => handleViewRaw(obj.id)}>
                                                <Eye className="h-4 w-4" />
                                                View Raw
                                            </DropdownMenuItem>
                                            <DropdownMenuItem onClick={() => handleEditObject(obj.id)}>
                                                <FileEdit className="h-4 w-4" />
                                                Edit
                                            </DropdownMenuItem>
                                            <DropdownMenuSeparator />
                                            <DropdownMenuItem
                                                className="text-destructive focus:text-destructive"
                                                onClick={() => setDeleteTarget(obj.id)}
                                            >
                                                <Trash className="h-4 w-4" />
                                                Delete
                                            </DropdownMenuItem>
                                        </DropdownMenuContent>
                                    </DropdownMenu>
                                </div>
                            ))
                        )}
                    </div>
                </Card>

                {/* 3. Object Detail */}
                <Card className="flex-1 flex flex-col">
                    {checkSelectedObject ? (
                        <>
                            <CardHeader className="py-4 border-b border-border flex flex-row items-center justify-between">
                                <div>
                                    <CardTitle className="text-lg flex items-center gap-2">
                                        <Box className="h-5 w-5 text-primary" />
                                        {checkSelectedObject.id}
                                    </CardTitle>
                                    <p className="text-sm text-muted-foreground mt-1">
                                        Class: <span className="text-foreground">{selectedClass}</span> • Partition: <span className="font-mono">{selectedPartition}</span>
                                    </p>
                                </div>
                                <div className="flex items-center gap-2">
                                    <Button variant="outline" size="sm" onClick={() => handleEditObject(checkSelectedObject.id)}>
                                        <FileEdit className="mr-2 h-4 w-4" /> Edit
                                    </Button>
                                    <Button variant="destructive" size="sm" onClick={() => setDeleteTarget(checkSelectedObject.id)}>
                                        <Trash className="mr-2 h-4 w-4" /> Delete
                                    </Button>
                                </div>
                            </CardHeader>
                            <div className="flex-1 overflow-auto p-4">
                                <Tabs defaultValue="entries" className="w-full">
                                    <TabsList className="grid w-full grid-cols-2 mb-4">
                                        <TabsTrigger value="entries">Entries</TabsTrigger>
                                        <TabsTrigger value="events">Events</TabsTrigger>
                                    </TabsList>
                                    <TabsContent value="entries" className="space-y-4">
                                        <div className="border rounded-md p-4 bg-muted/30">
                                            <pre className="text-sm font-mono overflow-auto">
                                                {JSON.stringify(checkSelectedObject.entries || {}, null, 2)}
                                            </pre>
                                        </div>
                                        <div className="border rounded-md p-4">
                                            <h4 className="text-sm font-medium mb-3 flex items-center gap-2">
                                                <Play className="h-4 w-4" /> Invoke Function
                                            </h4>
                                            <div className="grid gap-4">
                                                <div className="grid grid-cols-4 gap-4">
                                                    <Input
                                                        placeholder="Function Name"
                                                        className="col-span-1"
                                                        value={invokeFn}
                                                        onChange={(e) => setInvokeFn(e.target.value)}
                                                    />
                                                    <Textarea
                                                        placeholder='Payload (JSON): {"key": "value"}'
                                                        className="font-mono col-span-3 h-20"
                                                        value={invokePayload}
                                                        onChange={(e) => setInvokePayload(e.target.value)}
                                                    />
                                                </div>
                                                <Button onClick={handleInvoke} disabled={invoking}>
                                                    {invoking ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Play className="mr-2 h-4 w-4" />}
                                                    Invoke
                                                </Button>
                                                {!!invokeResult && (
                                                    <div className="mt-2 p-2 bg-muted rounded border text-sm font-mono whitespace-pre-wrap">
                                                        {JSON.stringify(invokeResult, null, 2) || "Undefined"}
                                                    </div>
                                                )}
                                            </div>
                                        </div>
                                    </TabsContent>
                                    <TabsContent value="events">
                                        {checkSelectedObject.events && checkSelectedObject.events.length > 0 ? (
                                            <div className="space-y-2">
                                                {checkSelectedObject.events.map((evt, i) => (
                                                    <div key={i} className="border rounded-md p-3 bg-muted/30">
                                                        <div className="text-sm font-medium">Type: {evt.type}</div>
                                                        {evt.target && (
                                                            <div className="text-xs text-muted-foreground mt-1">Target: {evt.target}</div>
                                                        )}
                                                    </div>
                                                ))}
                                            </div>
                                        ) : (
                                            <div className="text-center py-12 text-muted-foreground">
                                                No event triggers configured.
                                            </div>
                                        )}
                                    </TabsContent>
                                </Tabs>
                            </div>
                        </>
                    ) : (
                        <div className="flex-1 flex items-center justify-center text-muted-foreground">
                            Select an object to view details
                        </div>
                    )}
                </Card>
            </div>

            {/* Create Object Dialog */}
            <Dialog open={createOpen} onOpenChange={setCreateOpen}>
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>Create Object</DialogTitle>
                        <DialogDescription>
                            Create a new object in {selectedClass} (partition {selectedPartition}).
                        </DialogDescription>
                    </DialogHeader>
                    <div className="grid gap-4 py-4">
                        <div className="grid gap-2">
                            <Label>Object ID</Label>
                            <Input
                                value={newObjId}
                                onChange={(e) => setNewObjId(e.target.value.toLowerCase())}
                                placeholder="my-object-id"
                            />
                            <p className="text-xs text-muted-foreground">
                                Lowercase, max 160 chars. Allowed: a-z, 0-9, . _ : -
                            </p>
                        </div>
                        <div className="grid gap-2">
                            <Label>Entries (JSON)</Label>
                            <Textarea
                                className="font-mono h-32"
                                value={newObjEntries}
                                onChange={(e) => setNewObjEntries(e.target.value)}
                                spellCheck={false}
                            />
                        </div>
                    </div>
                    <DialogFooter>
                        <Button variant="outline" onClick={() => setCreateOpen(false)} disabled={creatingObj}>
                            Cancel
                        </Button>
                        <Button onClick={handleCreateObject} disabled={creatingObj}>
                            {creatingObj && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                            Create
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            {/* Edit Object Dialog */}
            <Dialog open={editOpen} onOpenChange={setEditOpen}>
                <DialogContent className="max-w-lg">
                    <DialogHeader>
                        <DialogTitle>Edit Object: {editObjId}</DialogTitle>
                        <DialogDescription>
                            Modify entries for this object.
                        </DialogDescription>
                    </DialogHeader>
                    <div className="grid gap-4 py-4">
                        {loadingEdit ? (
                            <div className="flex justify-center py-8">
                                <Loader2 className="h-6 w-6 animate-spin" />
                            </div>
                        ) : (
                            <div className="grid gap-2">
                                <Label>Entries (JSON)</Label>
                                <Textarea
                                    className="font-mono h-48"
                                    value={editObjEntries}
                                    onChange={(e) => setEditObjEntries(e.target.value)}
                                    spellCheck={false}
                                />
                            </div>
                        )}
                    </div>
                    <DialogFooter>
                        <Button variant="outline" onClick={() => setEditOpen(false)} disabled={editingObj}>
                            Cancel
                        </Button>
                        <Button onClick={handleSaveEdit} disabled={editingObj || loadingEdit}>
                            {editingObj && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                            Save
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

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
                title="Delete Object"
                description={`Are you sure you want to delete object "${deleteTarget}"? This action cannot be undone.`}
                confirmLabel="Delete"
                variant="destructive"
                loading={deletingObj}
                onConfirm={handleDeleteObject}
            />
        </div>
    );
}
