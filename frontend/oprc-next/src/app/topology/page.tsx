"use client";

import dynamic from "next/dynamic";
import { useState, useEffect, useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Activity, X, Package, Box, Zap, Server, Network, Globe, Database, RefreshCw, Loader2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Node, Edge, MarkerType } from "@xyflow/react";
import { fetchPackages, fetchDeployments, fetchEnvironments } from "@/lib/api";
import { OPackage } from "@/lib/bindings/OPackage";
import { OClassDeployment } from "@/lib/bindings/OClassDeployment";

// Dynamically import the Graph component to avoid SSR issues
const TopologyGraph = dynamic(() => import("@/components/features/topology-graph"), {
    ssr: false,
    loading: () => <div className="h-[600px] w-full bg-muted/20 animate-pulse rounded-lg border border-border flex items-center justify-center">Loading Topology...</div>
});

export default function TopologyPage() {
    const [source, setSource] = useState<"deployments" | "zenoh">("deployments");
    const [selectedNode, setSelectedNode] = useState<Node | null>(null);
    const [loading, setLoading] = useState(true);
    const [data, setData] = useState<{ nodes: Node[], edges: Edge[] }>({ nodes: [], edges: [] });
    const [lastUpdated, setLastUpdated] = useState<Date>(new Date());

    const refreshData = async () => {
        setLoading(true);
        try {
            const [packages, deployments, envs] = await Promise.all([
                fetchPackages(),
                fetchDeployments(),
                fetchEnvironments()
            ]);

            const nodes: Node[] = [];
            const edges: Edge[] = [];
            const yOffset = 0;

            // 1. Environments
            envs.forEach((env, i) => {
                nodes.push({
                    id: `env-${env.name}`,
                    type: 'topologyNode',
                    data: { label: env.name, type: "Environment", status: env.status },
                    position: { x: 0, y: 0 } // Layout will handle pos
                });
            });

            // 2. Packages
            packages.forEach((pkg, i) => {
                const pkgId = `pkg-${pkg.name}`;
                nodes.push({
                    id: pkgId,
                    type: 'topologyNode',
                    data: { label: pkg.name, type: "Package", version: pkg.version },
                    position: { x: 0, y: 0 }
                });

                // Classes in Package
                pkg.classes.forEach((cls) => {
                    const clsId = `cls-${pkg.name}-${cls.key}`;
                    nodes.push({
                        id: clsId,
                        type: 'topologyNode',
                        data: { label: cls.key, type: "Class" },
                        position: { x: 0, y: 0 }
                    });
                    edges.push({
                        id: `e-${pkgId}-${clsId}`,
                        source: pkgId,
                        target: clsId,
                        markerEnd: { type: MarkerType.ArrowClosed }
                    });

                    // Functions in Class
                    // Usually functions are part of OPackage functions list, but bound to class?
                    // OClass has function_bindings.
                    cls.function_bindings.forEach(fb => {
                        const fn = pkg.functions.find(f => f.key === fb.function_key);
                        if (fn) {
                            const fnId = `fn-${pkg.name}-${cls.key}-${fn.key}`;
                            // Avoid duplicates if function is reused across classes?
                            // Actually nodes ID must be unique. If same function used in multiple classes, duplicate node?
                            // Or one function node connected to multiple classes?
                            // Let's create unique function nodes per class context for tree view.
                            nodes.push({
                                id: fnId,
                                type: 'topologyNode',
                                data: { label: fn.key, type: "Function", fnType: fn.function_type },
                                position: { x: 0, y: 0 }
                            });
                            edges.push({
                                id: `e-${clsId}-${fnId}`,
                                source: clsId,
                                target: fnId,
                                markerEnd: { type: MarkerType.ArrowClosed }
                            });
                        }
                    });
                });
            });

            // 3. Deployments (Link Classes to Environments)
            deployments.forEach((dep) => {
                const clsId = `cls-${dep.package_name}-${dep.class_key}`;
                // Link to Target Envs
                dep.target_envs.forEach(envName => {
                    const envId = `env-${envName}`;
                    // Create edge from Env to Class (Deployment) or Class to Env?
                    // Usually Env hosts Class.
                    if (nodes.find(n => n.id === envId) && nodes.find(n => n.id === clsId)) {
                        edges.push({
                            id: `dep-${envId}-${clsId}`,
                            source: envId,
                            target: clsId,
                            animated: true,
                            label: "deploys",
                            markerEnd: { type: MarkerType.ArrowClosed }
                        });
                    }
                });
            });

            setData({ nodes, edges });
            setLastUpdated(new Date());
        } catch (e) {
            console.error(e);
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        refreshData();
    }, [source]);

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Topology</h1>
                <div className="flex items-center gap-4">
                    <div className="text-sm text-muted-foreground flex items-center gap-1">
                        {loading && <Loader2 className="h-3 w-3 animate-spin" />}
                        {data.nodes.length} nodes • {data.edges.length} connections
                    </div>
                    <div className="flex items-center space-x-2 bg-muted p-1 rounded-lg">
                        <Button
                            variant={source === "deployments" ? "default" : "ghost"}
                            size="sm"
                            onClick={() => setSource("deployments")}
                            className="h-8"
                        >
                            Deployments
                        </Button>
                        <Button
                            variant={source === "zenoh" ? "default" : "ghost"}
                            size="sm"
                            onClick={() => setSource("zenoh")}
                            className="h-8"
                        >
                            Zenoh
                        </Button>
                    </div>
                    <Button variant="outline" size="icon" onClick={refreshData} title="Refresh">
                        <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
                    </Button>
                </div>
            </div>

            <div className="relative flex gap-4 h-[600px]">
                {/* Graph Canvas */}
                <Card className="flex-1 overflow-hidden relative">
                    <TopologyGraph
                        data={data}
                        onNodeSelect={(node) => setSelectedNode(node)}
                    />

                    {/* Legend Overlay */}
                    <div className="absolute top-4 left-4 bg-background/90 backdrop-blur border rounded-md p-3 shadow-sm text-xs space-y-2 pointer-events-none z-10">
                        <div className="font-semibold mb-1">Legend</div>
                        <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                            {[
                                { type: "Package", icon: Package, color: "bg-indigo-500", text: "indigo-50" },
                                { type: "Class", icon: Box, color: "bg-cyan-500", text: "cyan-50" },
                                { type: "Function", icon: Zap, color: "bg-amber-500", text: "amber-50" },
                                { type: "Environment", icon: Server, color: "bg-pink-500", text: "pink-50" },
                                { type: "Router", icon: Network, color: "bg-purple-500", text: "purple-50" },
                                { type: "Gateway", icon: Globe, color: "bg-blue-500", text: "blue-50" },
                                { type: "ODGM", icon: Database, color: "bg-emerald-500", text: "emerald-50" },
                            ].map(({ type, icon: Icon, color, text }) => (
                                <div key={type} className="flex items-center gap-2">
                                    <div className={`w-6 h-6 rounded-md ${color} flex items-center justify-center text-${text}`}>
                                        <Icon className="w-3.5 h-3.5 text-white" />
                                    </div>
                                    {type}
                                </div>
                            ))}
                        </div>
                    </div>
                </Card>

                {/* Side Panel */}
                {selectedNode && (
                    <Card className="w-80 flex flex-col absolute right-4 top-4 bottom-4 z-10 shadow-xl animate-in slide-in-from-right-10 border-l">
                        <div className="p-4 border-b flex items-center justify-between bg-muted/30">
                            <div className="font-semibold">Node Details</div>
                            <Button variant="ghost" size="icon" className="h-6 w-6" onClick={() => setSelectedNode(null)}>
                                <X className="h-4 w-4" />
                            </Button>
                        </div>
                        <div className="p-4 flex-1 overflow-auto space-y-4">
                            <div>
                                <div className="text-sm text-muted-foreground mb-1">ID</div>
                                <div className="font-mono text-sm break-all">{selectedNode.id}</div>
                            </div>
                            <div>
                                <div className="text-sm text-muted-foreground mb-1">Type</div>
                                <Badge variant="outline">{selectedNode.data.type as string}</Badge>
                            </div>

                            <div className="border-t pt-4">
                                <h4 className="font-medium text-sm mb-2">Metadata</h4>
                                <div className="space-y-2 text-sm">
                                    {Object.entries(selectedNode.data).map(([k, v]) => (
                                        typeof v === 'string' || typeof v === 'number' ? (
                                            <div key={k} className="flex justify-between gap-2">
                                                <span className="text-muted-foreground capitalize">{k}</span>
                                                <span className="truncate max-w-[150px]">{v}</span>
                                            </div>
                                        ) : null
                                    ))}
                                </div>
                            </div>
                        </div>
                    </Card>
                )}
            </div>
        </div>
    );
}
