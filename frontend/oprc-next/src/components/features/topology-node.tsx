import React, { memo } from 'react';
import { Handle, Position, NodeProps } from '@xyflow/react';
import { Package, Box, Zap, Server, Network, Globe, Database } from 'lucide-react';
import { cn } from '@/lib/utils';

import { LucideIcon } from 'lucide-react';

const iconMap: Record<string, LucideIcon> = {
    Package,
    Class: Box,
    Function: Zap,
    Environment: Server,
    Router: Network,
    Gateway: Globe,
    ODGM: Database,
};

const TopologyNode = ({ data, selected }: NodeProps) => {
    const Icon = iconMap[data.type as string] || Box;

    return (
        <div className={cn(
            "px-4 py-2 shadow-sm rounded-md border min-w-[150px] transition-all duration-200",
            "flex items-center gap-3 bg-background",
            selected ? "ring-2 ring-primary border-primary" : "border-border",
            // Apply type-specific subtle backgrounds/borders if not selected
            !selected && data.type === 'Package' && "bg-indigo-50 border-indigo-200 dark:bg-indigo-950/30 dark:border-indigo-800",
            !selected && data.type === 'Class' && "bg-cyan-50 border-cyan-200 dark:bg-cyan-950/30 dark:border-cyan-800 rounded-full",
            !selected && data.type === 'Function' && "bg-amber-50 border-amber-200 dark:bg-amber-950/30 dark:border-amber-800",
            !selected && data.type === 'Environment' && "bg-pink-50 border-pink-200 dark:bg-pink-950/30 dark:border-pink-800",
            !selected && data.type === 'Router' && "bg-purple-50 border-purple-200 dark:bg-purple-950/30 dark:border-purple-800",
            !selected && data.type === 'Gateway' && "bg-blue-50 border-blue-200 dark:bg-blue-950/30 dark:border-blue-800",
            !selected && data.type === 'ODGM' && "bg-emerald-50 border-emerald-200 dark:bg-emerald-950/30 dark:border-emerald-800 rounded-full"
        )}>
            <Handle type="target" position={Position.Left} className="!bg-muted-foreground !w-2 !h-2" />

            <div className={cn(
                "p-1.5 rounded-md flex items-center justify-center shrink-0",
                data.type === 'Package' && "bg-indigo-100 text-indigo-600 dark:bg-indigo-900 dark:text-indigo-300",
                data.type === 'Class' && "bg-cyan-100 text-cyan-600 dark:bg-cyan-900 dark:text-cyan-300 rounded-full",
                data.type === 'Function' && "bg-amber-100 text-amber-600 dark:bg-amber-900 dark:text-amber-300",
                data.type === 'Environment' && "bg-pink-100 text-pink-600 dark:bg-pink-900 dark:text-pink-300",
                data.type === 'Router' && "bg-purple-100 text-purple-600 dark:bg-purple-900 dark:text-purple-300",
                data.type === 'Gateway' && "bg-blue-100 text-blue-600 dark:bg-blue-900 dark:text-blue-300",
                data.type === 'ODGM' && "bg-emerald-100 text-emerald-600 dark:bg-emerald-900 dark:text-emerald-300 rounded-full"
            )}>
                <Icon className="w-4 h-4" />
            </div>

            <div className="flex flex-col">
                <span className="text-sm font-medium leading-none">{data.label as string}</span>
                <span className="text-[10px] text-muted-foreground uppercase tracking-wider mt-0.5">{data.type as string}</span>
            </div>

            <Handle type="source" position={Position.Right} className="!bg-muted-foreground !w-2 !h-2" />
        </div>
    );
};

export default memo(TopologyNode);
