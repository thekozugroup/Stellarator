import { useState, type FormEvent } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { ErrorBlock } from "@/components/shared/block-states"
import { Field } from "@/components/shared/field"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { api } from "@/lib/api"
import { count } from "@/lib/format"
import type { PreferenceSet } from "@/lib/types"
import { prefKeys, shortModel } from "./queries"

interface AutoPairOut {
  created: number
  skipped: number
}

export function AutoPairDialog({
  set,
  open,
  onOpenChange,
}: {
  set: PreferenceSet
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        {open ? <AutoPairForm set={set} onDone={() => onOpenChange(false)} /> : null}
      </DialogContent>
    </Dialog>
  )
}

function runLabel(run: PreferenceSet["runs"][number]) {
  return `${run.name} (${shortModel(run.model)})`
}

function AutoPairForm({ set, onDone }: { set: PreferenceSet; onDone: () => void }) {
  const qc = useQueryClient()
  const [chosen, setChosen] = useState(String(set.runs[0]?.id ?? ""))
  const [rejected, setRejected] = useState(String(set.runs[1]?.id ?? ""))
  const [overwrite, setOverwrite] = useState(false)

  const pick = (which: "chosen" | "rejected", value: string) => {
    // Picking the run already on the other side swaps the two.
    if (which === "chosen") {
      if (value === rejected) setRejected(chosen)
      setChosen(value)
    } else {
      if (value === chosen) setChosen(rejected)
      setRejected(value)
    }
  }

  const run = useMutation({
    mutationFn: () =>
      api.post<AutoPairOut>(`/api/preference-sets/${set.id}/auto-pair`, {
        chosen_run_id: Number(chosen),
        rejected_run_id: Number(rejected),
        overwrite,
      }),
    onSuccess: (out) => {
      qc.invalidateQueries({ queryKey: prefKeys.list })
      qc.invalidateQueries({ queryKey: prefKeys.itemsOf(set.id) })
      const parts = [
        out.created ? `Paired ${count(out.created, "prompt", "prompts")}.` : "No prompt was paired.",
        out.skipped ? `Kept ${count(out.skipped, "pair", "pairs")} you picked by hand.` : "",
        !out.created && !out.skipped ? "No prompt has a finished response from both runs." : "",
      ]
      toast.success(parts.filter(Boolean).join(" "))
      onDone()
    },
  })

  function submit(e: FormEvent) {
    e.preventDefault()
    if (!run.isPending) run.mutate()
  }

  const chosenRun = set.runs.find((r) => String(r.id) === chosen)
  const rejectedRun = set.runs.find((r) => String(r.id) === rejected)

  return (
    <form onSubmit={submit} noValidate className="grid gap-4">
      <DialogHeader>
        <DialogTitle>Auto-pair by run</DialogTitle>
        <DialogDescription>
          For every prompt both runs answered, the first finished response of one run becomes chosen and the
          other run's becomes rejected.
        </DialogDescription>
      </DialogHeader>

      <Field id="auto-chosen" label="Chosen run">
        <Select value={chosen} onValueChange={(v) => pick("chosen", v)}>
          <SelectTrigger id="auto-chosen" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {set.runs.map((r) => (
              <SelectItem key={r.id} value={String(r.id)}>
                {runLabel(r)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <Field id="auto-rejected" label="Rejected run">
        <Select value={rejected} onValueChange={(v) => pick("rejected", v)}>
          <SelectTrigger id="auto-rejected" className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent position="popper">
            {set.runs.map((r) => (
              <SelectItem key={r.id} value={String(r.id)}>
                {runLabel(r)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <div className="flex items-start gap-3">
        <Checkbox
          id="auto-overwrite"
          checked={overwrite}
          onCheckedChange={(v) => setOverwrite(v === true)}
          aria-describedby="auto-overwrite-hint"
          className="mt-0.5"
        />
        <div className="grid gap-1">
          <Label htmlFor="auto-overwrite">Replace pairs picked by hand</Label>
          <p id="auto-overwrite-hint" className="text-xs text-muted-foreground">
            Off: prompts you already decided keep your choice. Pairs made by an earlier auto-pair are always
            replaced.
          </p>
        </div>
      </div>

      {chosenRun && rejectedRun ? (
        <p className="text-sm text-muted-foreground">
          Prefers <span className="text-foreground">{chosenRun.name}</span> over{" "}
          <span className="text-foreground">{rejectedRun.name}</span> on every prompt both answered.
        </p>
      ) : null}

      {run.isError ? <ErrorBlock message={run.error.message} /> : null}

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={run.isPending}>
          {run.isPending ? "Pairing…" : "Auto-pair prompts"}
        </Button>
      </DialogFooter>
    </form>
  )
}
