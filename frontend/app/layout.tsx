import type { Metadata } from "next";
import "./globals.css";
import { Providers } from "@/components/providers";
import { AppShell } from "@/components/app-shell";
import { CommandPalette } from "@/components/command-palette";
import { NotificationListener } from "@/components/run-completion-toast";
import { Toaster } from "@/components/ui/sonner";

export const metadata: Metadata = {
  title: "Stellarator",
  description: "Live observability for autonomous training runs.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark" suppressHydrationWarning>
      <body className="antialiased">
        <Providers>
          <AppShell>{children}</AppShell>
          <CommandPalette />
          <NotificationListener />
          <Toaster />
        </Providers>
      </body>
    </html>
  );
}
