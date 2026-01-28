"use client";

import { useState } from "react";
import {
    Zap,
    Search,
    FileText
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";

// Mock Data
const MOCK_FUNCTIONS = [
    {
        key: "echo",
        package: "my-package",
        version: "v1.0.0",
        type: "Custom",
        desc: "Echo function that returns the input",
        boundTo: "EchoClass.echo (stateless)"
    },
    {
        key: "process",
        package: "my-package",
        version: "v1.0.0",
        type: "Macro",
        desc: "Data processing macro",
        boundTo: "EchoClass.process"
    },
    {
        key: "debug",
        package: "another-pkg",
        version: "-",
        type: "Builtin",
        desc: "System debug utility",
        boundTo: null
    },
    {
        key: "profile",
        package: "my-package",
        version: "v1.0.0",
        type: "Logical",
        desc: "User profile logic",
        boundTo: "UserClass.profile"
    },
];

export default function FunctionsPage() {
    const [search, setSearch] = useState("");
    const [typeFilter, setTypeFilter] = useState("All");

    const filtered = MOCK_FUNCTIONS.filter((f) => {
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
                    <option value="Builtin">Builtin</option>
                    <option value="Custom">Custom</option>
                    <option value="Macro">Macro</option>
                    <option value="Logical">Logical</option>
                </select>
            </div>

            <div className="space-y-4">
                {filtered.length === 0 ? (
                    <div className="text-center py-12 text-muted-foreground">
                        No functions found
                    </div>
                ) : (
                    filtered.map((fn) => (
                        <Card key={fn.key} className="p-6">
                            <div className="flex items-start justify-between">
                                <div>
                                    <div className="flex items-center gap-2 mb-1">
                                        <Zap className="h-5 w-5 text-amber-500" />
                                        <h3 className="text-lg font-semibold">{fn.key}</h3>
                                        <Badge
                                            variant="outline"
                                            className={
                                                fn.type === "Custom" ? "border-blue-200 text-blue-700 dark:text-blue-300" :
                                                    fn.type === "Macro" ? "border-green-200 text-green-700 dark:text-green-300" :
                                                        fn.type === "Builtin" ? "border-gray-200 text-gray-700" : "border-purple-200 text-purple-700"
                                            }
                                        >
                                            {fn.type}
                                        </Badge>
                                    </div>
                                    <div className="text-sm text-muted-foreground mb-2">
                                        Package: <span className="text-foreground">{fn.package} {fn.version}</span>
                                    </div>
                                    {fn.desc && (
                                        <p className="text-sm italic text-muted-foreground mb-3">"{fn.desc}"</p>
                                    )}
                                    {fn.boundTo && (
                                        <div className="text-xs bg-muted inline-block px-2 py-1 rounded border">
                                            Bound to: <span className="font-mono">{fn.boundTo}</span>
                                        </div>
                                    )}
                                </div>
                                <Button variant="ghost" size="icon" title="View Source">
                                    <FileText className="h-4 w-4" />
                                </Button>
                            </div>
                        </Card>
                    ))
                )}
            </div>
        </div>
    );
}
