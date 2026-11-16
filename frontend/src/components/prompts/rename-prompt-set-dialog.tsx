import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Field } from "@/components/shared/field"
import { ApiError, api } from "@/lib/api"
import type { PromptSet } from "@/lib/types"

export function RenamePromptSetDialog({ set, open, onOpenChange }: { set: PromptSet; open: boolean; onOpenChange: (o: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <RenameForm set={set} onClose={() => onOpenChange(false)} />
      </DialogContent>
    </Dialog>
  )
}

function RenameForm({ set, onClose }: { set: PromptSet; onClose: () => void }) {
  const qc = useQueryClient()
  const [name, setName] = useState(set.name)
  const [error, setError] = useState<string | null>(null)
  const rename = useMutation({
    mutationFn: (n: string) => api.patch<PromptSet>(`/api/prompt-sets/${set.id}`, { name: n }),
    onSuccess: (ps) => {
      qc.setQueryData(["prompt-set", set.id], ps)
      qc.invalidateQueries({ queryKey: ["prompt-sets"] })
      toast.success("Prompt set renamed.")
      onClose()
    },
    onError: (e: Error) => {
      setError(e instanceof ApiError ? (e.fields.name ?? e.message) : e.message)
      document.getElementById("ps-name")?.focus()
    },
  })
  return (
    <form
      noValidate
      className="grid gap-4"
      onSubmit={(ev) => {
        ev.preventDefault()
        if (rename.isPending) return
        const n = name.trim()
        if (!n) {
          setError("Enter a name for this prompt set.")
          document.getElementById("ps-name")?.focus()
          return
        }
        setError(null)
        rename.mutate(n)
      }}
    >
      <DialogHeader>
        <DialogTitle>Rename prompt set</DialogTitle>
      </DialogHeader>
      <Field id="ps-name" label="Name" error={error}>
        <Input
          id="ps-name"
          value={name}
          autoComplete="off"
          aria-invalid={!!error}
          aria-describedby={error ? "ps-name-error" : undefined}
          onChange={(e) => {
            setName(e.target.value)
            setError(null)
          }}
          onFocus={(e) => e.currentTarget.select()}
        />
      </Field>
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose} disabled={rename.isPending}>
          Cancel
        </Button>
        <Button type="submit" aria-disabled={rename.isPending}>
          {rename.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
          {rename.isPending ? "Saving…" : "Save name"}
        </Button>
      </DialogFooter>
    </form>
  )
}
