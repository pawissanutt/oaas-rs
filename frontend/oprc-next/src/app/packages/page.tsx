"use client";

import { useState } from "react";
import {
    Package,
    Search,
    Trash,
    FileText,
    Tags,
    Zap,
    Plus
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
} from "@/components/ui/dialog";
import { Textarea } from "@/components/ui/textarea";

// Mock Data
const MOCK_PACKAGES = [
    {
        name: "my-package",
        version: "v1.0.0",
        classCount: 3,
        functionCount: 5,
        description: "Core utility package",
        classes: [
            { name: "EchoClass", partitions: 8, functions: ["echo", "process"] },
            { name: "StorageClass", partitions: 16, functions: ["read", "write"] },
            { name: "UserClass", partitions: 4, functions: ["profile"] },
        ],
        functions: [
            { name: "echo", type: "Custom", desc: "Echo function" },
            { name: "process", type: "Macro", desc: "" },
            { name: "read", type: "Custom", desc: "" },
            { name: "write", type: "Custom", desc: "" },
            { name: "profile", type: "Logical", desc: "" },
        ],
    },
    {
        name: "another-pkg",
        version: "-",
        classCount: 1,
        functionCount: 2,
        description: "Another test package",
        classes: [
            { name: "TestClass", partitions: 1, functions: ["test"] }
        ],
        functions: [
            { name: "test", type: "Custom", desc: "" },
            { name: "debug", type: "Builtin", desc: "" },
        ],
    },
];

export default function PackagesPage() {
    const [search, setSearch] = useState("");

    const filtered = MOCK_PACKAGES.filter((p) =>
        p.name.toLowerCase().includes(search.toLowerCase())
    );

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Packages</h1>
                <Dialog>
                    <DialogTrigger asChild>
                        <Button>
                            <Plus className="mr-2 h-4 w-4" /> New Package
                        </Button>
                    </DialogTrigger>
                    <DialogContent className="max-w-2xl h-[80vh] flex flex-col">
                        <DialogHeader>
                            <DialogTitle>Create Package</DialogTitle>
                        </DialogHeader>
                        <div className="flex-1 min-h-0 flex flex-col gap-4 py-4">
                            <p className="text-sm text-muted-foreground">
                                Define your package structure in YAML format.
                            </p>
                            <div className="flex-1 relative border rounded-md">
                                {/* Simple Line Numbers Mock */}
                                <div className="absolute left-0 top-0 bottom-0 w-8 bg-muted border-r flex flex-col items-center pt-2 text-xs text-muted-foreground select-none">
                                    {Array.from({ length: 20 }).map((_, i) => (
                                        <div key={i} className="h-5">{i + 1}</div>
                                    ))}
                                </div>
                                <Textarea
                                    className="w-full h-full pl-10 font-mono text-sm resize-none border-0 focus-visible:ring-0"
                                    placeholder={`name: my-new-package
version: 0.1.0
classes: []
functions: []`}
                                />
                            </div>
                            <div className="flex items-center space-x-2">
                                <input type="checkbox" id="deploy" className="rounded border-gray-300" />
                                <label htmlFor="deploy" className="text-sm font-medium">Also deploy all classes</label>
                            </div>
                        </div>
                        <DialogFooter>
                            <Button variant="outline">Cancel</Button>
                            <Button>Apply</Button>
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
                {filtered.length === 0 ? (
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
                                            <Tags className="h-3 w-3" /> {pkg.classCount} classes
                                            <span className="text-border">|</span>
                                            <Zap className="h-3 w-3" /> {pkg.functionCount} functions
                                        </div>
                                    </div>
                                </div>
                                <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                                    <Button variant="ghost" size="icon" title="View Raw">
                                        <FileText className="h-4 w-4" />
                                    </Button>
                                    <Button variant="ghost" size="icon" className="text-destructive hover:text-destructive" title="Delete">
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
                                                <div key={cls.name} className="p-3 bg-muted/50 rounded-md border border-border">
                                                    <div className="font-medium flex items-center gap-2">
                                                        <div className="w-2 h-2 rounded-full bg-cyan-500" />
                                                        {cls.name}
                                                    </div>
                                                    <div className="text-xs text-muted-foreground mt-1 ml-4">
                                                        Partitions: {cls.partitions} • Functions: {cls.functions.join(", ")}
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
                                                <div key={fn.name} className="p-3 bg-muted/50 rounded-md border border-border flex items-center justify-between">
                                                    <div className="font-medium flex items-center gap-2 font-mono text-sm">
                                                        <Zap className="h-3 w-3 text-amber-500" />
                                                        {fn.name}
                                                    </div>
                                                    <Badge variant="outline" className={
                                                        fn.type === "Custom" ? "border-blue-200 text-blue-700 dark:text-blue-300" :
                                                            fn.type === "Macro" ? "border-green-200 text-green-700 dark:text-green-300" :
                                                                fn.type === "Builtin" ? "border-gray-200 text-gray-700" : "border-purple-200 text-purple-700"
                                                    }>
                                                        {fn.type}
                                                    </Badge>
                                                </div>
                                            ))}
                                        </div>
                                    </div>
                                </div>
                                {pkg.description && (
                                    <div className="mt-4 pt-4 border-t border-border/50 text-sm text-muted-foreground">
                                        {pkg.description}
                                    </div>
                                )}
                            </div>
                        </details>
                    ))
                )}
            </div>
        </div>
    );
}
