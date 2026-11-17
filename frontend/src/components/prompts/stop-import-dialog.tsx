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
import type { PromptSet } from "@/lib/types"

/** Stopping an import deletes the partial set. */
export function StopImportDialog({ set, open, onOpenChange }: { set: PromptSet; open: boolean; onOpenChange: (o: boolean) => void }) {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const [error, setError] = useState<string | null>(null)
  const stop = useMutation({
    mutationFn: () => api.del(`/api/prompt-sets/${set.id}`),
    onSuccess: () => {
      qc.removeQueries({ queryKey: ["prompt-set", set.id] })
      qc.removeQueries({ queryKey: ["prompts", set.id] })
      qc.invalidateQueries({ queryKey: ["prompt-sets"] })
      toast.success(`Stopped the import of “${set.name}”.`)
      navigate("/prompts")
    },
    onError: (e: Error) => setError(e.message),
  })
  return (
    <AlertDialog
      open={open}
      onOpenChange={(o) => {
        if (stop.isPending) return
        if (!o) setError(null)
        onOpenChange(o)
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="break-words">Stop importing “{set.name}”?</AlertDialogTitle>
          <AlertDialogDescription>The prompts read so far are deleted with it.</AlertDialogDescription>
        </AlertDialogHeader>
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={stop.isPending}>Keep importing</AlertDialogCancel>
          <Button variant="destructive" disabled={stop.isPending} onClick={() => stop.mutate()}>
            {stop.isPending ? "Stopping…" : "Stop and delete"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
