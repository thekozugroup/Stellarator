import { Suspense } from "react"
import { NavLink, Outlet, useLocation } from "react-router"
import { Cable, FileText, GitCompareArrows, Play, Settings, type LucideIcon } from "lucide-react"
import { Logo } from "@/components/logo"
import { overlayTitleBar } from "@/lib/desktop-shell"
import { cn } from "@/lib/utils"

const NAV = [
  { to: "/runs", label: "Runs", icon: Play },
  { to: "/prompts", label: "Prompt sets", icon: FileText },
  { to: "/endpoints", label: "Endpoints", icon: Cable },
  { to: "/preferences", label: "Preferences", icon: GitCompareArrows },
]

/**
 * A macOS sidebar row: icon and label, 28 px on pointer-fine (44 pt on touch), a neutral rounded
 * selection. Labels are ink; icons are gray until their row is selected.
 */
function SidebarRow({ to, label, icon: Icon }: { to: string; label: string; icon: LucideIcon }) {
  return (
    <NavLink
      to={to}
      className={({ isActive }) =>
        cn(
          "group/row flex h-8 items-center gap-2 rounded-[10px] px-2 text-sm text-foreground transition-colors duration-150 pointer-fine:h-7",
          "hover:bg-foreground/5",
          isActive && "bg-sidebar-accent hover:bg-sidebar-accent",
        )
      }
    >
      <Icon
        className="size-4 shrink-0 text-muted-foreground group-aria-[current=page]/row:text-foreground"
        strokeWidth={1.75}
        aria-hidden="true"
      />
      {label}
    </NavLink>
  )
}

/**
 * Pointer-fine and tablets: the sidebar, a panel floating 8 px inside the window with 18 px corners
 * (concentric with the macOS 26 window) in the bar material. The mark and name at the top,
 * centered on the toolbar's band; Settings at the bottom. In the macOS app the traffic lights take
 * that band and the mark moves below it.
 */
function Sidebar() {
  return (
    <aside className="sticky top-0 hidden h-dvh p-2 md:block">
      <div className="material-bar material-float flex h-full flex-col justify-between rounded-3xl px-2 pt-[calc(max(0px,var(--titlebar-height)-0.5rem)+0.25rem)] pb-2 [--material-tint:var(--sidebar)]">
        <div className="grid gap-4">
          <NavLink
            to="/runs"
            className="flex h-8 items-center gap-2 self-start justify-self-start rounded-[10px] px-2 text-sm font-semibold tracking-[-0.01em] pointer-fine:h-7"
          >
            <Logo className="size-5" />
            Stellarator
          </NavLink>
          <nav aria-label="Main">
            <ul className="grid gap-0.5">
              {NAV.map((item) => (
                <li key={item.to}>
                  <SidebarRow {...item} />
                </li>
              ))}
            </ul>
          </nav>
        </div>
        <SidebarRow to="/settings" label="Settings" icon={Settings} />
      </div>
    </aside>
  )
}

/**
 * Phones: the four destinations in a capsule floating above the content (iOS 26), in the bar
 * material, 12 px from the sides. Settings sits in each top-level screen's bar (Page).
 */
function TabBar() {
  return (
    <nav
      aria-label="Main"
      className="material-bar material-float fixed right-[max(0.75rem,env(safe-area-inset-right))] bottom-(--tab-bar-bottom) left-[max(0.75rem,env(safe-area-inset-left))] z-30 h-(--tab-bar-height) rounded-full p-1 md:hidden"
    >
      <ul className="grid h-full grid-cols-4">
        {NAV.map(({ to, label, icon: Icon }) => (
          <li key={to} className="min-w-0">
            <NavLink
              to={to}
              className={({ isActive }) =>
                cn(
                  "flex h-full flex-col items-center justify-center gap-0.5 rounded-full text-xs text-muted-foreground",
                  isActive && "bg-foreground/8 font-medium text-foreground",
                )
              }
            >
              {({ isActive }) => (
                <>
                  <Icon className="size-[22px]" strokeWidth={isActive ? 2.25 : 1.75} aria-hidden="true" />
                  <span className="max-w-full truncate">{label}</span>
                </>
              )}
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  )
}

export function AppShell() {
  const location = useLocation()
  return (
    <div className="min-h-dvh md:grid md:grid-cols-[var(--sidebar-column)_minmax(0,1fr)] md:pl-[env(safe-area-inset-left)]">
      <a
        href="#main"
        className="sr-only focus:not-sr-only focus:fixed focus:top-[calc(var(--titlebar-height)+0.5rem)] focus:left-2 focus:z-50 focus:rounded-md focus:bg-background focus:px-3 focus:py-2"
      >
        Skip to content
      </a>

      {/* macOS app only (lib/desktop-shell.ts): the band over the sidebar that moves the window like
          a title bar, with the traffic lights on it. Over the content, the page toolbar does the
          same. Nothing interactive sits in the band. */}
      {overlayTitleBar ? (
        <div
          data-tauri-drag-region
          aria-hidden="true"
          className="fixed top-0 left-0 z-30 hidden h-(--titlebar-height) w-(--sidebar-column) md:block"
        />
      ) : null}

      <Sidebar />
      <TabBar />

      <main id="main" key={location.pathname.split("/")[1]} className="min-w-0 pb-(--bottom-chrome)">
        <Suspense fallback={null}>
          <Outlet />
        </Suspense>
      </main>
    </div>
  )
}
