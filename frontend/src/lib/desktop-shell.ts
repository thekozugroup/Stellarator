import { useSyncExternalStore } from "react"

/**
 * The desktop app's window on macOS. Its title bar is an overlay on the page, as in macOS 26 apps:
 * the window controls (traffic lights) sit at the top left of the sidebar panel, and the band they
 * share with the page toolbar moves the window. The app sets `window.__STELLARATOR_DESKTOP__`
 * before this code runs (desktop/src-tauri/src/titlebar.rs).
 *
 * In a browser (the Docker image, the dev server) and in the Windows and Linux app the global is
 * absent and nothing here changes the page.
 */

declare global {
  interface Window {
    __STELLARATOR_DESKTOP__?: { titleBar?: string }
  }
}

/** True in the macOS app, where the page draws under the title bar. */
export const overlayTitleBar =
  typeof window !== "undefined" && window.__STELLARATOR_DESKTOP__?.titleBar === "overlay"

/**
 * Mark <html> with data-titlebar="overlay" so the sidebar reserves room for the window controls
 * (see "Desktop app on macOS" in index.css). The app adds data-fullscreen itself while the window
 * is in full screen. Call once, before the first render.
 */
export function applyDesktopShell() {
  if (overlayTitleBar) document.documentElement.dataset.titlebar = "overlay"
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
