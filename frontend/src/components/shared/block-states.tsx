import type { ReactNode } from "react"
import { RotateCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

/** Failure inside the block that failed: what failed, and Retry. Never a full-screen takeover. */
export function ErrorBlock({
  message,
  onRetry,
  className,
}: {
  message: string
  onRetry?: () => void
  className?: string
}) {
  return (
    <div
      role="alert"
      className={cn("flex flex-wrap items-center justify-between gap-3 rounded-lg bg-negative-tint px-4 py-3 text-sm text-negative-ink", className)}
    >
      <p className="min-w-0">{message}</p>
      {onRetry ? (
        <Button variant="outline" size="sm" onClick={onRetry} className="bg-background">
          <RotateCw aria-hidden="true" />
          Retry
        </Button>
      ) : null}
    </div>
  )
}

/** An empty section keeps its frame: one sentence about what will appear, and the action that fills it. */
export function EmptyBlock({ children, action, className }: { children: ReactNode; action?: ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col items-start gap-3 py-6 text-sm text-muted-foreground", className)}>
      <div className="max-w-[60ch]">{children}</div>
      {action}
    </div>
  )
}

/** Flat gray bar at the real size of the value it stands in for. No shimmer. */
export function Bar({ className }: { className?: string }) {
  return <span aria-hidden="true" className={cn("inline-block h-4 w-16 rounded bg-muted", className)} />
}
