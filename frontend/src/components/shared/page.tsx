import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react"
import { Link, NavLink } from "react-router"
import { ChevronLeft, Settings } from "lucide-react"
import { Bar } from "@/components/shared/block-states"
import { buttonVariants } from "@/components/ui/button"
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

/** Phones: back to the parent screen, "‹ Runs", as iOS. The label goes once the small title takes the bar. */
function BackControl({ back, labelHidden = false }: { back: { to: string; label: string }; labelHidden?: boolean }) {
  return (
    <Link
      to={back.to}
      aria-label={`Back to ${back.label}`}
      className="-ml-2 inline-flex h-11 min-w-11 items-center gap-0.5 rounded-md pr-2 text-[17px] text-foreground"
    >
      <ChevronLeft className="size-6 flex-none" strokeWidth={2} aria-hidden="true" />
      {labelHidden ? null : <span className="truncate">{back.label}</span>}
    </Link>
  )
}

/**
 * From 768 px: where a detail screen sits, "Runs / Distill gsm8k", in the toolbar. The parent is a
 * link in muted ink, the current screen ink; both truncate, the parent last.
 */
function Breadcrumb({ back, current, hoverTitle }: { back: { to: string; label: string }; current: ReactNode; hoverTitle?: string }) {
  return (
    <nav aria-label="Breadcrumb" className="min-w-0 flex-1">
      <ol className="flex min-w-0 items-center gap-1.5 text-[15px] leading-5 tracking-[-0.011em]">
        <li className="flex min-w-0 shrink items-center gap-1.5">
          <Link
            to={back.to}
            className="-mx-1 truncate rounded-xs px-1 font-medium text-muted-foreground transition-colors duration-150 hover:text-foreground"
          >
            {back.label}
          </Link>
          <span aria-hidden="true" className="text-muted-foreground/60">
            /
          </span>
        </li>
        <li aria-current="page" title={hoverTitle} className="min-w-0 shrink-[2] truncate font-semibold">
          {current}
        </li>
      </ol>
    </nav>
  )
}

/** Phones: Settings sits at the leading end of each top-level screen's bar (the tab bar has four places). */
function PhoneSettingsLink() {
  return (
    <NavLink
      to="/settings"
      aria-label="Settings"
      title="Settings"
      // The Button's own attributes, so the touch rules size it like the "+" and "..." buttons.
      data-slot="button"
      data-size="icon"
      className={({ isActive }) => cn(buttonVariants({ variant: "secondary", size: "icon" }), "rounded-full", isActive && "bg-accent")}
    >
      <Settings aria-hidden="true" />
    </NavLink>
  )
}

/**
 * Every screen (DESIGN.md, Layout). A sticky toolbar runs along the top of the content column: at
 * rest it is the page color, and once content scrolls under it the bar material fades in. In the
 * macOS app it is also the title bar: its empty areas move the window.
 *
 * From 768 px:
 * - A top-level screen (no `back`) has its title, the page's h1, in the toolbar.
 * - A detail screen (`back` is its parent) has a breadcrumb in the toolbar ("Runs / Distill") and
 *   its h1 as the display title at the top of the content, followed by `meta` (the chips row).
 * - From 1024 px the actions sit at the end of the toolbar; below that, beside the meta under the
 *   title.
 *
 * On phones the bar holds the back button (or Settings on a top-level screen) and `barActions`
 * (icon buttons such as "+" or "…") at its trailing end, and the screen starts with a large title,
 * its meta and its actions, as iOS does; once the large title scrolls under the bar, a small title
 * fades in there. `title={null}` shows a loading bar.
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
  // The title in the content: the display title of a detail screen from 768 px, the large title on phones.
  const titleInContent = !toolbarLayout || !!back

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
          className="material-bar pointer-events-none absolute inset-0 border-b border-border opacity-0 transition-opacity duration-200 ease-out group-data-scrolled/toolbar:opacity-100"
        />
        <div
          className={cn(
            column,
            "relative grid h-(--toolbar-height) grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2",
            "md:flex md:gap-3",
          )}
        >
          {toolbarLayout ? (
            back ? (
              <Breadcrumb
                back={back}
                hoverTitle={hoverTitle}
                current={loading ? <Bar className="h-4 w-48 max-w-full align-middle" /> : title}
              />
            ) : (
              <h1 title={hoverTitle} className="min-w-0 flex-1 truncate text-[15px] leading-5 font-semibold tracking-[-0.011em]">
                {loading ? (
                  <>
                    <Bar className="h-4 w-56 max-w-full align-middle" />
                    <span className="sr-only">Loading…</span>
                  </>
                ) : (
                  title
                )}
              </h1>
            )
          ) : (
            <>
              <div className="flex min-w-0 items-center empty:hidden">
                {back ? <BackControl back={back} labelHidden={titleUnder} /> : <PhoneSettingsLink />}
              </div>
              <p
                aria-hidden="true"
                data-shown={titleUnder || undefined}
                className="max-w-0 min-w-0 truncate text-center text-[17px] font-semibold opacity-0 transition-opacity duration-200 ease-out data-shown:max-w-[50vw] data-shown:opacity-100"
              >
                {loading ? null : title}
              </p>
            </>
          )}
          <div className="flex items-center justify-end gap-2 empty:hidden max-md:[&_[data-slot=button]]:rounded-full">
            {actionsInBar ? actions : toolbarLayout ? null : barActions}
          </div>
        </div>
      </header>

      <div className={cn(column, "pt-1 pb-6 md:pt-4 lg:pb-8", back && toolbarLayout && "md:pt-6", className)}>
        {titleInContent ? (
          <h1
            ref={largeTitle}
            className="text-[28px] leading-[1.1] font-semibold tracking-[-0.03em] text-balance break-words md:text-[32px]"
          >
            {loading ? (
              <>
                <Bar className="h-7 w-72 max-w-full align-middle md:h-8" />
                <span className="sr-only">Loading…</span>
              </>
            ) : (
              title
            )}
          </h1>
        ) : null}
        {meta || contentActions ? (
          <div className={cn("mb-6 flex flex-wrap items-center justify-between gap-x-6 gap-y-3", titleInContent && "mt-3")}>
            {meta ? <div className="min-w-0 text-sm text-muted-foreground">{meta}</div> : null}
            {contentActions ? <div className="flex flex-wrap items-center gap-2">{contentActions}</div> : null}
          </div>
        ) : titleInContent ? (
          <div className="mb-6" />
        ) : null}
        {children}
      </div>
    </>
  )
}

/**
 * A card (DESIGN.md, Components): --muted, 16 px corners, no border, no shadow, with an optional
 * header row (a 15 px title, a description, actions). Never put a card inside a card.
 */
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
    <section id={id} className={cn("rounded-xl bg-muted", className)}>
      {title || actions ? (
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 px-4 pt-3.5 pb-1">
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
