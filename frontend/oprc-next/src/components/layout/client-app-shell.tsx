"use client";

import dynamic from "next/dynamic";
import React from "react";

const AppShell = dynamic(() => import("./app-shell").then(mod => mod.AppShell), {
    ssr: false,
    loading: () => <div className="min-h-screen bg-background" />
});

export function ClientAppShell({ children }: { children: React.ReactNode }) {
    return <AppShell>{children}</AppShell>;
}
