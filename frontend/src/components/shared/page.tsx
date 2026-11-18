import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react"
import { Link, NavLink } from "react-router"
import { ChevronLeft, Settings } from "lucide-react"
import { Bar } from "@/components/shared/block-states"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { useToolbarDragRegion } from "@/lib/desktop-shell"
import { TOOLBAR_ACTIONS_QUERY, TOOLBAR_QUERY, useMediaQuery } from "@/lib/media"
import { cn } from "@/lib/utils"

/**
 * Whether the page has scrolled at all (the toolbar's material fades in), and whether the phone's
 * large title has gone under the toolbar (its small title fades in), as in macOS and iOS.
 */
function useToolbarScroll(bar: RefObject<HTMLElement | null>, largeTitle: RefObject<HTMLElement | null>, deps: unknown) {
  const [state, setState] = useState({ scrolled: false, titleUnder: false })
  useEffect(() => {
    let frame = 0
    const read = () => {
      frame = 0
      const scrolled = window.scrollY > 0
      const title = largeTitle.current?.getBoundingClientRect()
      const barBox = bar.current?.getBoundingClientRect()
      const titleUnder = !!title && !!barBox && title.bottom <= barBox.bottom
      setState((s) => (s.scrolled === scrolled && s.titleUnder === titleUnder ? s : { scrolled, titleUnder }))
    }
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(read)
    }
    read()
    window.addEventListener("scroll", schedule, { passive: true })
    window.addEventListener("resize", schedule)
    return () => {
      cancelAnimationFrame(frame)
      window.removeEventListener("scroll", schedule)
      window.removeEventListener("resize", schedule)
    }
  }, [bar, largeTitle, deps])
  return state
}

/** Back to the parent screen. A chevron toolbar button on wide screens; "‹ Runs" on phones, as iOS. */
function BackControl({ back, compact }: { back: { to: string; label: string }; compact: boolean }) {
  if (compact) {
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <Button asChild variant="outline" size="icon">
            <Link to={back.to} aria-label={`Back to ${back.label}`}>
              <ChevronLeft aria-hidden="true" />
            </Link>
          </Button>
        </TooltipTrigger>
        <TooltipContent side="bottom">Back to {back.label}</TooltipContent>
      </Tooltip>
    )
  }
  return (
    <Link
      to={back.to}
      className="-ml-2 inline-flex h-11 min-w-0 items-center gap-0.5 rounded-lg pr-2 text-[17px] text-foreground"
    >
      <ChevronLeft className="size-6 flex-none" strokeWidth={2} aria-hidden="true" />
      <span className="truncate">{back.label}</span>
    </Link>
  )
}

/** Phones: Settings sits at the leading end of each top-level screen's bar (the tab bar has four places). */
function PhoneSettingsLink() {
  return (
    <NavLink
      to="/settings"
      aria-label="Settings"
      className={({ isActive }) =>
        cn(
          "-ml-2.5 inline-flex size-11 items-center justify-center rounded-full text-muted-foreground hover:text-foreground",
          isActive && "text-foreground",
        )
      }
    >
      <Settings className="size-[22px]" strokeWidth={1.75} aria-hidden="true" />
    </NavLink>
  )
}

/**
 * Every screen. Its title and actions live in a toolbar at the top of the content column, as in
 * macOS 26 apps: a back button when the screen has a parent, the title (the page's h1), and the
 * actions as capsule toolbar buttons. The toolbar is sticky; at rest it is the page color, and
 * once content scrolls under it the bar material fades in. In the macOS app it is also the title
 * bar: its empty areas move the window.
 *
 * On phones the bar holds the back button (or Settings on a top-level screen) and the screen
 * starts with a large title, its meta line and its actions, as iOS does; once the large title
 * scrolls under the bar, a small title fades in there. Below 1024 px the actions sit under the
 * title instead of in the bar, where they would crowd it. On phones `barActions` (an icon button
 * such as "+" or "…") sits at the trailing end of the bar, as iOS puts them in a navigation bar.
 * `title={null}` shows a loading bar.
 */
