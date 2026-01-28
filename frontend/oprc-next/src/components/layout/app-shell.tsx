import { MainNav } from "@/components/layout/main-nav"
import { MobileNav } from "@/components/layout/mobile-nav"

export function AppShell({ children }: { children: React.ReactNode }) {
    return (
        <div className="min-h-screen flex flex-col">
            <MainNav />
            <MobileNav />
            <main className="flex-1 container py-6 px-4 md:px-6 lg:px-8 mx-auto max-w-7xl">
                {children}
            </main>
        </div>
    )
}
