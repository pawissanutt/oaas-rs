"use client";

import React from "react";
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog";

interface RawJsonDialogProps {
    open: boolean;
    onOpenChange: (open: boolean) => void;
    title: string;
    data: unknown;
}

export function RawJsonDialog({
    open,
    onOpenChange,
    title,
    data,
}: RawJsonDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                </DialogHeader>
                <div className="flex-1 min-h-0 overflow-auto">
                    <pre className="text-sm font-mono bg-muted/50 p-4 rounded-md border whitespace-pre-wrap break-all">
                        {JSON.stringify(data, null, 2)}
                    </pre>
                </div>
            </DialogContent>
        </Dialog>
    );
}
