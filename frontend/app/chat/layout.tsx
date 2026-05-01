"use client";

import type { ReactNode } from "react";
import { useState } from "react";
import { ThreadList } from "@/components/chat/thread-list";
import { Sheet, SheetContent } from "@/components/ui/sheet";

export default function ChatLayout({ children }: { children: ReactNode }) {
  const [sheetOpen, setSheetOpen] = useState(false);

  return (
    // lg breakpoint (1024px) — iPad portrait (768px) too narrow for two-column
    <div className="grid h-screen grid-cols-1 bg-background lg:grid-cols-[260px_1fr]">
      {/* Desktop thread list sidebar (≥1024px) */}
      <aside className="hidden h-full border-r border-border/60 bg-sidebar/40 lg:block">
        <ThreadList />
      </aside>

      {/* Mobile sheet overlay (<768px) */}
      <Sheet open={sheetOpen} onOpenChange={setSheetOpen}>
        {/* Trigger button is rendered inside ChatPanel header via data-open-thread-sheet */}
        <SheetContent side="left" className="w-[280px] p-0">
          <ThreadList />
        </SheetContent>
      </Sheet>

      {/* Main content — passes sheet opener down via a custom event */}
      <section
        className="h-full min-w-0"
        data-chat-sheet-open={sheetOpen ? "true" : "false"}
        onClickCapture={(e) => {
          const target = e.target as HTMLElement;
          if (target.closest("[data-open-thread-sheet]")) {
            e.stopPropagation();
            setSheetOpen(true);
          }
        }}
      >
        {children}
      </section>
    </div>
  );
}
