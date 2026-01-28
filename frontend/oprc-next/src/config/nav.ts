import {
    Home,
    Box,
    Rocket,
    Zap,
    Globe,
    Network,
    ClipboardList,
    Settings
} from "lucide-react";

export const NAV_ITEMS = [
    {
        label: "Home",
        href: "/",
        icon: Home,
    },
    {
        label: "Objects",
        href: "/objects",
        icon: Box,
    },
    {
        label: "Deployments",
        href: "/deployments",
        icon: Rocket,
    },
    {
        label: "Functions",
        href: "/functions",
        icon: Zap,
    },
    {
        label: "Envs",
        href: "/environments",
        icon: Globe,
    },
    {
        label: "Topology",
        href: "/topology",
        icon: Network,
    },
    {
        label: "Packages",
        href: "/packages",
        icon: ClipboardList,
    },
] as const;

export const SETTINGS_ITEM = {
    label: "Settings",
    href: "/settings",
    icon: Settings,
} as const;
