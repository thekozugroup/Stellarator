"use client";

/**
 * Lightweight subset of the shadcn/ui sidebar block (icon-collapsible variant).
 * Built on top of the existing Sheet primitive for mobile.
 * Sidebar state is persisted to localStorage (not a cookie) — issue #10.
 */

import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { PanelLeft } from "lucide-react";
import { cn } from "@/lib/utils";
import { Sheet, SheetContent } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";

/** localStorage key for sidebar expanded/collapsed state — issue #10 */
const SIDEBAR_STORAGE_KEY = "stellarator.sidebar";
const W_EXPANDED = "16rem";
const W_ICON = "3.25rem";

type SidebarState = "expanded" | "collapsed";

interface SidebarContextValue {
  state: SidebarState;
  toggle: () => void;
  setState: (s: SidebarState) => void;
  isMobile: boolean;
  mobileOpen: boolean;
  setMobileOpen: (v: boolean) => void;
}

const SidebarContext = React.createContext<SidebarContextValue | null>(null);

export function useSidebar(): SidebarContextValue {
  const ctx = React.useContext(SidebarContext);
  if (!ctx) throw new Error("useSidebar must be used within <SidebarProvider>");
  return ctx;
}

export function SidebarProvider({
  children,
  defaultCollapsed = false,
}: {
  children: React.ReactNode;
  defaultCollapsed?: boolean;
}): React.ReactElement {
  const [state, setState] = React.useState<SidebarState>(
    defaultCollapsed ? "collapsed" : "expanded",
  );
  const [isMobile, setIsMobile] = React.useState<boolean>(false);
  const [mobileOpen, setMobileOpen] = React.useState<boolean>(false);

  React.useEffect(() => {
    const stored = window.localStorage.getItem(SIDEBAR_STORAGE_KEY);
    if (stored === "collapsed" || stored === "expanded") setState(stored);
    const mq = window.matchMedia("(max-width: 768px)");
    const apply = (): void => setIsMobile(mq.matches);
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, []);

  const toggle = React.useCallback((): void => {
    if (isMobile) {
      setMobileOpen((v) => !v);
      return;
    }
    setState((s) => {
      const next: SidebarState = s === "expanded" ? "collapsed" : "expanded";
      window.localStorage.setItem(SIDEBAR_STORAGE_KEY, next);
      return next;
    });
  }, [isMobile]);

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if ((e.metaKey || e.ctrlKey) && e.key === "b") {
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);

  const value: SidebarContextValue = {
    state,
    toggle,
    setState,
    isMobile,
    mobileOpen,
    setMobileOpen,
  };

  return (
    <SidebarContext.Provider value={value}>
      <div
        style={
          {
            "--sidebar-w": W_EXPANDED,
            "--sidebar-w-icon": W_ICON,
          } as React.CSSProperties
        }
        className="flex min-h-screen w-full bg-background"
      >
        {children}
      </div>
    </SidebarContext.Provider>
  );
}

export function Sidebar({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  const { state, isMobile, mobileOpen, setMobileOpen } = useSidebar();

  if (isMobile) {
    return (
      <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
        <SheetContent side="left" className="w-[var(--sidebar-w)] p-0">
          <div className={cn("flex h-full flex-col bg-sidebar text-sidebar-foreground", className)}>
            {children}
          </div>
        </SheetContent>
      </Sheet>
    );
  }

  return (
    <aside
      data-state={state}
      aria-label="Primary"
      className={cn(
        "group/sidebar sticky top-0 hidden h-screen shrink-0 border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-[width] duration-200 ease-out md:flex md:flex-col",
        state === "expanded" ? "w-[var(--sidebar-w)]" : "w-[var(--sidebar-w-icon)]",
        className,
      )}
    >
      {children}
    </aside>
  );
}

export function SidebarRail(): React.ReactElement {
  const { toggle, state } = useSidebar();
  return (
    <button
      type="button"
      aria-label={state === "expanded" ? "Collapse sidebar" : "Expand sidebar"}
      onClick={toggle}
      className="absolute inset-y-0 right-0 hidden w-1.5 cursor-ew-resize bg-transparent transition-colors hover:bg-sidebar-accent md:block"
    />
  );
}

export function SidebarTrigger({ className }: { className?: string }): React.ReactElement {
  const { toggle } = useSidebar();
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={toggle}
      aria-label="Toggle sidebar"
      className={cn("size-8", className)}
    >
      <PanelLeft className="size-4" />
      <span className="sr-only">Toggle sidebar</span>
    </Button>
  );
}

export function SidebarHeader({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return (
    <div className={cn("relative flex items-center gap-2 px-3 py-3.5", className)}>{children}</div>
  );
}

export function SidebarContent({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return (
    <nav
      aria-label="Sidebar"
      className={cn("flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-2 py-2", className)}
    >
      {children}
    </nav>
  );
}

export function SidebarFooter({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return (
    <div className={cn("border-t border-sidebar-border px-2 py-2", className)}>{children}</div>
  );
}

export function SidebarGroup({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return <div className={cn("flex flex-col gap-1", className)}>{children}</div>;
}

export function SidebarGroupLabel({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  const { state } = useSidebar();
  return (
    <div
      className={cn(
        "px-2 pb-0.5 pt-1 text-[10px] font-medium uppercase tracking-[0.18em] text-muted-foreground/80",
        state === "collapsed" && "md:hidden",
        className,
      )}
    >
      {children}
    </div>
  );
}

export function SidebarMenu({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return <ul className={cn("flex flex-col gap-0.5", className)}>{children}</ul>;
}

export function SidebarMenuItem({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return <li className={cn("relative", className)}>{children}</li>;
}

export interface SidebarMenuButtonProps
  extends Omit<React.ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  asChild?: boolean;
  isActive?: boolean;
  tooltip?: string;
  children: React.ReactNode;
}

export const SidebarMenuButton = React.forwardRef<HTMLElement, SidebarMenuButtonProps>(
  function SidebarMenuButton(
    { asChild = false, isActive = false, tooltip, className, children, ...props },
    ref,
  ): React.ReactElement {
    const { state } = useSidebar();
    const Comp = (asChild ? Slot : "button") as React.ElementType;
    return (
      <Comp
        ref={ref as React.Ref<HTMLElement>}
        data-active={isActive}
        title={state === "collapsed" ? tooltip : undefined}
        className={cn(
          "group/menu-btn flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-sm text-sidebar-foreground/85 outline-none transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-sidebar-ring [&_svg]:size-4 [&_svg]:shrink-0",
          isActive && "bg-sidebar-accent font-medium text-sidebar-foreground",
          state === "collapsed" && "md:justify-center md:px-0",
          className,
        )}
        {...props}
      >
        {children}
      </Comp>
    );
  },
);

export function SidebarMenuLabel({
  children,
}: {
  children: React.ReactNode;
}): React.ReactElement {
  const { state } = useSidebar();
  return (
    <span
      className={cn("min-w-0 flex-1 truncate", state === "collapsed" && "md:hidden")}
    >
      {children}
    </span>
  );
}

export function SidebarInset({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}): React.ReactElement {
  return <main className={cn("min-w-0 flex-1", className)}>{children}</main>;
}
