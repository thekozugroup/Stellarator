import type { ReactNode } from "react"
import { cn } from "@/lib/utils"

/**
 * Facts as tiles (DESIGN.md, Layout): --muted tiles with 16 px corners and 16 px padding, each a
 * figure (28 px, tabular) over its label (12 px muted). At most four, sharing the row equally; a tile with `span`
 * takes two columns and can hold a progress bar under its figure (`children`). Never repeat a
 * count that is shown elsewhere on the screen.
 *
 *   <Tiles>
 *     <Tile label="of 7,473 samples" value="3,759" span><ProgressBar … /></Tile>
 *     <Tile label="Output tokens/s" value="320" />
 *   </Tiles>
 */
export function Tiles({ children, className }: { children: ReactNode; className?: string }) {
  return <dl className={cn("grid grid-cols-2 gap-2 lg:grid-cols-[repeat(auto-fit,minmax(11rem,1fr))]", className)}>{children}</dl>
}

export function Tile({
  label,
  value,
  unit,
  span = false,
  children,
  className,
}: {
  label: ReactNode
  value: ReactNode
  /** Smaller text after the figure: "tok/s", "of 7,473". */
  unit?: ReactNode
  span?: boolean
  children?: ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1 rounded-xl bg-muted p-4", span && "col-span-2", className)}>
      <dt className="order-2 truncate text-xs text-muted-foreground">{label}</dt>
      <dd className="order-1 flex min-w-0 items-baseline gap-1.5">
        <span className="text-figure truncate">{value}</span>
        {unit ? <span className="truncate text-sm text-muted-foreground tabular">{unit}</span> : null}
      </dd>
      {children ? <dd className="order-3 mt-2">{children}</dd> : null}
    </div>
  )
}
