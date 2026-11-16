import { useState } from "react"
import { useNavigate } from "react-router"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { api } from "@/lib/api"
import { count } from "@/lib/format"
import type { PromptSet } from "@/lib/types"

export function DeletePromptSetDialog({ set, open, onOpenChange }: { set: PromptSet; open: boolean; onOpenChange: (o: boolean) => void }) {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const [error, setError] = useState<string | null>(null)
  const del = useMutation({
    mutationFn: () => api.del(`/api/prompt-sets/${set.id}`),
    onSuccess: () => {
      qc.removeQueries({ queryKey: ["prompt-set", set.id] })
      qc.invalidateQueries({ queryKey: ["prompt-sets"] })
      qc.invalidateQueries({ queryKey: ["runs"] })
      toast.success(`Deleted “${set.name}”.`)
      navigate("/prompts")
    },
    onError: (e: Error) => setError(e.message),
  })
  return (
    <AlertDialog
      open={open}
      onOpenChange={(o) => {
        if (del.isPending) return
        if (!o) setError(null)
        onOpenChange(o)
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="break-words">Delete “{set.name}”?</AlertDialogTitle>
          <AlertDialogDescription>
            Its {count(set.prompt_count, "prompt", "prompts")} and {count(set.run_count, "run", "runs")} (with their samples) are deleted too.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={del.isPending}>Cancel</AlertDialogCancel>
          <Button variant="destructive" disabled={del.isPending} onClick={() => del.mutate()}>
            {del.isPending ? "Deleting…" : "Delete prompt set"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
