import { useEffect, useSyncExternalStore, type ReactNode } from "react"

/**
 * Light, dark or follow the system. The class on <html> is set before the first paint by the
 * inline script in index.html (same storage key), so there is no flash of the wrong theme.
 */
export type Theme = "light" | "dark" | "system"

const KEY = "theme"
const listeners = new Set<() => void>()
const media = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null

function readStored(): Theme {
  try {
    const v = window.localStorage.getItem(KEY)
    return v === "light" || v === "dark" ? v : "system"
  } catch {
    return "system"
  }
}

let current: Theme = typeof window === "undefined" ? "system" : readStored()

function resolved(theme: Theme): "light" | "dark" {
  if (theme === "system") return media?.matches ? "dark" : "light"
  return theme
}

function apply(theme: Theme) {
  const mode = resolved(theme)
  const root = document.documentElement
  // Switch without animating every color transition on the page.
  const freeze = document.createElement("style")
  freeze.textContent = "*,*::before,*::after{transition:none!important}"
  document.head.appendChild(freeze)
  root.classList.toggle("dark", mode === "dark")
  root.style.colorScheme = mode
  void root.offsetHeight
  requestAnimationFrame(() => freeze.remove())
}

function emit() {
  listeners.forEach((l) => l())
}

export function setTheme(theme: Theme) {
  current = theme
  try {
    if (theme === "system") window.localStorage.removeItem(KEY)
    else window.localStorage.setItem(KEY, theme)
  } catch {
    /* storage blocked: the choice lasts for this page */
  }
  apply(theme)
  emit()
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function useTheme(): { theme: Theme; resolvedTheme: "light" | "dark"; setTheme: (t: Theme) => void } {
  const theme = useSyncExternalStore(subscribe, () => current, () => "system" as Theme)
  return { theme, resolvedTheme: resolved(theme), setTheme }
}

/** Follows the operating system's setting while the theme is "system". */
export function ThemeProvider({ children }: { children: ReactNode }) {
  useEffect(() => {
    if (!media) return
    const onChange = () => {
      if (current === "system") {
        apply("system")
        emit()
      }
    }
    media.addEventListener("change", onChange)
    return () => media.removeEventListener("change", onChange)
  }, [])
  return children
}
