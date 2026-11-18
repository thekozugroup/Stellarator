import { cn } from "@/lib/utils"

const BAR_BELOW_LG =
  "max-lg:material-bar max-lg:sticky max-lg:bottom-0 max-lg:z-20 max-lg:-mx-4 max-lg:border-t max-lg:border-material-edge max-lg:px-4 max-lg:pt-3 max-lg:pb-[calc(0.75rem+var(--bottom-chrome))] max-lg:-mb-[calc(1.5rem+var(--bottom-chrome))] sm:max-lg:-mx-6 sm:max-lg:px-6"
const BAR_ALWAYS =
  "material-bar sticky bottom-0 z-20 -mx-4 border-t border-material-edge px-4 pt-3 pb-[calc(0.75rem+var(--bottom-chrome))] -mb-[calc(1.5rem+var(--bottom-chrome))] sm:-mx-6 sm:px-6 lg:-mx-8 lg:px-8 lg:-mb-8"

/**
 * The classes of a sticky bottom action bar (Import, preference review): the bar material, and on
 * phones it runs down behind the floating tab bar. Its bottom padding clears the tab bar, and a
 * negative bottom margin lets it cover the page padding and `--bottom-chrome` below it, so no
 * strip of content shows between the bar and the tab bar. With `below: "lg"` it is a bar only
 * under 1024 px and plain content from there (Import); with null it is a bar at every width.
 */
export function actionBarClass(below: "lg" | null, className?: string) {
  return cn(below === "lg" ? BAR_BELOW_LG : BAR_ALWAYS, className)
}
