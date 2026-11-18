import { useSyncExternalStore } from "react"

/** Whether a media query matches, kept in step as the window resizes or the setting changes. */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const list = window.matchMedia(query)
      list.addEventListener("change", onChange)
      return () => list.removeEventListener("change", onChange)
    },
    () => window.matchMedia(query).matches,
    () => false,
  )
}

/** The toolbar layout (sidebar, toolbar title) from 768 px up; phones below. */
export const TOOLBAR_QUERY = "(min-width: 48rem)"
/** From 1024 px up the content column has room for a page's actions in the toolbar. */
export const TOOLBAR_ACTIONS_QUERY = "(min-width: 64rem)"
