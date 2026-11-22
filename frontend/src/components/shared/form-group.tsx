import { useId, type ReactNode } from "react"
import { ChevronRight } from "lucide-react"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Label } from "@/components/ui/label"
import { INFO_ROW, InfoTip } from "@/components/shared/info-tip"
import { cn } from "@/lib/utils"

/*
 * The System Settings grouped form (DESIGN.md, Components). A group is a --muted card with 16 px
 * corners and no border; its rows are separated by hairlines inset 12 px from the card edge. A row
 * puts its label on the left and its control on the right once the group is 28rem wide (a
 * container query, so a narrow window or a phone stacks them whatever the input). An optional
 * title sits above the box and an optional footnote below it, both in secondary text.
 *
 *   <FormGroup title="Requests" footnote="Seqs can change while a run is going.">
 *     <FormRow id="seqs" label="Seqs" hint="Match your server's --max-num-seqs.">
 *       <Input id="seqs" />
 *     </FormRow>
 *   </FormGroup>
 *
 * FormRow ties its label to the control with htmlFor, and its hint and error to the control through
 * the ids `${id}-hint` and `${id}-error`: put those in the control's aria-describedby. A row whose
 * control is several elements (a segmented control, a stepper) passes `group` instead of `id`; the
 * label then names the role="group" around the control. A row with neither is a read-only fact.
 * GroupBox is the same box without the row layout, for panels that hold a chart or a list.
 */

const BOX = "@container/fg rounded-xl bg-muted"

/** The box with its title above and footnote below. Rows and content go inside. */
export function GroupBox({
  title,
  footnote,
  children,
  className,
  boxClassName,
  titleClassName,
  id,
}: {
  title?: ReactNode
  footnote?: ReactNode
  children: ReactNode
  className?: string
  boxClassName?: string
  titleClassName?: string
  id?: string
}) {
  const uid = useId()
  const titleId = title ? `${uid}-title` : undefined
  return (
    <section id={id} aria-labelledby={titleId} className={cn("grid min-w-0 content-start gap-1.5", className)}>
      {title ? (
        <h2 id={titleId} className={cn("px-3 text-sm font-semibold text-muted-foreground", titleClassName)}>
          {title}
        </h2>
      ) : null}
      <div className={cn(BOX, boxClassName)}>{children}</div>
      {footnote ? <p className="px-3 text-xs text-muted-foreground">{footnote}</p> : null}
    </section>
  )
}

/** A form group: the box for FormRow children. */
export function FormGroup(props: Parameters<typeof GroupBox>[0]) {
  return <GroupBox {...props} />
}

/** Inset hairline above every row but the first. */
const SEPARATOR =
  "relative not-first:before:pointer-events-none not-first:before:absolute not-first:before:inset-x-3 not-first:before:top-0 not-first:before:h-px not-first:before:bg-border"

/** A row with any content, for a line of buttons or a fact that has no label. */
export function FormItem({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn(SEPARATOR, "px-3 py-2 pointer-coarse:py-1", className)}>{children}</div>
}

/**
 * Label left, control right (stacked below 28rem). `align="end"` pushes a compact control (a
 * stepper, a segmented control) to the trailing edge of its column. `stacked` always puts the label
 * above the control, for text areas.
 */
export function FormRow({
  id,
  group,
  label,
  optional,
  info,
  hint,
  hintLive,
  error,
  align = "fill",
  stacked = false,
  children,
  className,
}: {
  /** The id of the one control the label names. */
  id?: string
  /** The label names a role="group" around several controls instead of one control. */
  group?: boolean
  label: ReactNode
  optional?: boolean
  /** An (i) button after the label that explains the row in a sentence or two. */
  info?: ReactNode
  hint?: ReactNode
  /** Announce changes to the hint (a refresh result): the hint element is always present. */
  hintLive?: boolean
  error?: string | null
  align?: "fill" | "end"
  stacked?: boolean
  children: ReactNode
  className?: string
}) {
  const uid = useId()
  const labelId = `${id ?? uid}-label`
  const labelText = (
    <>
      {label}
      {optional ? <span className="font-normal text-muted-foreground"> (optional)</span> : null}
    </>
  )
  const labelClass = "text-sm leading-snug font-normal"
  const labelEl = id ? (
    <Label id={labelId} htmlFor={id} className={labelClass}>
      {labelText}
    </Label>
  ) : (
    <span id={labelId} className={labelClass}>
      {labelText}
    </span>
  )
  const labelTip = info ? (
    <div className={INFO_ROW}>
      {labelEl}
      <InfoTip label={typeof label === "string" ? label : (id ?? "this setting")}>{info}</InfoTip>
    </div>
  ) : (
    labelEl
  )
  const hintId = id ? `${id}-hint` : undefined
  const errorId = id ? `${id}-error` : undefined
  const note = error ? (
    <p id={errorId} className="text-sm text-negative-ink">
      {error}
    </p>
  ) : hint || hintLive ? (
    <p id={hintId} aria-live={hintLive ? "polite" : undefined} className="text-xs text-muted-foreground">
      {hint}
    </p>
  ) : null

  return (
    <div
      className={cn(
        SEPARATOR,
        "grid gap-1.5 px-3 py-2 pointer-coarse:py-1.5",
        !stacked && "@md/fg:grid-cols-[minmax(0,12rem)_minmax(0,1fr)] @md/fg:items-start @md/fg:gap-x-4",
        className,
      )}
    >
      <div className={cn("flex items-center", !stacked && "@md/fg:min-h-8 @md/fg:pointer-coarse:min-h-11")}>{labelTip}</div>
      <div
        role={group ? "group" : undefined}
        aria-labelledby={group ? labelId : undefined}
        className={cn("grid min-w-0 content-start gap-1.5", align === "end" && !stacked && "@md/fg:justify-items-end")}
      >
        {children}
        {note ? <div className={cn(align === "end" && !stacked && "@md/fg:text-right")}>{note}</div> : null}
      </div>
    </div>
  )
}

/**
 * A group that opens and closes, for the settings most runs never touch. The header row is the
 * trigger: the title at the start, a short summary of what is set and a chevron at the end. The
 * rows inside keep the group's hairlines.
 */
export function FormDisclosure({
  title,
  summary,
  open,
  onOpenChange,
  children,
}: {
  title: string
  summary: string
  open: boolean
  onOpenChange: (open: boolean) => void
  children: ReactNode
}) {
  return (
    <Collapsible open={open} onOpenChange={onOpenChange} className={cn(BOX)}>
      <h2 className="text-sm font-normal">
        <CollapsibleTrigger className="group flex min-h-9 w-full items-center gap-2 px-3 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring rounded-xl">
          <ChevronRight
            className="size-4 flex-none text-muted-foreground transition-transform duration-200 ease-out group-data-[state=open]:rotate-90"
            aria-hidden="true"
          />
          <span className="font-semibold">{title}</span>
          {!open ? <span className="ml-auto min-w-0 truncate text-muted-foreground">{summary}</span> : null}
        </CollapsibleTrigger>
      </h2>
      <CollapsibleContent className="relative before:pointer-events-none before:absolute before:inset-x-3 before:top-0 before:h-px before:bg-border">
        {children}
      </CollapsibleContent>
    </Collapsible>
  )
}
