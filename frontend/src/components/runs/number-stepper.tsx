import { Minus, Plus } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"

/**
 * A whole-number input with − and + beside it, the buttons in the raised (secondary) tone. The
 * value stays a string so typing is never blocked; the buttons clamp to [min, max]. Up and Down
 * arrows work as in any number input.
 */
export function NumberStepper({
  id,
  name,
  value,
  onChange,
  onBlur,
  min,
  max,
  step = 1,
  invalid,
  describedBy,
  disabled,
  className,
  inputClassName,
}: {
  id: string
  /** What the number counts, for the button names: "Decrease seqs". */
  name: string
  value: string
  onChange: (value: string) => void
  onBlur?: () => void
  min: number
  max: number
  step?: number
  invalid?: boolean
  describedBy?: string
  disabled?: boolean
  className?: string
  inputClassName?: string
}) {
  const n = Number.parseInt(value, 10)
  const current = Number.isFinite(n) ? n : min
  const set = (next: number) => onChange(String(Math.min(max, Math.max(min, next))))
  return (
    <div className={cn("flex items-center gap-1", className)}>
      <Button
        type="button"
        variant="secondary"
        size="icon"
        aria-label={`Decrease ${name}`}
        disabled={disabled || current <= min}
        onClick={() => set(current - step)}
      >
        <Minus aria-hidden="true" />
      </Button>
      <Input
        id={id}
        type="number"
        inputMode="numeric"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        className={cn(
          "w-20 text-center tabular [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
          inputClassName,
        )}
      />
      <Button
        type="button"
        variant="secondary"
        size="icon"
        aria-label={`Increase ${name}`}
        disabled={disabled || current >= max}
        onClick={() => set(current + step)}
      >
        <Plus aria-hidden="true" />
      </Button>
    </div>
  )
}
