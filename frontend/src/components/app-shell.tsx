import { Suspense } from "react"
import { NavLink, Outlet, useLocation } from "react-router"
import { useTheme } from "next-themes"
import { Cable, FileText, GitCompareArrows, Monitor, Moon, Play, Settings, Sun } from "lucide-react"
import { Logo } from "@/components/logo"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"

const NAV = [
  { to: "/runs", label: "Runs", short: "Runs", icon: Play },
  { to: "/prompts", label: "Prompt sets", short: "Prompts", icon: FileText },
  { to: "/endpoints", label: "Endpoints", short: "Endpoints", icon: Cable },
  { to: "/preferences", label: "Preferences", short: "Preferences", icon: GitCompareArrows },
]

function NavItems({ onNavigate }: { onNavigate?: () => void }) {
  return (
    <ul className="grid gap-0.5">
      {NAV.map(({ to, label, icon: Icon }) => (
        <li key={to}>
          <NavLink
            to={to}
            onClick={onNavigate}
            className={({ isActive }) =>
              cn(
                "flex h-9 items-center gap-2.5 rounded-md px-2.5 text-sm text-muted-foreground transition-colors duration-150",
                "hover:bg-sidebar-accent hover:text-foreground",
                isActive && "bg-sidebar-accent font-medium text-foreground",
              )
            }
          >
            <Icon className="size-4 shrink-0" strokeWidth={1.75} aria-hidden="true" />
            {label}
          </NavLink>
        </li>
      ))}
    </ul>
  )
}

function ThemeSwitch() {
  const { theme, setTheme } = useTheme()
  return (
    <ToggleGroup
      type="single"
      size="sm"
      variant="outline"
      spacing={0}
      value={theme ?? "system"}
      onValueChange={(v) => v && setTheme(v)}
      aria-label="Theme"
      className="w-full"
    >
      <ToggleGroupItem value="light" aria-label="Light theme" title="Light" className="flex-1">
        <Sun aria-hidden="true" />
      </ToggleGroupItem>
      <ToggleGroupItem value="dark" aria-label="Dark theme" title="Dark" className="flex-1">
        <Moon aria-hidden="true" />
      </ToggleGroupItem>
      <ToggleGroupItem value="system" aria-label="Match system theme" title="System" className="flex-1">
        <Monitor aria-hidden="true" />
      </ToggleGroupItem>
    </ToggleGroup>
  )
}

function Brand() {
  return (
    <NavLink to="/runs" className="flex items-center gap-2 rounded-md px-1 py-1 pointer-coarse:min-h-11 text-[15px] font-semibold tracking-[-0.01em]">
      <Logo className="size-6" />
      Stellarator
    </NavLink>
  )
}

function SidebarFooter({ onNavigate }: { onNavigate?: () => void }) {
  return (
    <div className="grid gap-3">
      <NavLink
        to="/settings"
        onClick={onNavigate}
        className={({ isActive }) =>
          cn(
            "flex h-9 items-center gap-2.5 rounded-md px-2.5 text-sm text-muted-foreground transition-colors duration-150 hover:bg-sidebar-accent hover:text-foreground",
            isActive && "bg-sidebar-accent font-medium text-foreground",
          )
        }
      >
        <Settings className="size-4" strokeWidth={1.75} aria-hidden="true" />
        Settings
      </NavLink>
      <ThemeSwitch />
    </div>
  )
}

export function AppShell() {
  const location = useLocation()
  return (
    <div className="min-h-dvh md:grid md:grid-cols-[224px_minmax(0,1fr)] md:pl-[env(safe-area-inset-left)]">
      <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:top-2 focus:left-2 focus:z-50 focus:rounded-md focus:bg-background focus:px-3 focus:py-2">
        Skip to content
      </a>

      {/* Desktop sidebar */}
      <aside className="sticky top-0 hidden h-dvh flex-col justify-between border-r bg-sidebar px-3 py-4 md:flex">
        <div className="grid gap-6">
          <Brand />
          <nav aria-label="Main">
            <NavItems />
          </nav>
        </div>
        <SidebarFooter />
      </aside>

      {/* Phone and small tablet: a top bar with the product and Settings, and a tab bar for the
          four destinations (HIG: tab bars, not a hidden menu, for top-level navigation). */}
      <header className="sticky top-0 z-30 border-b bg-sidebar pt-[env(safe-area-inset-top)] md:hidden">
        <div className="flex h-12 items-center justify-between px-[max(1rem,env(safe-area-inset-left))]">
          <Brand />
          <NavLink
            to="/settings"
            aria-label="Settings"
            className={({ isActive }) =>
              cn(
                "-mr-2 inline-flex size-11 items-center justify-center rounded-md text-muted-foreground hover:text-foreground",
                isActive && "text-foreground",
              )
            }
          >
            <Settings className="size-5" strokeWidth={1.75} aria-hidden="true" />
          </NavLink>
        </div>
      </header>
      <nav
        aria-label="Main"
        className="fixed inset-x-0 bottom-0 z-30 border-t bg-sidebar pb-[env(safe-area-inset-bottom)] md:hidden"
      >
        <ul className="grid grid-cols-4 px-[env(safe-area-inset-left)]">
          {NAV.map(({ to, label, short, icon: Icon }) => (
            <li key={to}>
              <NavLink
                to={to}
                className={({ isActive }) =>
                  cn(
                    "flex h-14 flex-col items-center justify-center gap-0.5 text-xs text-muted-foreground",
                    isActive && "font-medium text-foreground",
                  )
                }
              >
                {({ isActive }) => (
                  <>
                    <Icon className="size-[22px]" strokeWidth={isActive ? 2.25 : 1.75} aria-hidden="true" />
                    <span>{short}</span>
                    <span className="sr-only">{label !== short ? label : ""}</span>
                  </>
                )}
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>

      <main
        id="main"
        key={location.pathname.split("/")[1]}
        className="min-w-0 pb-[var(--bottom-chrome)] md:pb-0"
      >
        <Suspense fallback={null}>
          <Outlet />
        </Suspense>
      </main>
    </div>
  )
}
