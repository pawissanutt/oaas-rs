"use client";

import { useState } from "react";
import {
    Rocket,
    Search,
    Plus,
    Trash,
    FileText,
    Activity,
    Server,
    Cpu
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardHeader, CardContent } from "@/components/ui/card";

// Mock Data
const MOCK_DEPLOYMENTS = [
    {
        key: "my-package.EchoClass",
        package: "my-package",
        class: "EchoClass",
        status: "Running",
        selectedEnvs: ["cluster-1", "cluster-2"],
        lastReconciled: "2m ago",
        nfr: {
            throughput: "1000 rps",
            availability: "99.9%",
            cpu: "70%"
        }
    },
    {
        key: "my-package.StorageClass",
        package: "my-package",
        class: "StorageClass",
        status: "Deploying",
        selectedEnvs: ["cluster-1"],
        lastReconciled: "10s ago",
        nfr: null
    },
    {
        key: "another-pkg.TestClass",
        package: "another-pkg",
        class: "TestClass",
        status: "Error",
        selectedEnvs: ["cluster-2"],
        lastReconciled: "1h ago",
        error: "CrashLoopBackOff: Container failed to start",
        nfr: {
            throughput: "100 rps",
            availability: "99.0%",
            cpu: "-"
        }
    }
];

export default function DeploymentsPage() {
    const [search, setSearch] = useState("");

    const filtered = MOCK_DEPLOYMENTS.filter((d) =>
        d.key.toLowerCase().includes(search.toLowerCase())
    );

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Deployments</h1>
                <Button>
                    <Plus className="mr-2 h-4 w-4" /> New Deployment
                </Button>
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
                {filtered.length === 0 ? (
                    <div className="text-center py-12 text-muted-foreground">
                        No deployments found
                    </div>
                ) : (
                    filtered.map((deploy) => (
                        <Card key={deploy.key} className="overflow-hidden">
                            <div className="flex flex-col sm:flex-row border-l-4 border-l-transparent data-[status=Running]:border-l-green-500 data-[status=Deploying]:border-l-yellow-500 data-[status=Error]:border-l-red-500" data-status={deploy.status}>
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
                                                        deploy.status === "Running" ? "success" :
                                                            deploy.status === "Deploying" ? "warning" : "destructive"
                                                    }
                                                >
                                                    {deploy.status}
                                                </Badge>
                                            </div>
                                            <div className="text-sm text-muted-foreground mb-4">
                                                Package: <span className="text-foreground">{deploy.package}</span> • Class: <span className="text-foreground">{deploy.class}</span>
                                            </div>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            <Button variant="ghost" size="icon" title="View Raw">
                                                <FileText className="h-4 w-4" />
                                            </Button>
                                            <Button variant="ghost" size="icon" className="text-destructive hover:text-destructive" title="Delete">
                                                <Trash className="h-4 w-4" />
                                            </Button>
                                        </div>
                                    </div>

                                    {deploy.nfr && (
                                        <div className="bg-muted/40 rounded-md p-3 mb-4 text-sm grid sm:grid-cols-3 gap-2">
                                            <div className="flex items-center gap-2 text-muted-foreground">
                                                <Activity className="h-4 w-4" />
                                                TPS: <span className="text-foreground font-mono">{deploy.nfr.throughput}</span>
                                            </div>
                                            <div className="flex items-center gap-2 text-muted-foreground">
                                                <Server className="h-4 w-4" />
                                                Avail: <span className="text-foreground font-mono">{deploy.nfr.availability}</span>
                                            </div>
                                            <div className="flex items-center gap-2 text-muted-foreground">
                                                <Cpu className="h-4 w-4" />
                                                CPU: <span className="text-foreground font-mono">{deploy.nfr.cpu}</span>
                                            </div>
                                        </div>
                                    )}

                                    {deploy.error && (
                                        <div className="bg-destructive/10 text-destructive text-sm p-3 rounded-md mb-4 border border-destructive/20">
                                            Error: {deploy.error}
                                        </div>
                                    )}

                                    <div className="flex items-center justify-between text-sm">
                                        <div className="flex items-center gap-2">
                                            <span className="text-muted-foreground">Selected Environments:</span>
                                            <div className="flex gap-2">
                                                {deploy.selectedEnvs.map(env => (
                                                    <Badge key={env} variant="outline" className="font-normal">{env}</Badge>
                                                ))}
                                            </div>
                                        </div>
                                        <span className="text-muted-foreground text-xs">
                                            Last reconciled: {deploy.lastReconciled}
                                        </span>
                                    </div>
                                </div>
                            </div>
                        </Card>
                    ))
                )}
            </div>
        </div>
    );
}
