import { useEffect, useId, useRef, useState, type PointerEvent, type ReactNode } from "react"
import { Info } from "lucide-react"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { cn } from "@/lib/utils"

/** Hover delay before a tip opens, and the grace period for moving the pointer onto it. */
const OPEN_MS = 300
const CLOSE_MS = 150

/**
 * Classes for the row that holds a label and its InfoTip. On touch screens the tip's 44 pt hit
 * area reaches 13 px above and 11 px below the 20 px icon, so the row gets 8 px more room below
 * and the hit area stays clear of the control under it.
 */
export const INFO_ROW = "flex items-center gap-1 pointer-coarse:mb-2"

type OpenedBy = "hover" | "focus" | "click"

/**
 * An (i) button after a label that explains the field in a sentence or two. It opens on hover
 * (mouse), on keyboard focus, and on click or tap; Esc, a click outside or moving focus away closes
 * it. A click while it is open from hover keeps it open. The text is also the button's accessible
 * description, so screen readers read it with the button.
 */
export function InfoTip({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  const [openedBy, setOpenedBy] = useState<OpenedBy | null>(null)
  const timer = useRef<number | undefined>(undefined)
  const descId = useId()

  const clear = () => window.clearTimeout(timer.current)
  const after = (ms: number, fn: () => void) => {
    clear()
    timer.current = window.setTimeout(fn, ms)
  }
  useEffect(() => clear, [])

  const enter = (e: PointerEvent) => {
    if (e.pointerType !== "mouse") return
    if (openedBy) clear()
    else after(OPEN_MS, () => setOpenedBy((o) => o ?? "hover"))
  }
  const leave = (e: PointerEvent) => {
    if (e.pointerType !== "mouse") return
    after(CLOSE_MS, () => setOpenedBy((o) => (o === "hover" ? null : o)))
  }

  return (
    <Popover
      open={openedBy !== null}
      onOpenChange={(o) => {
        if (!o) {
          clear()
          setOpenedBy(null)
        }
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`About ${label}`}
          aria-describedby={descId}
          data-touch="inline"
          onPointerEnter={enter}
          onPointerLeave={leave}
          onFocus={(e) => {
            let visible = false
            try {
              visible = e.currentTarget.matches(":focus-visible")
            } catch {
              /* older engines: open on click only */
            }
            if (visible) setOpenedBy((o) => o ?? "focus")
          }}
          onBlur={() => setOpenedBy((o) => (o === "focus" ? null : o))}
          onClick={(e) => {
            // The open state is ours, not Radix's toggle: a click pins a tip opened by hover.
            e.preventDefault()
            clear()
            setOpenedBy((o) => (o === null || o === "hover" ? "click" : null))
          }}
          className={cn(
            "relative -my-[3px] inline-flex size-5 flex-none items-center justify-center rounded-full text-muted-foreground transition-colors outline-none", // taste-ignore: outline-check -- the next line draws the focus-visible ring
            "hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring data-[state=open]:text-foreground",
            "after:absolute after:-inset-0.5 after:content-[''] pointer-coarse:after:-inset-x-3 pointer-coarse:after:-top-[13px] pointer-coarse:after:-bottom-[11px]",
            className,
          )}
        >
          <Info className="size-4" strokeWidth={1.75} aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <span id={descId} className="sr-only">
        {children}
      </span>
      <PopoverContent
        side="top"
        sideOffset={6}
        collisionPadding={16}
        aria-label={`About ${label}`}
        onOpenAutoFocus={(e) => e.preventDefault()}
        onCloseAutoFocus={(e) => e.preventDefault()}
        onPointerEnter={(e) => e.pointerType === "mouse" && clear()}
        onPointerLeave={leave}
        className="w-auto max-w-[min(18rem,calc(100vw-2rem))] p-3 leading-normal"
      >
        {children}
      </PopoverContent>
    </Popover>
  )
}
