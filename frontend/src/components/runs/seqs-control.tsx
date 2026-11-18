import { useEffect, useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Minus, Plus } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { api } from "@/lib/api"
import { int } from "@/lib/format"
import type { Run, RunLive } from "@/lib/types"
import { cn } from "@/lib/utils"
import { runKeys } from "./run-data"
import { SeqsChoices } from "./seqs-choices"

const MIN = 1
const MAX = 4096

/**
 * The run's one copper control, for a run that is going or paused. Each change is applied to the
 * screen at once and sent to the server after 400 ms of quiet; a failure puts the old value back
 * and says why. Common values sit below as the same segmented row as on New run.
 */
export function SeqsControl({ run, inFlight }: { run: Run; inFlight: number }) {
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState(String(run.concurrency))
  const [editing, setEditing] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  const settled = useRef(run.concurrency) // last value the server confirmed

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

  const hint = invalid
    ? `Enter a whole number from ${MIN} to ${int(MAX)}.`
    : run.status === "pausing"
      ? `Pausing. ${int(inFlight)} still in flight.`
      : run.status === "running"
        ? `In flight: ${int(inFlight)} of ${int(current)}`
        : null

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
          disabled={current <= MIN}
          onClick={() => apply(current - 1)}
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
          aria-invalid={invalid || undefined}
          aria-describedby={hint ? "seqs-hint" : undefined}
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
            "w-20 border-copper text-center text-base font-medium text-copper-ink tabular md:text-base dark:border-copper",
            "[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none",
          )}
        />
        <Button
          type="button"
          variant="outline"
          size="icon"
          aria-label="Increase seqs"
          disabled={current >= MAX}
          onClick={() => apply(current + 1)}
        >
          <Plus aria-hidden="true" />
        </Button>
      </div>
      <SeqsChoices value={current} onChange={apply} />
      {hint ? (
        <p id="seqs-hint" className={cn("text-xs tabular", invalid ? "text-destructive" : "text-muted-foreground")}>
          {hint}
        </p>
      ) : null}
    </div>
  )
}