export function Page({
  title,
  meta,
  actions,
  barActions,
  back,
  children,
  className,
  wide = false,
}: {
  title: ReactNode
  meta?: ReactNode
  actions?: ReactNode
  /** Phones only (below 768 px): icon buttons at the trailing end of the bar. */
  barActions?: ReactNode
  back?: { to: string; label: string }
  children: ReactNode
  className?: string
  wide?: boolean
}) {
  const toolbarLayout = useMediaQuery(TOOLBAR_QUERY)
  const actionsInBar = useMediaQuery(TOOLBAR_ACTIONS_QUERY)
  const bar = useRef<HTMLElement>(null)
  const largeTitle = useRef<HTMLHeadingElement>(null)
  const { scrolled, titleUnder } = useToolbarScroll(bar, largeTitle, toolbarLayout)
  const column = cn("mx-auto w-full px-[max(1rem,env(safe-area-inset-left))] sm:px-6 lg:px-8", wide ? "max-w-[96rem]" : "max-w-[80rem]")
  const hoverTitle = typeof title === "string" ? title : undefined
  const loading = title === null
  const contentActions = actions && !actionsInBar ? actions : null
  const dragsWindow = useToolbarDragRegion()

  return (
    <>
      <header
        ref={bar}
        data-slot="toolbar"
        data-scrolled={scrolled || undefined}
        data-tauri-drag-region={dragsWindow ? "deep" : undefined}
        className="group/toolbar sticky top-0 z-30 pt-[env(safe-area-inset-top)] md:pt-0"
      >
        <div
          aria-hidden="true"
          className="material-bar pointer-events-none absolute inset-0 border-b border-material-edge opacity-0 transition-opacity duration-200 ease-out group-data-scrolled/toolbar:opacity-100"
        />
        <div
          className={cn(
            column,
            "relative grid h-(--toolbar-height) grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2",
            "md:flex md:gap-3",
          )}
        >
          <div className="flex min-w-0 items-center empty:hidden">
            {back ? <BackControl back={back} compact={toolbarLayout} /> : toolbarLayout ? null : <PhoneSettingsLink />}
          </div>
          {toolbarLayout ? (
            <h1
              title={hoverTitle}
              className="min-w-0 flex-1 truncate text-[15px] leading-5 font-semibold tracking-[-0.01em] pointer-coarse:text-[17px]"
            >
              {loading ? (
                <>
                  <Bar className="h-4 w-56 max-w-full align-middle" />
                  <span className="sr-only">Loading…</span>
                </>
              ) : (
                title
              )}
            </h1>
          ) : (
            <p
              aria-hidden="true"
              data-shown={titleUnder || undefined}
              className="max-w-0 min-w-0 truncate text-center text-[17px] font-semibold opacity-0 transition-opacity duration-200 ease-out data-shown:max-w-[50vw] data-shown:opacity-100"
            >
              {loading ? null : title}
            </p>
          )}
          <div className="flex items-center justify-end gap-2 empty:hidden">
            {actionsInBar ? actions : toolbarLayout ? null : barActions}
          </div>
        </div>
      </header>

      <div className={cn(column, "pt-1 pb-6 md:pt-4 lg:pb-8", className)}>
        {toolbarLayout ? null : (
          <h1 ref={largeTitle} className="text-[28px] leading-tight font-semibold tracking-[-0.02em] break-words">
            {loading ? (
              <>
                <Bar className="h-7 w-64 max-w-full align-middle" />
                <span className="sr-only">Loading…</span>
              </>
            ) : (
              title
            )}
          </h1>
        )}
        {meta || contentActions ? (
          <div className="mb-6 flex flex-wrap items-start justify-between gap-x-6 gap-y-3 max-md:mt-1.5">
            {meta ? <div className="min-w-0 text-sm text-muted-foreground">{meta}</div> : null}
            {contentActions ? <div className="flex flex-wrap items-center gap-2">{contentActions}</div> : null}
          </div>
        ) : toolbarLayout ? null : (
          <div className="mb-5" />
        )}
        {children}
      </div>
    </>
  )
}

/** A bordered panel with a section heading (15 px on pointer-fine, 16 px on touch). No shadow. */
export function Panel({
  title,
  description,
  actions,
  children,
  className,
  bodyClassName,
  id,
}: {
  title?: ReactNode
  description?: ReactNode
  actions?: ReactNode
  children: ReactNode
  className?: string
  bodyClassName?: string
  id?: string
}) {
  return (
    <section id={id} className={cn("rounded-xl border bg-card", className)}>
      {title || actions ? (
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b px-4 py-3">
          <div className="min-w-0">
            {title ? <h2 className="text-base font-semibold">{title}</h2> : null}
            {description ? <p className="text-sm text-muted-foreground">{description}</p> : null}
          </div>
          {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
        </div>
      ) : null}
      <div className={cn("p-4", bodyClassName)}>{children}</div>
    </section>
  )
}
