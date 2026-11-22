import { useMemo, useRef, useState, type FormEvent } from "react"
import { Link, useNavigate } from "react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ErrorBlock, Bar } from "@/components/shared/block-states"
import { FormGroup, FormRow } from "@/components/shared/form-group"
import { Plasma } from "@/components/shared/plasma"
import { RunStatusPill } from "@/components/shared/status-pill"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { api, ApiError } from "@/lib/api"
import { count, int } from "@/lib/format"
import type { PreferenceSet, Run } from "@/lib/types"
import { cn } from "@/lib/utils"
import { prefKeys, shortModel } from "./queries"

/**
 * Inside the thick material the page's --muted is the same tone as the dialog (244 on 244 in light),
 * so groups and wells inside it take the ground in light and the raised tone in dark, as grouped
 * sections do on iOS sheets.
 */
const ON_MATERIAL = "[--muted:var(--background)] dark:[--muted:var(--secondary)]"

interface Group {
  promptSetId: number
  promptSetName: string
  runs: Run[]
}

function groupRuns(runs: Run[]): Group[] {
  const groups = new Map<number, Group>()
  for (const run of runs) {
    let g = groups.get(run.prompt_set_id)
    if (!g) {
      g = { promptSetId: run.prompt_set_id, promptSetName: run.prompt_set_name, runs: [] }
      groups.set(run.prompt_set_id, g)
    }
    g.runs.push(run)
  }
  // Runs arrive newest first, so groups are ordered by their newest run.
  return [...groups.values()]
}

function suggestName(selected: Run[]): string {
  if (!selected.length) return ""
  if (selected.length === 1) return `${shortModel(selected[0].model)} samples`
  const models = [...new Set(selected.map((r) => shortModel(r.model)))]
  return models.length > 1 ? models.join(" vs ") : selected.map((r) => r.name).join(" vs ")
}

export function NewPreferenceSetDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined} className={cn("max-h-[90dvh] grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden sm:max-w-xl", ON_MATERIAL)}>
        {open ? <NewSetForm onDone={() => onOpenChange(false)} /> : null}
      </DialogContent>
    </Dialog>
  )
}

