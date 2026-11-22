import type { ReactNode } from "react"
import { Label } from "@/components/ui/label"
import { INFO_ROW, InfoTip } from "@/components/shared/info-tip"
import { cn } from "@/lib/utils"

/**
 * Label above control, hint below, error under the field (replacing the hint). `info` adds an (i)
 * button after the label that explains the field in a sentence or two.
 */
export function Field({
  id,
  label,
  hint,
  error,
  info,
  children,
  className,
  optional,
}: {
  id: string
  label: ReactNode
  hint?: ReactNode
  error?: string | null
  info?: ReactNode
  children: ReactNode
  className?: string
  optional?: boolean
}) {
  const labelEl = (
    <Label htmlFor={id} className="text-sm font-medium">
      {label}
      {optional ? <span className="font-normal text-muted-foreground"> (optional)</span> : null}
    </Label>
  )
  return (
    <div className={cn("grid gap-1.5", className)}>
      {info ? (
        <div className={INFO_ROW}>
          {labelEl}
          <InfoTip label={typeof label === "string" ? label : id}>{info}</InfoTip>
        </div>
      ) : (
        labelEl
      )}
      {children}
      {error ? (
        <p id={`${id}-error`} className="text-sm text-negative-ink">
          {error}
        </p>
      ) : hint ? (
        <p id={`${id}-hint`} className="text-xs text-muted-foreground">
          {hint}
        </p>
      ) : null}
    </div>
  )
}
