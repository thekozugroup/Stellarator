import { cn } from "@/lib/utils"

const BAR =
  "material-bar material-float sticky bottom-[max(0.5rem,var(--bottom-chrome))] z-20 rounded-(--panel-radius) px-4 py-3"
const BAR_BELOW_LG =
  "max-lg:material-bar max-lg:material-float max-lg:sticky max-lg:bottom-[max(0.5rem,var(--bottom-chrome))] max-lg:z-20 max-lg:rounded-(--panel-radius) max-lg:px-4 max-lg:py-3"

/**
 * The classes of a sticky bottom action bar (New run, Import, preference review): a floating
 * panel in the bar material with --panel-radius corners, like the sidebar. It sticks 8 px above
 * the window's bottom edge while the page scrolls under it, and on phones it floats above the tab
 * bar (--bottom-chrome) instead of stacking on it. When the page is shorter than the window it
 * sits right after the content. With `below: "lg"` it is a bar only under 1024 px and plain
 * content from there (Import); with null it is a bar at every width.
 */
export function actionBarClass(below: "lg" | null, className?: string) {
  return cn(below === "lg" ? BAR_BELOW_LG : BAR, className)
}
