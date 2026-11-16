import type { ReactNode } from "react"
import { Link } from "react-router"
import { ChevronLeft } from "lucide-react"
import { cn } from "@/lib/utils"

/**
 * Every screen: one h1 (a noun, 24 px), an optional back link that names the parent, and the
 * screen's actions on the right. Content sits in a column capped at 80rem.
 */
export function Page({
  title,
  meta,
  actions,
  back,
  children,
  className,
  wide = false,
}: {
  title: ReactNode
  meta?: ReactNode
  actions?: ReactNode
  back?: { to: string; label: string }
  children: ReactNode
  className?: string
  wide?: boolean
}) {
  return (
    <div className={cn("mx-auto w-full px-[max(1rem,env(safe-area-inset-left))] py-6 sm:px-6 lg:px-8 lg:py-8", wide ? "max-w-[96rem]" : "max-w-[80rem]", className)}>
      {back ? (
        <Link
          to={back.to}
          className="-ml-1.5 mb-2 inline-flex min-h-8 items-center gap-0.5 rounded-md pr-1.5 text-sm text-muted-foreground hover:text-foreground pointer-coarse:min-h-11"
        >
          <ChevronLeft className="size-4" aria-hidden="true" />
          {back.label}
        </Link>
      ) : null}
      <header className="mb-6 flex flex-wrap items-start justify-between gap-x-6 gap-y-3">
        <div className="min-w-0">
          <h1 className="text-2xl font-semibold leading-tight tracking-[-0.01em] break-words">{title}</h1>
          {meta ? <div className="mt-1.5 text-sm text-muted-foreground">{meta}</div> : null}
        </div>
        {actions ? <div className="flex flex-wrap items-center gap-2">{actions}</div> : null}
      </header>
      {children}
    </div>
  )
}

/** A bordered panel with a 16 px heading. No shadow. */
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
