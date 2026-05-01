"use client";

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { ConnectionIndicator } from "@/components/connection-indicator";
import {
  Activity,
  AlertTriangle,
  Archive,
  BookText,
  GaugeCircle,
  History,
  LogOut,
  MessageSquare,
  Pin,
  Settings,
  Sparkles,
  Workflow,
} from "lucide-react";
import { useEffect, useState } from "react";
import { getAlertsLastSeen } from "@/lib/notifications";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuLabel,
  SidebarProvider,
  SidebarRail,
  SidebarTrigger,
  useSidebar,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";
import { api } from "@/lib/api";
import type { Run } from "@/lib/types";
import { usePrefs } from "@/lib/local-prefs";

interface NavLink {
  href: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

const WORKSPACE: NavLink[] = [
  { href: "/", label: "Dashboard", icon: GaugeCircle },
  { href: "/chat", label: "Chat", icon: MessageSquare },
  { href: "/runs/compare", label: "Compare", icon: Workflow },
];

const AUDIT: NavLink[] = [
  { href: "/research", label: "Agent research", icon: BookText },
  { href: "/alerts", label: "Alerts", icon: AlertTriangle },
];

const FOOTER: NavLink[] = [
  { href: "/settings", label: "Settings", icon: Settings },
];

export function AppShell({ children }: { children: React.ReactNode }) {
  return (
    <SidebarProvider>
      <Sidebar>
        <SidebarBody />
        <SidebarRail />
      </Sidebar>
      <SidebarInset>
        <TopBar />
        {children}
      </SidebarInset>
    </SidebarProvider>
  );
}

function SidebarBody(): React.ReactElement {
  const pathname = usePathname();
  const router = useRouter();
  const queryClient = useQueryClient();
  const { state } = useSidebar();
  const { prefs } = usePrefs();

  const runsQuery = useQuery({
    queryKey: ["runs"],
    queryFn: () => api.listRuns({ limit: 50 }),
    refetchInterval: 10_000,
  });
  const runs: Run[] = runsQuery.data?.runs ?? [];
  const active = runs.filter((r) =>
    ["queued", "provisioning", "running", "paused"].includes(r.status),
  ).length;
  const recent = runs.slice(0, 10);
  const pinnedCount = prefs.pinnedRuns.length;
  const archived = runs.filter((r) =>
    ["succeeded", "failed", "cancelled"].includes(r.status),
  ).length;

  const isActive = (href: string): boolean =>
    href === "/" ? pathname === "/" : pathname.startsWith(href);

  return (
    <>
      <SidebarHeader>
        <Link href="/" className="flex min-w-0 items-center gap-2">
          <div className="grid size-8 shrink-0 place-items-center rounded-lg bg-primary text-primary-foreground">
            {/* Sparkles is the single brand anchor — issue #9 */}
            <Sparkles className="size-4" />
          </div>
          {state === "expanded" ? (
            <div className="min-w-0 leading-tight">
              <div className="truncate text-sm font-semibold tracking-tight">Stellarator</div>
              <div className="truncate text-micro uppercase tracking-[0.2em] text-muted-foreground">
                Training Control
              </div>
            </div>
          ) : null}
        </Link>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Workspace</SidebarGroupLabel>
          <SidebarMenu>
            {WORKSPACE.map((n) => (
              <SidebarMenuItem key={n.href}>
                <SidebarMenuButton asChild isActive={isActive(n.href)} tooltip={n.label}>
                  <Link href={n.href} aria-current={isActive(n.href) ? "page" : undefined}>
                    <n.icon />
                    <SidebarMenuLabel>{n.label}</SidebarMenuLabel>
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        </SidebarGroup>

        <AuditGroup isActive={isActive} />

        <SidebarGroup>
          <SidebarGroupLabel>Runs</SidebarGroupLabel>
          <SidebarMenu>
            <RunsLink
              href="/?status=running"
              label="Active"
              count={active}
              icon={Activity}
              live={active > 0}
            />
            <RunsLink href="/?view=pinned" label="Pinned" count={pinnedCount} icon={Pin} />
            <RunsLink href="/?view=recent" label="Recent" count={recent.length} icon={History} />
            <RunsLink
              href="/?status=succeeded,failed,cancelled"
              label="Archived"
              count={archived}
              icon={Archive}
            />
          </SidebarMenu>
        </SidebarGroup>
      </SidebarContent>

      <SidebarFooter>
        <SidebarMenu>
          {FOOTER.map((n) => (
            <SidebarMenuItem key={n.href}>
              <SidebarMenuButton asChild isActive={isActive(n.href)} tooltip={n.label}>
                <Link href={n.href}>
                  <n.icon />
                  <SidebarMenuLabel>{n.label}</SidebarMenuLabel>
                </Link>
              </SidebarMenuButton>
            </SidebarMenuItem>
          ))}
          <SidebarMenuItem>
            <SidebarMenuButton
              tooltip="Sign out"
              type="button"
              onClick={() => {
                // Clear only auth/session keys — preserve UI state (sidebar, prefs) across sign-outs
                // issue #10: do NOT wipe stellarator.sidebar (SIDEBAR_STORAGE_KEY)
                const AUTH_KEYS = ["stellarator.viewerToken"];
                // Also clear codex session from sessionStorage
                if (typeof sessionStorage !== "undefined") {
                  sessionStorage.removeItem("stellarator.codex.user");
                }
                AUTH_KEYS.forEach((k) => localStorage.removeItem(k));
                // Invalidate whoami so identity resets
                queryClient.invalidateQueries({ queryKey: ["whoami"] });
                toast.success("Signed out");
                router.push("/");
              }}
            >
              <LogOut />
              <SidebarMenuLabel>Sign out</SidebarMenuLabel>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
        {state === "expanded" ? (
          <div className="mt-2 px-2 pb-1 pt-1.5">
            <ConnectionIndicator />
          </div>
        ) : null}
      </SidebarFooter>
    </>
  );
}

function AuditGroup({ isActive }: { isActive: (href: string) => boolean }): React.ReactElement {
  const [open, setOpen] = useState(true);
  const [unread, setUnread] = useState(0);
  const { state } = useSidebar();

  useEffect(() => {
    if (typeof window === "undefined") return;
    function recompute() {
      // Naive: count alert events received via SSE since last-seen ts.
      // We listen for our custom event and tick a counter; the page that
      // displays alerts calls markAlertsSeen() to reset.
      const lastSeen = getAlertsLastSeen();
      setUnread((n) => (lastSeen ? 0 : n));
    }
    recompute();
    function onNotif(e: Event) {
      const detail = (e as CustomEvent<{ type?: string }>).detail;
      if (detail?.type === "alert_error") {
        setUnread((n) => n + 1);
      }
    }
    function onSeen() {
      setUnread(0);
    }
    window.addEventListener("stellarator:notification", onNotif);
    window.addEventListener("stellarator:alerts-seen", onSeen);
    return () => {
      window.removeEventListener("stellarator:notification", onNotif);
      window.removeEventListener("stellarator:alerts-seen", onSeen);
    };
  }, []);

  return (
    <SidebarGroup>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between px-2 py-1 text-[11px] uppercase tracking-widest text-muted-foreground hover:text-foreground transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring rounded"
        aria-expanded={open}
      >
        <span>Audit</span>
        <span className="text-muted-foreground/60">{open ? "−" : "+"}</span>
      </button>
      {open ? (
        <SidebarMenu>
          {AUDIT.map((n) => {
            const showBadge = n.href === "/alerts" && unread > 0;
            return (
              <SidebarMenuItem key={n.href}>
                <SidebarMenuButton asChild isActive={isActive(n.href)} tooltip={n.label}>
                  <Link href={n.href} aria-current={isActive(n.href) ? "page" : undefined}>
                    <n.icon />
                    <SidebarMenuLabel>{n.label}</SidebarMenuLabel>
                    {showBadge && state === "expanded" ? (
                      <span className="ml-auto rounded-md bg-destructive/15 px-1.5 py-0.5 font-mono text-micro tabular-nums text-destructive">
                        {unread}
                      </span>
                    ) : null}
                  </Link>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
        </SidebarMenu>
      ) : null}
    </SidebarGroup>
  );
}

function RunsLink({
  href,
  label,
  count,
  icon: Icon,
  live,
}: {
  href: string;
  label: string;
  count: number;
  icon: React.ComponentType<{ className?: string }>;
  live?: boolean;
}): React.ReactElement {
  const { state } = useSidebar();
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild tooltip={`${label} (${count})`}>
        <Link href={href}>
          <Icon />
          <SidebarMenuLabel>{label}</SidebarMenuLabel>
          {state === "expanded" ? (
            <span
              className={cn(
                "ml-auto rounded-md px-1.5 py-0.5 font-mono text-micro tabular-nums",
                live
                  ? "bg-success/15 text-success"
                  : "bg-muted text-muted-foreground",
              )}
            >
              {count}
            </span>
          ) : null}
        </Link>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

function TopBar(): React.ReactElement {
  return (
    <div className="sticky top-0 z-30 flex h-12 items-center gap-2 border-b border-border/50 bg-background/85 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <SidebarTrigger />
    </div>
  );
}
