"use client";

import { useState, useEffect } from "react";
import {
    Search,
    Trash,
    Plus,
    FileEdit,
    Play,
    Box,
    MoreVertical,
    Loader2
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import {
    fetchDeployments,
    fetchObjects,
    invokeFunction
} from "@/lib/api";
import { OClassDeployment } from "@/lib/bindings/OClassDeployment";
import { OObject } from "@/lib/types";

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
    const [invokeResult, setInvokeResult] = useState<any>(null);

    // Initial load
    useEffect(() => {
        fetchDeployments().then(deps => {
            setDeployments(deps);
            if (deps.length > 0) {
                // Default to first deployment
                setSelectedClass(deps[0].class_key);
                setSelectedPartition(0);
            }
        });
    }, []);

    // Fetch objects when class/partition changes
    useEffect(() => {
        if (!selectedClass) return;

        setLoadingObjects(true);
        fetchObjects(selectedClass, selectedPartition)
            .then(objs => {
                setObjects(objs);
                setLoadingObjects(false);
                if (objs.length > 0) {
                    setSelectedId(objs[0].id);
                } else {
                    setSelectedId(null);
                }
            })
            .catch(err => {
                console.error(err);
                setObjects([]);
                setLoadingObjects(false);
            });
    }, [selectedClass, selectedPartition]);

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

    // Derived state
    const uniqueClasses = Array.from(new Set(deployments.map(d => d.class_key)));
    const currentDeployment = deployments.find(d => d.class_key === selectedClass);
    // Determine max partitions.
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
                                setSelectedPartition(0); // Reset partition
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
                            {/* Dynamic partition dropdown */}
                            {Array.from({ length: maxPartitions }).map((_, i) => (
                                <option key={i} value={i}>P-{i}</option>
                            ))}
                        </select>
                        <Button>
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
                                            {/* Assuming obj.class exists or derived from wrapper? */}
                                            {selectedClass} • P{selectedPartition}
                                        </div>
                                    </div>
                                    <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 h-8 w-8">
                                        <MoreVertical className="h-4 w-4" />
                                    </Button>
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
                                    <Button variant="outline" size="sm">
                                        <FileEdit className="mr-2 h-4 w-4" /> Edit
                                    </Button>
                                    <Button variant="destructive" size="sm">
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
                                        <div className="text-center py-12 text-muted-foreground">
                                            No event triggers configured.
                                        </div>
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
        </div>
    );
}
