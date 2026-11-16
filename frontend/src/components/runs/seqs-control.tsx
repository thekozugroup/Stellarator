import { useEffect, useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { ChevronDown, Minus, Plus } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { api } from "@/lib/api"
import { int } from "@/lib/format"
import type { Run, RunLive } from "@/lib/types"
import { cn } from "@/lib/utils"
import { runKeys } from "./run-data"

const PRESETS = [1, 4, 8, 16, 32, 64, 128, 256]
const MIN = 1
const MAX = 4096

/**
 * The run's one copper control. Each change is applied to the screen at once and sent to the
 * server after 400 ms of quiet; a failure puts the old value back and says why.
 */
export function SeqsControl({ run, inFlight }: { run: Run; inFlight: number }) {
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState(String(run.concurrency))
  const [editing, setEditing] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  const settled = useRef(run.concurrency) // last value the server confirmed
  const finished = run.status === "completed"

  const mutation = useMutation({
    mutationFn: (concurrency: number) => api.patch<Run>(`/api/runs/${run.id}`, { concurrency }),
    onSuccess: (updated) => {
      settled.current = updated.concurrency
      queryClient.setQueryData<RunLive>(runKeys.live(run.id), (old) => (old ? { ...old, run: { ...old.run, ...updated } } : old))
      void queryClient.invalidateQueries({ queryKey: runKeys.list })
    },
    onError: (e: Error) => {
      const back = settled.current
      setDraft(String(back))
      queryClient.setQueryData<RunLive>(runKeys.live(run.id), (old) =>
        old ? { ...old, run: { ...old.run, concurrency: back } } : old,
      )
      toast.error(`Couldn't change seqs. ${e.message}`, { description: `Seqs stays at ${int(back)}.` })
    },
  })

  // Follow the server while nothing is being typed or sent.
  useEffect(() => {
    if (!editing && timer.current === undefined && !mutation.isPending) {
      setDraft(String(run.concurrency))
      settled.current = run.concurrency
    }
  }, [run.concurrency, editing, mutation.isPending])

  useEffect(() => () => window.clearTimeout(timer.current), [])

  function apply(n: number) {
    const next = Math.min(MAX, Math.max(MIN, Math.round(n)))
    setDraft(String(next))
    void queryClient.cancelQueries({ queryKey: runKeys.live(run.id) })
    // Optimistic: the hint and chart show the new value straight away.
    queryClient.setQueryData<RunLive>(runKeys.live(run.id), (old) => (old ? { ...old, run: { ...old.run, concurrency: next } } : old))
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => {
      timer.current = undefined
      if (next !== settled.current) mutation.mutate(next)
    }, 400)
  }

  const parsed = Number.parseInt(draft, 10)
  const current = Number.isFinite(parsed) ? parsed : run.concurrency
  const invalid = draft.trim() !== "" && (!/^\d+$/.test(draft.trim()) || parsed < MIN || parsed > MAX)

  const hint = finished
    ? "Run finished. Run again to change seqs."
    : run.status === "paused"
      ? `Paused. The run sends ${int(current)} at once when it resumes.`
      : run.status === "pausing"
        ? `Pausing. ${int(inFlight)} still in flight.`
        : `In flight: ${int(inFlight)} of ${int(current)}`

  return (
    <div className="grid gap-1.5">
      <label htmlFor="seqs-value" className="text-xs text-muted-foreground">
        Seqs
      </label>
      <div className="flex items-center gap-1">
        <Button
          type="button"
          variant="outline"
          size="icon"
          aria-label="Decrease seqs"
          disabled={finished || current <= MIN}
          onClick={() => apply(current - 1)}
          className="pointer-coarse:size-11"
        >
          <Minus aria-hidden="true" />
        </Button>
        <Input
          id="seqs-value"
          type="number"
          inputMode="numeric"
          min={MIN}
          max={MAX}
          value={draft}
          disabled={finished}
          aria-invalid={invalid || undefined}
          aria-describedby="seqs-hint"
          onFocus={() => setEditing(true)}
          onChange={(e) => {
            setDraft(e.target.value)
            const v = e.target.value.trim()
            if (/^\d+$/.test(v) && Number(v) >= MIN && Number(v) <= MAX) apply(Number(v))
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur()
          }}
          onBlur={() => {
            setEditing(false)
            if (invalid || draft.trim() === "") setDraft(String(run.concurrency))
          }}
          className={cn(
            "h-8 w-20 border-copper text-center text-base font-medium text-copper-ink tabular md:text-base dark:border-copper",
            "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none pointer-coarse:h-11",
          )}
        />
        <Button
          type="button"
          variant="outline"
          size="icon"
          aria-label="Increase seqs"
          disabled={finished || current >= MAX}
          onClick={() => apply(current + 1)}
          className="pointer-coarse:size-11"
        >
          <Plus aria-hidden="true" />
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild disabled={finished}>
            <Button type="button" variant="ghost" size="sm" className="ml-0.5 text-muted-foreground pointer-coarse:h-11">
              Presets
              <ChevronDown aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-36">
            <DropdownMenuLabel>Set seqs to</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={String(current)} onValueChange={(v) => apply(Number(v))}>
              {PRESETS.map((n) => (
                <DropdownMenuRadioItem key={n} value={String(n)} className="tabular">
                  {n}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <p id="seqs-hint" className={cn("text-xs tabular", invalid ? "text-destructive" : "text-muted-foreground")}>
        {invalid ? `Enter a whole number from ${MIN} to ${int(MAX)}.` : hint}
      </p>
    </div>
  )
}
