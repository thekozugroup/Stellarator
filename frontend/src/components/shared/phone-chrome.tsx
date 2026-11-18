import { useState, type ReactNode } from "react"
import { createPortal } from "react-dom"
import { TOOLBAR_QUERY, useMediaQuery } from "@/lib/media"

/**
 * Puts a control in the phone toolbar (the 44 px bar under the status bar), at its trailing end,
 * as iOS puts a "+" in a navigation bar. From 768 px up it renders nothing: the page passes the
 * same action to Page's `actions` there, which shows it in the toolbar or under it.
 *
 * Page only fills the bar's trailing slot from 1024 px up, so this reaches into that slot. If Page
 * gets a prop for phone-bar actions, use it and delete this.
 */
export function PhoneToolbarAction({ children }: { children: ReactNode }) {
  const wide = useMediaQuery(TOOLBAR_QUERY)
  const [slot, setSlot] = useState<Element | null>(null)
  return (
    <>
      {/* header[data-slot=toolbar] > its grid > the grid's last child, the actions slot. */}
      <span
        hidden
        ref={() => {
          const found = document.querySelector('header[data-slot="toolbar"]')?.lastElementChild?.lastElementChild ?? null
          setSlot((cur) => (cur === found ? cur : found))
        }}
      />
      {!wide && slot ? createPortal(children, slot) : null}
    </>
  )
}
