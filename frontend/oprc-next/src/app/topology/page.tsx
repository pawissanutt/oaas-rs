"use client";

import dynamic from "next/dynamic";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Activity, X, Package, Box, Zap, Server, Network, Globe, Database } from "lucide-react";
import { Badge } from "@/components/ui/badge";

// Dynamically import the Graph component to avoid SSR issues
const TopologyGraph = dynamic(() => import("@/components/features/topology-graph"), {
    ssr: false,
    loading: () => <div className="h-[600px] w-full bg-muted/20 animate-pulse rounded-lg border border-border flex items-center justify-center">Loading Topology...</div>
});

export default function TopologyPage() {
    const [source, setSource] = useState<"deployments" | "zenoh">("deployments");
    const [selectedNode, setSelectedNode] = useState<any>(null);

    return (
        <div className="space-y-6">
            <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4">
                <h1 className="text-3xl font-bold tracking-tight">Topology</h1>
                <div className="flex items-center space-x-2 bg-muted p-1 rounded-lg">
                    <Button
                        variant={source === "deployments" ? "default" : "ghost"}
                        size="sm"
                        onClick={() => setSource("deployments")}
                        className="h-8"
                    >
                        From Deployments
                    </Button>
                    <Button
                        variant={source === "zenoh" ? "default" : "ghost"}
                        size="sm"
                        onClick={() => setSource("zenoh")}
                        className="h-8"
                    >
                        From Zenoh
                    </Button>
                </div>
            </div>

            <div className="text-sm text-muted-foreground">
                12 nodes and 8 connections • Last updated: 2m ago
            </div>

            <div className="relative flex gap-4 h-[600px]">
                {/* Graph Canvas */}
                <Card className="flex-1 overflow-hidden relative">
                    <TopologyGraph
                        source={source}
                        onNodeSelect={(node) => setSelectedNode(node)}
                    />

                    {/* Legend Overlay */}
                    <div className="absolute top-4 left-4 bg-background/90 backdrop-blur border rounded-md p-3 shadow-sm text-xs space-y-2 pointer-events-none z-10">
                        <div className="font-semibold mb-1">Legend</div>
                        <div className="grid grid-cols-2 gap-x-4 gap-y-2">
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-md bg-indigo-500 flex items-center justify-center text-indigo-50">
                                    <Package className="w-3.5 h-3.5" />
                                </div>
                                Package
                            </div>
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-full bg-cyan-500 flex items-center justify-center text-cyan-50">
                                    <Box className="w-3.5 h-3.5" />
                                </div>
                                Class
                            </div>
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-sm bg-amber-500 flex items-center justify-center text-amber-50">
                                    <Zap className="w-3.5 h-3.5" />
                                </div>
                                Function
                            </div>
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-sm bg-pink-500 flex items-center justify-center text-pink-50">
                                    <Server className="w-3.5 h-3.5" />
                                </div>
                                Environment
                            </div>
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-sm bg-purple-500 flex items-center justify-center text-purple-50">
                                    <Network className="w-3.5 h-3.5" />
                                </div>
                                Router
                            </div>
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-sm bg-blue-500 flex items-center justify-center text-blue-50">
                                    <Globe className="w-3.5 h-3.5" />
                                </div>
                                Gateway
                            </div>
                            <div className="flex items-center gap-2">
                                <div className="w-6 h-6 rounded-full bg-emerald-500 flex items-center justify-center text-emerald-50">
                                    <Database className="w-3.5 h-3.5" />
                                </div>
                                ODGM
                            </div>
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
                                <Badge variant="outline">{selectedNode.data.type}</Badge>
                            </div>
                            <div>
                                <div className="text-sm text-muted-foreground mb-1">Status</div>
                                <Badge variant="success" className="bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300 border-none">Healthy</Badge>
                            </div>

                            <div className="border-t pt-4">
                                <h4 className="font-medium text-sm mb-2">Metadata</h4>
                                <div className="space-y-2 text-sm">
                                    <div className="flex justify-between">
                                        <span className="text-muted-foreground">Version</span>
                                        <span>1.0.0</span>
                                    </div>
                                    <div className="flex justify-between">
                                        <span className="text-muted-foreground">Region</span>
                                        <span>us-east-1</span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </Card>
                )}
            </div>
        </div>
    );
}
