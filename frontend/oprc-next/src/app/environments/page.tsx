"use client";

import { useState } from "react";
import {
    Globe,
    Search,
    RefreshCcw,
    CheckCircle2,
    AlertTriangle,
    XCircle,
    FileText
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";

// Mock Data
const MOCK_ENVS = [
    {
        name: "cluster-1",
        status: "Healthy",
        crmVersion: "v0.5.0",
        nodes: "3/3 ready",
        avail: "99.95%",
        lastSeen: "30s ago"
    },
    {
        name: "cluster-2",
        status: "Degraded",
        crmVersion: "v0.5.0",
        nodes: "2/3 ready",
        avail: "98.50%",
        lastSeen: "45s ago"
    },
    {
        name: "dev-cluster",
        status: "Unhealthy",
        crmVersion: "v0.4.9",
        nodes: "0/1 ready",
        avail: "0.00%",
        lastSeen: "10m ago"
    },
];

export default function EnvironmentsPage() {
    const [search, setSearch] = useState("");

    const filtered = MOCK_ENVS.filter((e) =>
        e.name.toLowerCase().includes(search.toLowerCase())
    );

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Environments</h1>
                <Button variant="outline">
                    <RefreshCcw className="mr-2 h-4 w-4" /> Refresh
                </Button>
            </div>

            <div className="flex flex-col sm:flex-row gap-4 items-center justify-between">
                <div className="relative flex-1 w-full max-w-sm">
                    <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
                    <Input
                        type="search"
                        placeholder="Search environments..."
                        className="pl-8"
                        value={search}
                        onChange={(e) => setSearch(e.target.value)}
                    />
                </div>
                <div className="text-sm text-muted-foreground">
                    {filtered.length} environment(s) • {filtered.filter(e => e.status === "Healthy").length} healthy
                </div>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                {filtered.length === 0 ? (
                    <div className="col-span-full text-center py-12 text-muted-foreground">
                        No environments found
                    </div>
                ) : (
                    filtered.map((env) => (
                        <Card key={env.name} className="flex flex-col">
                            <div className="p-6 flex items-start justify-between border-b pb-4">
                                <div className="flex items-center gap-2">
                                    <Globe className="h-5 w-5 text-muted-foreground" />
                                    <span className="font-semibold text-lg">{env.name}</span>
                                </div>
                                <Badge
                                    variant={
                                        env.status === "Healthy" ? "success" :
                                            env.status === "Degraded" ? "warning" : "destructive"
                                    }
                                    className="flex items-center gap-1"
                                >
                                    {env.status === "Healthy" && <CheckCircle2 className="h-3 w-3" />}
                                    {env.status === "Degraded" && <AlertTriangle className="h-3 w-3" />}
                                    {env.status === "Unhealthy" && <XCircle className="h-3 w-3" />}
                                    {env.status}
                                </Badge>
                            </div>
                            <div className="p-6 pt-4 flex-1 space-y-3">
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">CRM Version</span>
                                    <span className="font-mono">{env.crmVersion}</span>
                                </div>
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">Nodes</span>
                                    <span>{env.nodes}</span>
                                </div>
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">Availability</span>
                                    <span className={`font-mono font-medium ${parseFloat(env.avail) >= 99 ? "text-green-600 dark:text-green-400" :
                                            parseFloat(env.avail) >= 95 ? "text-yellow-600 dark:text-yellow-400" :
                                                "text-red-600 dark:text-red-400"
                                        }`}>{env.avail}</span>
                                </div>
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">Last Seen</span>
                                    <span className="text-muted-foreground">{env.lastSeen}</span>
                                </div>
                            </div>
                            <div className="p-4 bg-muted/30 border-t flex justify-end">
                                <Button variant="ghost" size="sm" className="h-8">
                                    <FileText className="mr-2 h-3 w-3" /> Details
                                </Button>
                            </div>
                        </Card>
                    ))
                )}
            </div>
        </div>
    );
}
