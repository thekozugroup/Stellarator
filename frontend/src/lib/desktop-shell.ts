/**
 * The desktop app's window on macOS. Its title bar is an overlay on the page: the window controls
 * (traffic lights) sit on the sidebar, and the strip across the top of the window moves it. The
 * app sets `window.__STELLARATOR_DESKTOP__` before this code runs (desktop/src-tauri/src/titlebar.rs).
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
 * Mark <html> with data-titlebar="overlay" so the CSS reserves room for the window controls
 * (see "Desktop app on macOS" in index.css). The app adds data-fullscreen itself while the window
 * is in full screen. Call once, before the first render.
 */
export function applyDesktopShell() {
  if (overlayTitleBar) document.documentElement.dataset.titlebar = "overlay"
}
