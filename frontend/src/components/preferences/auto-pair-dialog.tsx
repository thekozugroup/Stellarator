import { useState, type FormEvent } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { ErrorBlock } from "@/components/shared/block-states"
import { Field } from "@/components/shared/field"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { api, qs } from "@/lib/api"
import { count } from "@/lib/format"
import type { Page, PreferenceItem, PreferenceSet } from "@/lib/types"
import { cn } from "@/lib/utils"
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
      <DialogContent aria-describedby={undefined} className="sm:max-w-md">
        {open ? <AutoPairForm set={set} onDone={() => onOpenChange(false)} /> : null}
      </DialogContent>
    </Dialog>
  )
}

/** How many decided prompts of this set were picked by hand: what "replace" would overwrite. */
function useHandPicked(set: PreferenceSet) {
  return useQuery({
    queryKey: [...prefKeys.itemsOf(set.id), "hand-picked", set.decided],
    enabled: set.decided > 0,
    queryFn: async () => {
      let manual = 0
      for (let offset = 0; ; offset += 200) {
        const page = await api.get<Page<PreferenceItem>>(
          `/api/preference-sets/${set.id}/items${qs({ filter: "decided", offset, limit: 200 })}`,
        )
        manual += page.items.filter((it) => it.source === "manual").length
        if (offset + 200 >= page.total || page.items.length === 0) return manual
      }
    },
  })
}

function runLabel(run: PreferenceSet["runs"][number]) {
  return `${run.name} (${shortModel(run.model)})`
}

function AutoPairForm({ set, onDone }: { set: PreferenceSet; onDone: () => void }) {
  const qc = useQueryClient()
  const [chosen, setChosen] = useState(String(set.runs[0]?.id ?? ""))
  const [rejected, setRejected] = useState(String(set.runs[1]?.id ?? ""))
  const [overwrite, setOverwrite] = useState(false)
  const handPicked = useHandPicked(set)
  const manual = set.decided > 0 ? handPicked.data : 0

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
        overwrite: overwrite && manual !== 0,
      }),
    onSuccess: (out) => {
      qc.invalidateQueries({ queryKey: prefKeys.list })
      qc.invalidateQueries({ queryKey: prefKeys.itemsOf(set.id) })
      if (out.created === 0) {
        toast("No new pairs")
      } else {
        const kept = out.skipped ? ` Kept ${count(out.skipped, "pair", "pairs")} picked by hand.` : ""
        toast.success(`Paired ${count(out.created, "prompt", "prompts")}.${kept}`)
      }
      onDone()
    },
  })

  function submit(e: FormEvent) {
    e.preventDefault()
    if (!run.isPending) run.mutate()
  }

  return (
    <form onSubmit={submit} noValidate className="grid gap-4">
      <DialogHeader>
        <DialogTitle>Auto-pair by run</DialogTitle>
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

      {manual === 0 ? null : (
        <div className="flex items-start gap-3">
          <Checkbox
            id="auto-overwrite"
            checked={overwrite}
            onCheckedChange={(v) => setOverwrite(v === true)}
            aria-describedby="auto-overwrite-hint"
            className={cn("mt-0.5", overwrite && "border-destructive data-[state=checked]:bg-destructive data-[state=checked]:border-destructive")}
          />
          <div className="grid gap-1">
            <Label htmlFor="auto-overwrite" className={cn(overwrite && "text-destructive")}>
              Replace {manual === undefined ? "pairs" : count(manual, "pair", "pairs")} picked by hand
            </Label>
            <p id="auto-overwrite-hint" className="text-xs text-muted-foreground">
              Earlier auto-pairs are always replaced.
            </p>
          </div>
        </div>
      )}

      {run.isError ? <ErrorBlock message={run.error.message} /> : null}

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" variant={overwrite && manual ? "destructive" : "default"} disabled={run.isPending}>
          {run.isPending ? "Pairing…" : overwrite && manual ? `Replace ${count(manual, "pair", "pairs")}` : "Auto-pair prompts"}
        </Button>
      </DialogFooter>
    </form>
  )
}
