import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Loader2, RefreshCw } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { api } from "@/lib/api"
import { count } from "@/lib/format"
import type { Endpoint } from "@/lib/types"

/** Re-reads the model list. The spinner sits inside the button; the label and width stay. */
export function CheckButton({ endpoint }: { endpoint: Endpoint }) {
  const qc = useQueryClient()
  const check = useMutation({
    mutationFn: () => api.post<Endpoint>(`/api/endpoints/${endpoint.id}/check`),
    onSuccess: (ep) => {
      qc.setQueryData<Endpoint[]>(["endpoints"], (list) => list?.map((e) => (e.id === ep.id ? ep : e)))
      if (ep.last_ok) toast.success(`${ep.name} is reachable with ${count(ep.models.length, "model", "models")}.`)
      else toast.error(`${ep.name} is unreachable.`)
    },
    onError: (e: Error) => toast.error(e.message),
  })
  return (
    <Button variant="outline" size="sm" onClick={() => check.mutate()} disabled={check.isPending} aria-label={`Check ${endpoint.name} now`}>
      {check.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : <RefreshCw aria-hidden="true" />}
      {check.isPending ? "Checking…" : "Check now"}
    </Button>
  )
}
