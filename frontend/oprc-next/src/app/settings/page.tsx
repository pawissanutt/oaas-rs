"use client";

import { useState, useEffect } from "react";
import { useTheme } from "next-themes";
import {
    Palette,
    Monitor,
    Settings as SettingsIcon,
    Database,
    Link as LinkIcon
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";

export default function SettingsPage() {
    const { theme, setTheme } = useTheme();
    // We need to mount first to avoid hydration mismatch for theme
    const [mounted, setMounted] = useState(false);
    const [pageSize, setPageSize] = useState("25");
    const [autoRefresh, setAutoRefresh] = useState(false);
    const [refreshInterval, setRefreshInterval] = useState("30");
    const [showSaved, setShowSaved] = useState(false);

    useEffect(() => {
        // eslint-disable-next-line
        setMounted(true);
        // Load settings from local storage
        const savedPageSize = localStorage.getItem("oprc-pageSize");
        const savedAutoRefresh = localStorage.getItem("oprc-autoRefresh");
        const savedRefreshInterval = localStorage.getItem("oprc-refreshInterval");

        if (savedPageSize) setPageSize(savedPageSize);
        if (savedAutoRefresh) setAutoRefresh(savedAutoRefresh === "true");
        if (savedRefreshInterval) setRefreshInterval(savedRefreshInterval);
    }, []);

    const saveSettings = () => {
        localStorage.setItem("oprc-pageSize", pageSize);
        localStorage.setItem("oprc-autoRefresh", String(autoRefresh));
        localStorage.setItem("oprc-refreshInterval", refreshInterval);

        setShowSaved(true);
        setTimeout(() => setShowSaved(false), 2000);
    };

    if (!mounted) return null;

    return (
        <div className="space-y-6 max-w-4xl mx-auto">
            <div className="flex items-center justify-between">
                <h1 className="text-3xl font-bold tracking-tight">Settings</h1>
                {showSaved && (
                    <div className="bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 px-4 py-2 rounded-md font-medium text-sm animate-in fade-in slide-in-from-top-2">
                        Settings saved!
                    </div>
                )}
            </div>

            <div className="grid gap-6">
                {/* Appearance */}
                <Card>
                    <CardHeader className="flex flex-row items-center gap-4 py-4">
                        <div className="p-2 bg-primary/10 rounded-full">
                            <Palette className="h-6 w-6 text-primary" />
                        </div>
                        <div>
                            <CardTitle className="text-lg">Appearance</CardTitle>
                            <p className="text-sm text-muted-foreground">Customize the look and feel of the console</p>
                        </div>
                    </CardHeader>
                    <CardContent className="space-y-4">
                        <div className="grid sm:grid-cols-2 gap-4">
                            <div className="space-y-2">
                                <Label>Theme</Label>
                                <Select value={theme} onValueChange={setTheme}>
                                    <SelectTrigger>
                                        <SelectValue placeholder="Select theme" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="light">Light</SelectItem>
                                        <SelectItem value="dark">Dark</SelectItem>
                                        <SelectItem value="system">System</SelectItem>
                                    </SelectContent>
                                </Select>
                            </div>
                        </div>
                    </CardContent>
                </Card>

                {/* Data Display */}
                <Card>
                    <CardHeader className="flex flex-row items-center gap-4 py-4">
                        <div className="p-2 bg-primary/10 rounded-full">
                            <Monitor className="h-6 w-6 text-primary" />
                        </div>
                        <div>
                            <CardTitle className="text-lg">Data Display</CardTitle>
                            <p className="text-sm text-muted-foreground">Manage how data is presented</p>
                        </div>
                    </CardHeader>
                    <CardContent className="space-y-6">
                        <div className="flex items-center justify-between">
                            <div className="space-y-0.5">
                                <Label className="text-base">Auto-refresh data</Label>
                                <p className="text-sm text-muted-foreground">
                                    Automatically update dashboard and lists
                                </p>
                            </div>
                            <Switch
                                checked={autoRefresh}
                                onCheckedChange={setAutoRefresh}
                            />
                        </div>

                        {autoRefresh && (
                            <div className="grid sm:grid-cols-2 gap-4 animate-in fade-in slide-in-from-top-2">
                                <div className="space-y-2">
                                    <Label>Refresh Interval (seconds)</Label>
                                    <Input
                                        type="number"
                                        value={refreshInterval}
                                        onChange={(e) => setRefreshInterval(e.target.value)}
                                        min="5"
                                        max="300"
                                    />
                                </div>
                            </div>
                        )}

                        <div className="grid sm:grid-cols-2 gap-4 border-t pt-4">
                            <div className="space-y-2">
                                <Label>Default Page Size</Label>
                                <Select value={pageSize} onValueChange={setPageSize}>
                                    <SelectTrigger>
                                        <SelectValue placeholder="Select size" />
                                    </SelectTrigger>
                                    <SelectContent>
                                        <SelectItem value="10">10 rows</SelectItem>
                                        <SelectItem value="25">25 rows</SelectItem>
                                        <SelectItem value="50">50 rows</SelectItem>
                                        <SelectItem value="100">100 rows</SelectItem>
                                    </SelectContent>
                                </Select>
                            </div>
                        </div>
                    </CardContent>
                </Card>

                {/* Connection Info */}
                <Card>
                    <CardHeader className="flex flex-row items-center gap-4 py-4">
                        <div className="p-2 bg-primary/10 rounded-full">
                            <LinkIcon className="h-6 w-6 text-primary" />
                        </div>
                        <div>
                            <CardTitle className="text-lg">Connection</CardTitle>
                            <p className="text-sm text-muted-foreground">API connection details</p>
                        </div>
                    </CardHeader>
                    <CardContent>
                        <div className="space-y-2">
                            <Label>API Base URL</Label>
                            <div className="flex gap-2">
                                <Input value="/api/v1 (Same Origin)" disabled readOnly className="bg-muted" />
                            </div>
                            <p className="text-xs text-muted-foreground">
                                The frontend connects directly to the Package Manager API hosted on the same origin.
                            </p>
                        </div>
                    </CardContent>
                </Card>

                <div className="flex justify-end gap-4">
                    <Button variant="outline" onClick={() => {
                        localStorage.removeItem("oprc-pageSize");
                        localStorage.removeItem("oprc-autoRefresh");
                        localStorage.removeItem("oprc-refreshInterval");
                        setPageSize("25");
                        setAutoRefresh(false);
                        setRefreshInterval("30");
                        setTheme("system");
                        setShowSaved(true);
                        setTimeout(() => setShowSaved(false), 2000);
                    }}>Reset to Defaults</Button>
                    <Button onClick={saveSettings}>Save Preferences</Button>
                </div>
            </div>
        </div>
    );
}
