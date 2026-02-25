"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { NAV_ITEMS, SETTINGS_ITEM } from "@/config/nav"
import { cn } from "@/lib/utils"

export function MainNav() {
    const pathname = usePathname()

    return (
        <div className="hidden md:block border-b border-border bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
            <div className="flex h-16 items-center px-4 md:px-6">
                <div className="mr-8 font-bold text-xl">OaaS Console</div>
                <nav className="flex items-center space-x-4 lg:space-x-6 mx-6 flex-1">
                    {NAV_ITEMS.map((item) => {
                        const isActive = pathname === item.href
                        return (
                            <Link
                                key={item.href}
                                href={item.href}
                                className={cn(
                                    "flex items-center text-sm font-medium transition-colors hover:text-primary",
                                    isActive
                                        ? "text-foreground"
                                        : "text-muted-foreground"
                                )}
                            >
                                <item.icon className="mr-2 h-4 w-4" />
                                {item.label}
                            </Link>
                        )
                    })}
                </nav>
                <div className="ml-auto flex items-center space-x-4">
                    <Link
                        href={SETTINGS_ITEM.href}
                        className={cn(
                            "p-2 rounded-md transition-colors hover:bg-muted hover:text-foreground",
                            pathname === SETTINGS_ITEM.href ? "text-foreground" : "text-muted-foreground"
                        )}
                    >
                        <SETTINGS_ITEM.icon className="h-5 w-5" />
                        <span className="sr-only">Settings</span>
                    </Link>
                </div>
            </div>
        </div>
    )
}
