import { cn } from "@/lib/utils"
import { int } from "@/lib/format"

/**
 * Progress at its true ratio. Done is copper, failed is red, both labelled in text beside it.
 * Updates in place; the width transition is short and has no travel under reduced motion.
 */
export function ProgressBar({
  done,
  failed = 0,
  total,
  className,
  label,
}: {
  done: number
  failed?: number
  total: number
  className?: string
  label?: string
}) {
  const d = total ? Math.min(100, (done / total) * 100) : 0
  const f = total ? Math.min(100 - d, (failed / total) * 100) : 0
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={done + failed}
      aria-label={label ?? `${int(done)} of ${int(total)} done${failed ? `, ${int(failed)} failed` : ""}`}
      className={cn("flex h-1.5 w-full overflow-hidden rounded-full bg-muted", className)}
    >
      <div className="h-full bg-copper transition-[width] duration-300 ease-out" style={{ width: `${d}%` }} />
      <div className="h-full bg-destructive transition-[width] duration-300 ease-out" style={{ width: `${f}%` }} />
    </div>
  )
}
