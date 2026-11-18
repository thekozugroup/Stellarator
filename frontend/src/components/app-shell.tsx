import { Suspense, useSyncExternalStore } from "react"
import { NavLink, Outlet, useLocation } from "react-router"
import { Cable, FileText, GitCompareArrows, Play, Settings } from "lucide-react"
import { Logo } from "@/components/logo"
import { overlayTitleBar } from "@/lib/desktop-shell"
import { cn } from "@/lib/utils"

const NAV = [
  { to: "/runs", label: "Runs", short: "Runs", icon: Play },
  { to: "/prompts", label: "Prompt sets", short: "Prompt sets", icon: FileText },
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
    </div>
  )
}

function subscribeToScroll(onChange: () => void) {
  window.addEventListener("scroll", onChange, { passive: true })
  return () => window.removeEventListener("scroll", onChange)
}

/**
 * macOS app only (lib/desktop-shell.ts): the strip across the top of the window,
 * --titlebar-height tall, that moves the window like a title bar (Tauri's data-tauri-drag-region;
 * a double click zooms). Over the sidebar it is clear and the window controls sit on it. Over the
 * content it has the page's ground, so content scrolls under it, with a divider once the page is
 * scrolled, as in Mac apps. Nothing interactive is placed under it.
 */
function TitleBarStrip() {
  const scrolled = useSyncExternalStore(subscribeToScroll, () => window.scrollY > 0, () => false)
  return (
    <>
      <div data-tauri-drag-region aria-hidden="true" className="fixed top-0 left-0 z-30 hidden h-[var(--titlebar-height)] w-[224px] md:block" />
      <div
        data-tauri-drag-region
        aria-hidden="true"
        data-scrolled={scrolled || undefined}
        className="fixed top-0 right-0 left-[224px] z-30 hidden h-[var(--titlebar-height)] border-b border-transparent bg-background md:block data-scrolled:border-border"
      />
    </>
  )
}

export function AppShell() {
  const location = useLocation()
  return (
    <div className="min-h-dvh md:grid md:grid-cols-[224px_minmax(0,1fr)] md:pl-[env(safe-area-inset-left)]">
      <a href="#main" className="sr-only focus:not-sr-only focus:fixed focus:top-[calc(var(--titlebar-height)+0.5rem)] focus:left-2 focus:z-50 focus:rounded-md focus:bg-background focus:px-3 focus:py-2">
        Skip to content
      </a>

      {overlayTitleBar ? <TitleBarStrip /> : null}

      {/* Desktop sidebar. In the macOS app its content starts below the title bar strip. */}
      <aside className="sticky top-0 hidden h-dvh flex-col justify-between border-r bg-sidebar px-3 pt-[max(1rem,var(--titlebar-height))] pb-4 md:flex">
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

      {/* The pages' own top padding is 24 px or more (Page in shared/page.tsx); in the macOS app,
          main adds what the title bar strip needs beyond that. */}
      <main
        id="main"
        key={location.pathname.split("/")[1]}
        className="min-w-0 pb-[var(--bottom-chrome)] md:pt-[max(0px,calc(var(--titlebar-height)-1.5rem))] md:pb-0"
      >
        <Suspense fallback={null}>
          <Outlet />
        </Suspense>
      </main>
    </div>
  )
}
