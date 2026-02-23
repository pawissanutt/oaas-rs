"use client";

import React, { useCallback, useEffect } from "react";
import {
    ReactFlow,
    MiniMap,
    Controls,
    Background,
    useNodesState,
    useEdgesState,
    addEdge,
    Node,
    Edge,
    Position,
    Connection,
    MarkerType,
} from "@xyflow/react";
import dagre from "dagre";
import TopologyNode from "./topology-node";

const nodeTypes = {
    topologyNode: TopologyNode,
};

// --- Layouting Helper ---
const dagreGraph = new dagre.graphlib.Graph();
dagreGraph.setDefaultEdgeLabel(() => ({}));

const nodeWidth = 150;
const nodeHeight = 50;

const getLayoutedElements = (nodes: Node[], edges: Edge[], direction = "LR") => {
    const isHorizontal = direction === "LR";
    dagreGraph.setGraph({ rankdir: direction });

    nodes.forEach((node) => {
        dagreGraph.setNode(node.id, { width: nodeWidth, height: nodeHeight });
    });

    edges.forEach((edge) => {
        dagreGraph.setEdge(edge.source, edge.target);
    });

    dagre.layout(dagreGraph);

    const newNodes = nodes.map((node) => {
        const nodeWithPosition = dagreGraph.node(node.id);
        return {
            ...node,
            targetPosition: isHorizontal ? Position.Left : Position.Top,
            sourcePosition: isHorizontal ? Position.Right : Position.Bottom,
            // We are shifting the dagre node position (anchor=center center) to the top left
            // so it matches the React Flow node anchor point (top left).
            position: {
                x: nodeWithPosition.x - nodeWidth / 2,
                y: nodeWithPosition.y - nodeHeight / 2,
            },
        };
    });

    return { nodes: newNodes, edges };
};

const refinedNodes: Node[] = [
    {
        id: "pkg-1",
        data: { label: "my-package", type: "Package" },
        position: { x: 0, y: 0 },
        type: 'topologyNode',
    },
    {
        id: "cls-1",
        data: { label: "EchoClass", type: "Class" },
        position: { x: 0, y: 0 },
        type: 'topologyNode',
    },
    {
        id: "fn-1",
        data: { label: "echo", type: "Function" },
        position: { x: 0, y: 0 },
        type: 'topologyNode',
    },
    {
        id: "router-1",
        data: { label: "Router-US", type: "Router" },
        position: { x: 0, y: 0 },
        type: 'topologyNode',
    },
    {
        id: "env-1",
        data: { label: "cluster-1", type: "Environment" },
        position: { x: 0, y: 0 },
        type: 'topologyNode',
    },
];

const initialEdges: Edge[] = [
    { id: "e1-2", source: "env-1", target: "router-1", animated: true, markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "e2-3", source: "router-1", target: "pkg-1", markerEnd: { type: MarkerType.ArrowClosed } }, // Router -> Package logical link
    { id: "e3-4", source: "pkg-1", target: "cls-1", markerEnd: { type: MarkerType.ArrowClosed } },
    { id: "e4-5", source: "cls-1", target: "fn-1", markerEnd: { type: MarkerType.ArrowClosed } },
];


interface TopologyGraphProps {
    data: { nodes: Node[], edges: Edge[] };
    onNodeSelect: (node: Node | null) => void;
}

export default function TopologyGraph({ data, onNodeSelect }: TopologyGraphProps) {
    const { nodes: initialNodes, edges: initialEdges } = data;
    const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
    const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

    const onConnect = useCallback(
        (params: Connection) => setEdges((eds) => addEdge(params, eds)),
        [setEdges]
    );

    // Apply Layout whenever data changes
    useEffect(() => {
        const layouted = getLayoutedElements(initialNodes, initialEdges);
        setNodes([...layouted.nodes]);
        setEdges([...layouted.edges]);
    }, [initialNodes, initialEdges, setNodes, setEdges]);

    const onNodeClick = (_: React.MouseEvent, node: Node) => {
        onNodeSelect(node);
    };

    const onPaneClick = () => {
        onNodeSelect(null);
    }

    return (
        <div className="w-full h-full bg-slate-50 dark:bg-slate-900 relative">
            <ReactFlow
                nodes={nodes}
                edges={edges}
                nodeTypes={nodeTypes}
                onNodesChange={onNodesChange}
                onEdgesChange={onEdgesChange}
                onConnect={onConnect}
                onNodeClick={onNodeClick}
                onPaneClick={onPaneClick}
                fitView
                className="bg-background"
            >
                <Controls className="bg-background border-muted fill-foreground" />
                <MiniMap
                    className="bg-background border rounded-md"
                    maskColor="rgba(0, 0, 0, 0.1)"
                    nodeColor={(node) => {
                        // Use matching colors for minimap nodes
                        const type = node.data?.type;
                        switch (type) {
                            case 'Package': return '#6366f1';
                            case 'Class': return '#06b6d4';
                            case 'Function': return '#f59e0b';
                            case 'Environment': return '#ec4899';
                            case 'Router': return '#a855f7';
                            default: return '#9ca3af';
                        }
                    }}
                />
                <Background gap={12} size={1} />
            </ReactFlow>
        </div>
    );
}
