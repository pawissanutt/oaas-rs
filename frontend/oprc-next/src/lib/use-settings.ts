"use client";

import { useState, useEffect, useCallback } from "react";

export interface AppSettings {
    pageSize: number;
    autoRefresh: boolean;
    refreshInterval: number; // seconds
}

const DEFAULTS: AppSettings = {
    pageSize: 25,
    autoRefresh: false,
    refreshInterval: 30,
};

export function useSettings(): AppSettings & { resetDefaults: () => void } {
    const [settings, setSettings] = useState<AppSettings>(DEFAULTS);

    useEffect(() => {
        const ps = localStorage.getItem("oprc-pageSize");
        const ar = localStorage.getItem("oprc-autoRefresh");
        const ri = localStorage.getItem("oprc-refreshInterval");

        setSettings({
            pageSize: ps ? Number(ps) : DEFAULTS.pageSize,
            autoRefresh: ar ? ar === "true" : DEFAULTS.autoRefresh,
            refreshInterval: ri ? Number(ri) : DEFAULTS.refreshInterval,
        });
    }, []);

    const resetDefaults = useCallback(() => {
        localStorage.removeItem("oprc-pageSize");
        localStorage.removeItem("oprc-autoRefresh");
        localStorage.removeItem("oprc-refreshInterval");
        setSettings(DEFAULTS);
    }, []);

    return { ...settings, resetDefaults };
}

/**
 * Hook for auto-refreshing data at a configurable interval.
 * Only active when autoRefresh is enabled in settings.
 */
export function useAutoRefresh(callback: () => void) {
    const { autoRefresh, refreshInterval } = useSettings();

    useEffect(() => {
        if (!autoRefresh) return;
        const id = setInterval(callback, refreshInterval * 1000);
        return () => clearInterval(id);
    }, [autoRefresh, refreshInterval, callback]);
}
