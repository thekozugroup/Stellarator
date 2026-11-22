import { useSyncExternalStore } from "react"

/**
 * The desktop app's window. The app sets `window.__STELLARATOR_DESKTOP__` before this code runs
 * (desktop/src-tauri/src/titlebar.rs):
 * - `titleBar: "overlay"` on macOS, where the title bar is an overlay on the page, as in macOS 26
 *   apps: the window controls (traffic lights) sit at the top left of the sidebar panel, and the
 *   band they share with the page toolbar moves the window.
 * - `windowRadius`, the window's corner radius in points on every platform, which the page's
 *   nested corners derive from (`--window-radius`, DESIGN.md "Shape").
 *
 * In a browser (the Docker image, the dev server) the global is absent and the page uses its
 * design values.
 */

declare global {
  interface Window {
    __STELLARATOR_DESKTOP__?: { titleBar?: string; windowRadius?: number }
  }
}

const desktop = typeof window !== "undefined" ? window.__STELLARATOR_DESKTOP__ : undefined

/** True in the macOS app, where the page draws under the title bar. */
export const overlayTitleBar = desktop?.titleBar === "overlay"

/** Window corners at least this round get the floating sidebar; smaller ones a flush sidebar. */
const FLOATING_SIDEBAR_MIN_RADIUS = 20

/**
 * Mark <html> for the desktop window: data-titlebar="overlay" so the sidebar reserves room for the
 * window controls (see "Desktop app on macOS" in index.css), the window's corner radius as
 * --window-radius, and data-window="flush" when those corners are too tight for a floating
 * sidebar to be concentric with them. The app adds data-fullscreen itself while the window is in
 * full screen. Call once, before the first render.
 */
export function applyDesktopShell() {
  const root = document.documentElement
  if (overlayTitleBar) root.dataset.titlebar = "overlay"
  const radius = desktop?.windowRadius
  if (typeof radius === "number" && Number.isFinite(radius) && radius >= 0) {
    root.style.setProperty("--window-radius", `${radius}px`)
    if (radius < FLOATING_SIDEBAR_MIN_RADIUS) root.dataset.window = "flush"
  }
}

/**
 * Whether the page toolbar moves the window: in the macOS app, except in full screen (the app sets
 * data-fullscreen on <html>), where a window cannot be moved.
 */
export function useToolbarDragRegion(): boolean {
  return useSyncExternalStore(
    (onChange) => {
      if (!overlayTitleBar) return () => {}
      const observer = new MutationObserver(onChange)
      observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-fullscreen"] })
      return () => observer.disconnect()
    },
    () => overlayTitleBar && !document.documentElement.hasAttribute("data-fullscreen"),
    () => false,
  )
}
