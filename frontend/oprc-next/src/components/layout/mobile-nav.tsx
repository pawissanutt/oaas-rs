"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { Menu, X } from "lucide-react"
import { NAV_ITEMS, SETTINGS_ITEM } from "@/config/nav"
import { cn } from "@/lib/utils"

export function MobileNav() {
    const [open, setOpen] = React.useState(false)
    const pathname = usePathname()

    // Close sidebar on route change
    React.useEffect(() => {
        setOpen(false)
    }, [pathname])

    return (
        <div className="md:hidden sticky top-0 z-50 bg-background border-b border-border">
            <div className="flex h-16 items-center justify-between px-4">
                <button
                    onClick={() => setOpen(true)}
                    className="p-2 -ml-2 hover:bg-muted rounded-md"
                    aria-label="Open menu"
                >
                    <Menu className="h-6 w-6" />
                </button>
                <div className="font-semibold text-lg">OaaS Console</div>
                <Link href={SETTINGS_ITEM.href} className="p-2 -mr-2 hover:bg-muted rounded-md">
                    <SETTINGS_ITEM.icon className="h-6 w-6" />
                </Link>
            </div>

            {/* Overlay */}
            {open && (
                <div
                    className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50 animate-in fade-in-0"
                    onClick={() => setOpen(false)}
                />
            )}

            {/* Sidebar Drawer */}
            <div
                className={cn(
                    "fixed inset-y-0 left-0 z-50 w-64 bg-background border-r border-border shadow-lg transform transition-transform duration-300 ease-in-out",
                    open ? "translate-x-0" : "-translate-x-full"
                )}
            >
                <div className="flex flex-col h-full">
                    <div className="h-16 flex items-center justify-between px-4 border-b border-border">
                        <span className="font-semibold text-lg">Menu</span>
                        <button
                            onClick={() => setOpen(false)}
                            className="p-2 -mr-2 hover:bg-muted rounded-md"
                        >
                            <X className="h-5 w-5" />
                        </button>
                    </div>
                    <div className="flex-1 overflow-auto py-4">
                        <nav className="flex flex-col gap-1 px-2">
                            {NAV_ITEMS.map((item) => {
                                const isActive = pathname === item.href
                                return (
                                    <Link
                                        key={item.href}
                                        href={item.href}
                                        className={cn(
                                            "flex items-center gap-3 px-3 py-2 text-sm font-medium rounded-md transition-colors",
                                            isActive
                                                ? "bg-primary text-primary-foreground"
                                                : "hover:bg-muted text-muted-foreground hover:text-foreground"
                                        )}
                                    >
                                        <item.icon className="h-4 w-4" />
                                        {item.label}
                                    </Link>
                                )
                            })}
                        </nav>
                    </div>
                    <div className="border-t border-border p-4">
                        <Link
                            href={SETTINGS_ITEM.href}
                            className={cn(
                                "flex items-center gap-3 px-3 py-2 text-sm font-medium rounded-md transition-colors",
                                pathname === SETTINGS_ITEM.href
                                    ? "bg-primary text-primary-foreground"
                                    : "hover:bg-muted text-muted-foreground hover:text-foreground"
                            )}
                        >
                            <SETTINGS_ITEM.icon className="h-4 w-4" />
                            {SETTINGS_ITEM.label}
                        </Link>
                    </div>
                </div>
            </div>
        </div>
    )
}
