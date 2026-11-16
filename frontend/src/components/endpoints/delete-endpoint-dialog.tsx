import { useState } from "react"
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
import type { Endpoint } from "@/lib/types"

export function DeleteEndpointDialog({ endpoint, onClose }: { endpoint: Endpoint | null; onClose: () => void }) {
  const qc = useQueryClient()
  const [error, setError] = useState<string | null>(null)
  const del = useMutation({
    mutationFn: (id: number) => api.del(`/api/endpoints/${id}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["endpoints"] })
      toast.success(`Deleted ${endpoint?.name ?? "endpoint"}.`)
      onClose()
    },
    onError: (e: Error) => setError(e.message),
  })
  return (
    <AlertDialog
      open={endpoint !== null}
      onOpenChange={(open) => {
        if (!open && !del.isPending) {
          setError(null)
          onClose()
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className="break-words">Delete “{endpoint?.name}”?</AlertDialogTitle>
          <AlertDialogDescription>
            {endpoint && endpoint.run_count > 0
              ? `${count(endpoint.run_count, "run", "runs")} used this endpoint. They keep their data but can’t resume.`
              : "No runs use this endpoint. You can add it again later."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {error ? (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={del.isPending}>Cancel</AlertDialogCancel>
          <Button variant="destructive" disabled={del.isPending} onClick={() => endpoint && del.mutate(endpoint.id)}>
            {del.isPending ? "Deleting…" : "Delete endpoint"}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
