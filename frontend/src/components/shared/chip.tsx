import type { ReactNode } from "react"
import { Link } from "react-router"
import { cn } from "@/lib/utils"

/**
 * A chip (DESIGN.md, Components): a 28 px capsule in the raised tone with 13 px ink text. In a
 * detail screen's header, after the status badge, chips name the objects the screen uses (model,
 * endpoint, prompt set). A chip with `to` is a link and shows the highlight on hover; `mono` sets
 * its text in Roboto Mono (model names, IDs). `leading` holds a small icon or plasma tile.
 */
export function Chip({
  children,
  to,
  mono = false,
  leading,
  title,
  className,
}: {
  children: ReactNode
  to?: string
  mono?: boolean
  leading?: ReactNode
  title?: string
  className?: string
}) {
  const classes = cn(
    "inline-flex h-7 max-w-full min-w-0 items-center gap-1.5 rounded-full bg-secondary px-3 text-sm text-foreground",
    leading && "pl-1.5",
    mono && "font-mono text-xs",
    to && "transition-colors duration-150 outline-none hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
    className,
  )
  const body = (
    <>
      {leading}
      <span className="truncate">{children}</span>
    </>
  )
  return to ? (
    <Link to={to} title={title} className={classes}>
      {body}
    </Link>
  ) : (
    <span title={title} className={classes}>
      {body}
    </span>
  )
}

/** A row of chips (and a status badge first), wrapping. */
export function ChipRow({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("flex min-w-0 flex-wrap items-center gap-2", className)}>{children}</div>
}
