import { useNavigate } from "react-router"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { ErrorBlock } from "@/components/shared/block-states"
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
import type { PreferenceSet } from "@/lib/types"
import { prefKeys } from "./queries"

export function DeleteSetDialog({
  set,
  open,
  onOpenChange,
}: {
  set: PreferenceSet
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const navigate = useNavigate()
  const qc = useQueryClient()
  const del = useMutation({
    mutationFn: () => api.del(`/api/preference-sets/${set.id}`),
    onSuccess: () => {
      qc.removeQueries({ queryKey: prefKeys.set(set.id) })
      qc.removeQueries({ queryKey: prefKeys.itemsOf(set.id) })
      qc.invalidateQueries({ queryKey: prefKeys.list })
      toast.success(`Deleted ‘${set.name}’`)
      navigate("/preferences")
    },
  })
  return (
    <AlertDialog open={open} onOpenChange={(v) => !del.isPending && onOpenChange(v)}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="break-words">Delete ‘{set.name}’?</AlertDialogTitle>
          <AlertDialogDescription>
            Its {count(set.decided, "pair", "pairs")} are deleted. Runs and responses are kept.
          </AlertDialogDescription>
        </AlertDialogHeader>
        {del.isError ? <ErrorBlock message={del.error.message} /> : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={del.isPending}>Cancel</AlertDialogCancel>
          {/* A plain button so the dialog stays open while the request runs and on failure. */}
          <Button variant="destructive" disabled={del.isPending} onClick={() => del.mutate()}>
            {del.isPending ? "Deleting…" : "Delete preference set"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