function NewSetForm({ onDone }: { onDone: () => void }) {
  const navigate = useNavigate()
  const qc = useQueryClient()
  const runsQuery = useQuery({ queryKey: ["runs"], queryFn: () => api.get<Run[]>("/api/runs") })
  const runs = useMemo(() => runsQuery.data ?? [], [runsQuery.data])
  const groups = useMemo(() => groupRuns(runs), [runs])

  const [selected, setSelected] = useState<number[]>([])
  const [name, setName] = useState("")
  const [nameTouched, setNameTouched] = useState(false)
  const [errors, setErrors] = useState<{ name?: string; runs?: string; form?: string }>({})
  const nameRef = useRef<HTMLInputElement>(null)
  const runsRef = useRef<HTMLFieldSetElement>(null)

  const selectedRuns = selected.map((id) => runs.find((r) => r.id === id)).filter((r): r is Run => !!r)
  const lockedSet = selectedRuns[0]?.prompt_set_id ?? null
  const lockedSetName = selectedRuns[0]?.prompt_set_name ?? ""
  const shownName = nameTouched ? name : suggestName(selectedRuns)

  const create = useMutation({
    mutationFn: (body: { name: string; run_ids: number[] }) => api.post<PreferenceSet>("/api/preference-sets", body),
    onSuccess: (set) => {
      qc.invalidateQueries({ queryKey: prefKeys.list })
      onDone()
      navigate(`/preferences/${set.id}`)
    },
    onError: (err) => {
      if (err instanceof ApiError && err.fields.name) {
        setErrors({ name: err.fields.name })
        nameRef.current?.focus()
      } else if (err instanceof ApiError && err.fields.run_ids) {
        setErrors({ runs: err.fields.run_ids })
      } else {
        setErrors({ form: err instanceof Error ? err.message : "Couldn't create the preference set. Try again." })
      }
    },
  })

  function toggle(run: Run, on: boolean) {
    setErrors((e) => ({ ...e, runs: undefined, form: undefined }))
    setSelected((cur) => (on ? [...cur, run.id] : cur.filter((id) => id !== run.id)))
  }

  function submit(e: FormEvent) {
    e.preventDefault()
    if (create.isPending) return
    const finalName = shownName.trim()
    const next: typeof errors = {}
    if (!finalName) next.name = "Enter a name for the preference set."
    if (!selectedRuns.length) next.runs = "Pick at least one run."
    else if (selectedRuns.length === 1 && selectedRuns[0].params.samples_per_prompt < 2)
      next.runs = "This run has one response per prompt. Pick a second run on the same prompt set to compare."
    setErrors(next)
    if (next.name) {
      nameRef.current?.focus()
      return
    }
    if (next.runs) {
      runsRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus()
      return
    }
    create.mutate({ name: finalName, run_ids: selected })
  }

  return (
    <form onSubmit={submit} noValidate className="contents">
      <DialogHeader>
        <DialogTitle>New preference set</DialogTitle>
      </DialogHeader>

      {/* The scroll area runs to the dialog's edges so focus rings are not clipped. */}
      <div className="-mx-5 grid min-h-0 content-start gap-5 overflow-y-auto px-5 py-1">
        <FormGroup>
          <FormRow id="pref-name" label="Name" error={errors.name}>
            <Input
              id="pref-name"
              ref={nameRef}
              value={shownName}
              onChange={(e) => {
                setNameTouched(true)
                setName(e.target.value)
                if (errors.name) setErrors((x) => ({ ...x, name: undefined }))
              }}
              placeholder="Qwen3-32B vs Llama-3.1-8B"
              autoComplete="off"
              maxLength={300}
              aria-invalid={errors.name ? true : undefined}
              aria-describedby={errors.name ? "pref-name-error" : undefined}
            />
          </FormRow>
        </FormGroup>

        <fieldset ref={runsRef} className="grid min-w-0 gap-1.5" aria-describedby="pref-runs-hint">
          <legend className="mb-1.5 px-3 text-sm font-semibold text-muted-foreground">Runs</legend>
          {runsQuery.isPending ? (
            <div className="grid gap-1 rounded-xl bg-muted p-2" aria-label="Loading runs…">
              {[0, 1, 2].map((n) => (
                <div key={n} className="flex h-12 items-center gap-3 px-2">
                  <Bar className="size-4 rounded-xs bg-background" />
                  <Bar className="h-4 w-48 bg-background" />
                  <Bar className="ml-auto h-4 w-16 bg-background" />
                </div>
              ))}
            </div>
          ) : runsQuery.isError ? (
            <ErrorBlock message={runsQuery.error.message} onRetry={() => runsQuery.refetch()} />
          ) : runs.length === 0 ? (
            <p className="rounded-xl bg-muted px-4 py-4 text-sm text-muted-foreground">
              No runs yet.{" "}
              <Link to="/runs/new" className="rounded-xs font-medium text-foreground underline underline-offset-4 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none">
                New run
              </Link>
            </p>
          ) : (
            // A muted box with 8 px padding holds rows with 8 px corners (16 - 8), concentric.
            <div className="max-h-80 overflow-y-auto overscroll-contain rounded-xl bg-muted px-2 pb-2">
              {groups.map((g) => {
                const blocked = lockedSet !== null && lockedSet !== g.promptSetId
                return (
                  <div key={g.promptSetId}>
                    <div className="sticky top-0 z-10 flex items-center gap-2 bg-muted px-2 pt-3 pb-1.5">
                      <Plasma seed={`prompt-set-${g.promptSetId}`} cells={5} className="size-5 rounded-xs" />
                      <p className="min-w-0 truncate text-sm font-medium" title={g.promptSetName}>
                        {g.promptSetName}
                      </p>
                    </div>
                    <ul className="grid gap-0.5">
                      {g.runs.map((run) => {
                        const id = `pref-run-${run.id}`
                        const checked = selected.includes(run.id)
                        const empty = run.done === 0
                        const off = blocked || empty
                        return (
                          <li key={run.id}>
                            <label
                              htmlFor={id}
                              className={cn(
                                "flex min-h-11 items-start gap-3 rounded-sm px-2 py-2 transition-colors duration-150",
                                off ? "cursor-not-allowed opacity-60" : "cursor-pointer hover:bg-accent",
                                checked && "bg-background hover:bg-background dark:bg-accent",
                              )}
                            >
                              <Checkbox
                                id={id}
                                checked={checked}
                                disabled={off}
                                onCheckedChange={(v) => toggle(run, v === true)}
                                className="mt-0.5"
                              />
                              <span className="grid min-w-0 flex-1 gap-0.5">
                                <span className="text-sm break-words">{run.name}</span>
                                <span className="font-mono text-xs text-muted-foreground [overflow-wrap:anywhere]">{run.model}</span>
                              </span>
                              <span className="grid flex-none justify-items-end gap-0.5 text-right">
                                {empty ? (
                                  <span className="text-sm text-muted-foreground">No finished samples</span>
                                ) : (
                                  <span className="text-sm tabular">{int(run.done)} done</span>
                                )}
                                {empty ? null : run.status === "completed" ? (
                                  <span className="text-xs text-muted-foreground tabular">
                                    {count(run.params.samples_per_prompt, "sample", "samples")} per prompt
                                  </span>
                                ) : (
                                  <RunStatusPill status={run.status} />
                                )}
                              </span>
                            </label>
                          </li>
                        )
                      })}
                    </ul>
                  </div>
                )
              })}
            </div>
          )}
          {errors.runs ? (
            <p id="pref-runs-error" role="alert" className="px-3 text-sm text-destructive">
              {errors.runs}
            </p>
          ) : (
            <p id="pref-runs-hint" className="px-3 text-xs text-muted-foreground">
              {selectedRuns.length
                ? `${count(selectedRuns.length, "run", "runs")} on ${lockedSetName} selected.`
                : "Only runs on the same prompt set can be combined."}
            </p>
          )}
        </fieldset>

        {errors.form ? <ErrorBlock message={errors.form} /> : null}
      </div>

      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={create.isPending} aria-disabled={create.isPending || undefined}>
          {create.isPending ? "Creating…" : "Create preference set"}
        </Button>
      </DialogFooter>
    </form>
  )
}
