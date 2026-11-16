import { useState } from "react"
import { NavLink, Outlet, useLocation } from "react-router"
import { useTheme } from "next-themes"
import { Cable, FileText, GitCompareArrows, Menu, Monitor, Moon, Play, Settings, Sun } from "lucide-react"
import { Logo } from "@/components/logo"
import { Button } from "@/components/ui/button"
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetTrigger } from "@/components/ui/sheet"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"

const NAV = [
  { to: "/runs", label: "Runs", icon: Play },
  { to: "/prompts", label: "Prompt sets", icon: FileText },
  { to: "/endpoints", label: "Endpoints", icon: Cable },
  { to: "/preferences", label: "Preferences", icon: GitCompareArrows },
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
    <NavLink to="/runs" className="flex items-center gap-2 rounded-md px-1 py-1 text-[15px] font-semibold tracking-[-0.01em]">
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
  const [open, setOpen] = useState(false)
  const location = useLocation()
  return (
    <div className="min-h-dvh md:grid md:grid-cols-[224px_minmax(0,1fr)]">
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

      {/* Phone and small tablet top bar */}
      <div className="sticky top-0 z-30 flex h-14 items-center justify-between border-b bg-sidebar px-4 md:hidden">
        <Brand />
        <Sheet open={open} onOpenChange={setOpen}>
          <SheetTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="Open menu">
              <Menu aria-hidden="true" />
            </Button>
          </SheetTrigger>
          <SheetContent side="left" className="flex w-72 flex-col justify-between bg-sidebar p-4">
            <div className="grid gap-6">
              <SheetHeader className="p-0">
                <SheetTitle asChild>
                  <div className="flex items-center gap-2 text-[15px] font-semibold">
                    <Logo className="size-6" />
                    Stellarator
                  </div>
                </SheetTitle>
              </SheetHeader>
              <nav aria-label="Main">
                <NavItems onNavigate={() => setOpen(false)} />
              </nav>
            </div>
            <SidebarFooter onNavigate={() => setOpen(false)} />
          </SheetContent>
        </Sheet>
      </div>

      <main id="main" key={location.pathname.split("/")[1]} className="min-w-0">
        <Outlet />
      </main>
    </div>
  )
}
