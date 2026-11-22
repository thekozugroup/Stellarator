import { Link, type To } from "react-router"
import { ChevronLeft, ChevronRight } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

interface Step {
  /** The button's name for assistive technology and its tooltip, "Previous page". */
  label: string
  /** A link target (the page pager), or a handler (the sample sheet). */
  to?: To
  onClick?: () => void
  /** Nothing in that direction: the button is dimmed and does nothing. */
  disabled?: boolean
  /** The keyboard shortcut, announced with the name. */
  shortcut?: string
}

function StepButton({ step, direction }: { step: Step; direction: "prev" | "next" }) {
  const Icon = direction === "prev" ? ChevronLeft : ChevronRight
  const common = {
    variant: "ghost" as const,
    size: "icon-xs" as const,
    className: "size-[26px] rounded-full",
    title: step.shortcut ? `${step.label} (${step.shortcut.toUpperCase()})` : step.label,
    "aria-label": step.label,
    "aria-keyshortcuts": step.shortcut,
  }
  if (step.to !== undefined && !step.disabled) {
    return (
      <Button asChild {...common}>
        <Link to={step.to}>
          <Icon className="size-4" aria-hidden="true" />
        </Link>
      </Button>
    )
  }
  return (
    <Button type="button" {...common} disabled={step.disabled} onClick={step.onClick}>
      <Icon className="size-4" aria-hidden="true" />
    </Button>
  )
}

/**
 * Previous and next as a ‹ › capsule pair, the way macOS and iOS step through a series. One raised
 * capsule with two round buttons; each keeps its own name ("Previous sample"), so no visible words
 * are needed.
 */
export function StepPair({ prev, next, className }: { prev: Step; next: Step; className?: string }) {
  return (
    <div className={cn("inline-flex items-center gap-0.5 rounded-full bg-secondary p-0.5", className)}>
      <StepButton step={prev} direction="prev" />
      <StepButton step={next} direction="next" />
    </div>
  )
}
