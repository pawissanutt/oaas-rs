import type { Metadata } from "next";
import { ThemeProvider } from "@/components/theme-provider";
import { ClientAppShell } from "@/components/layout/client-app-shell";
import { Toaster } from "sonner";
import "./globals.css";

export const metadata: Metadata = {
  title: "OaaS-RS Console",
  description: "Management Console for OaaS-RS",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body
        className="antialiased bg-background text-foreground"
      >
        <ThemeProvider
          attribute="class"
          defaultTheme="system"
          enableSystem
          disableTransitionOnChange
        >
          <ClientAppShell>{children}</ClientAppShell>
          <Toaster richColors closeButton position="bottom-right" />
        </ThemeProvider>
      </body>
    </html>
  );
}
