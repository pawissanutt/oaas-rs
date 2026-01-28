"use client";

import { useState } from "react";
import {
    Search,
    Trash,
    Plus,
    FileEdit,
    Play,
    Box,
    MoreVertical
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";

// Mock Data
const MOCK_OBJECTS = [
    { id: "obj-001", class: "EchoClass", partition: 0, entries: { state: "active", counter: 42 }, events: [] },
    { id: "obj-002", class: "EchoClass", partition: 1, entries: { state: "idle" }, events: [] },
    { id: "obj-003", class: "StorageClass", partition: 0, entries: { data: "blob..." }, events: [] },
];

export default function ObjectsPage() {
    const [selectedId, setSelectedId] = useState<string | null>("obj-001");
    const [search, setSearch] = useState("");

    const filtered = MOCK_OBJECTS.filter((o) =>
        o.id.toLowerCase().includes(search.toLowerCase())
    );

    const selectedObject = MOCK_OBJECTS.find(o => o.id === selectedId);

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
                        <select className="flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50">
                            <option>All Classes</option>
                            <option>EchoClass</option>
                            <option>StorageClass</option>
                        </select>
                        <select className="flex h-10 w-24 items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50">
                            <option value="all">All</option>
                            <option value="0">P-0</option>
                            <option value="1">P-1</option>
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
                        {filtered.map(obj => (
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
                                        {obj.class} • P{obj.partition}
                                    </div>
                                </div>
                                <Button variant="ghost" size="icon" className="opacity-0 group-hover:opacity-100 h-8 w-8">
                                    <MoreVertical className="h-4 w-4" />
                                </Button>
                            </div>
                        ))}
                        {filtered.length === 0 && (
                            <div className="text-center py-8 text-sm text-muted-foreground">
                                No objects found.
                            </div>
                        )}
                    </div>
                </Card>

                {/* 3. Object Detail */}
                <Card className="flex-1 flex flex-col">
                    {selectedObject ? (
                        <>
                            <CardHeader className="py-4 border-b border-border flex flex-row items-center justify-between">
                                <div>
                                    <CardTitle className="text-lg flex items-center gap-2">
                                        <Box className="h-5 w-5 text-primary" />
                                        {selectedObject.id}
                                    </CardTitle>
                                    <p className="text-sm text-muted-foreground mt-1">
                                        Class: <span className="text-foreground">{selectedObject.class}</span> • Partition: <span className="font-mono">{selectedObject.partition}</span>
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
                                                {JSON.stringify(selectedObject.entries, null, 2)}
                                            </pre>
                                        </div>
                                        <div className="border rounded-md p-4">
                                            <h4 className="text-sm font-medium mb-3 flex items-center gap-2">
                                                <Play className="h-4 w-4" /> Invoke Function
                                            </h4>
                                            <div className="grid gap-4">
                                                <select className="flex h-10 w-full items-center justify-between rounded-md border border-input bg-background px-3 py-2 text-sm">
                                                    <option>echo</option>
                                                    <option>process</option>
                                                </select>
                                                <Textarea placeholder='Payload (JSON): {"key": "value"}' className="font-mono" />
                                                <Button>Invoke</Button>
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
