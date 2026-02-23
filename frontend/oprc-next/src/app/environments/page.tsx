"use client";

import { useState, useEffect, useCallback } from "react";
import {
    Globe,
    Search,
    RefreshCcw,
    CheckCircle2,
    AlertTriangle,
    XCircle,
    FileText,
    Loader2
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog";
import { fetchEnvironments } from "@/lib/api";
import { ClusterInfo } from "@/lib/types";

export default function EnvironmentsPage() {
    const [search, setSearch] = useState("");
    const [environments, setEnvironments] = useState<ClusterInfo[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const loadData = useCallback(async () => {
        try {
            setLoading(true);
            setError(null);
            const data = await fetchEnvironments();
            setEnvironments(data);
        } catch (e) {
            setError(e instanceof Error ? e.message : "Failed to load environments");
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadData();
    }, [loadData]);


    const filtered = environments.filter((e) =>
        e.name.toLowerCase().includes(search.toLowerCase())
    );

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Environments</h1>
                <Button variant="outline" onClick={loadData} disabled={loading}>
                    <RefreshCcw className={`mr-2 h-4 w-4 ${loading ? 'animate-spin' : ''}`} /> Refresh
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
                {loading ? (
                    <div className="col-span-full flex flex-col items-center justify-center py-12 text-muted-foreground space-y-4">
                        <Loader2 className="h-8 w-8 animate-spin text-primary" />
                        <p>Loading environments...</p>
                    </div>
                ) : error ? (
                    <div className="col-span-full text-center py-12 text-destructive">
                        Error: {error}
                    </div>
                ) : filtered.length === 0 ? (
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
                                    <span className="font-mono">{env.crmVersion || "N/A"}</span>
                                </div>
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">Nodes</span>
                                    <span>{env.nodes || "N/A"}</span>
                                </div>
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">Availability</span>
                                    <span className={`font-mono font-medium ${!env.avail || parseFloat(env.avail) >= 99 ? "text-green-600 dark:text-green-400" :
                                        parseFloat(env.avail) >= 95 ? "text-yellow-600 dark:text-yellow-400" :
                                            "text-red-600 dark:text-red-400"
                                        }`}>{env.avail || "N/A"}</span>
                                </div>
                                <div className="flex justify-between text-sm">
                                    <span className="text-muted-foreground">Last Seen</span>
                                    <span className="text-muted-foreground">{env.lastSeen || "N/A"}</span>
                                </div>
                            </div>
                            <div className="p-4 bg-muted/30 border-t flex justify-end">
                                <Dialog>
                                    <DialogTrigger asChild>
                                        <Button variant="ghost" size="sm" className="h-8 group hover:bg-primary/10 hover:text-primary transition-all duration-200">
                                            <FileText className="mr-2 h-3 w-3 group-hover:scale-110 transition-transform duration-200" /> Details
                                        </Button>
                                    </DialogTrigger>
                                    <DialogContent className="max-w-3xl overflow-hidden flex flex-col max-h-[80vh]">
                                        <DialogHeader>
                                            <DialogTitle>Raw Environment Data ({env.name})</DialogTitle>
                                        </DialogHeader>
                                        <div className="flex-1 overflow-auto bg-muted p-4 rounded-md font-mono text-sm whitespace-pre-wrap break-all">
                                            <pre>{JSON.stringify(env.raw, null, 2)}</pre>
                                        </div>
                                    </DialogContent>
                                </Dialog>
                            </div>
                        </Card>
                    ))
                )}
            </div>
        </div>
    );
}
