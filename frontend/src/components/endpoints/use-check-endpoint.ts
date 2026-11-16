import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { api } from "@/lib/api"
import { count } from "@/lib/format"
import type { Endpoint } from "@/lib/types"
import { isKeyRejected } from "./endpoint-status"

/** Re-reads the model list and reports the result in a toast. */
export function useCheckEndpoint(endpoint: Endpoint) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.post<Endpoint>(`/api/endpoints/${endpoint.id}/check`),
    onSuccess: (ep) => {
      qc.setQueryData<Endpoint[]>(["endpoints"], (list) => list?.map((e) => (e.id === ep.id ? ep : e)))
      if (ep.last_ok) toast.success(`${ep.name}: ${count(ep.models.length, "model", "models")}.`)
      else toast.error(`${ep.name}: ${isKeyRejected(ep.last_error) ? "key rejected" : "unreachable"}.`)
    },
    onError: (e: Error) => toast.error(e.message),
  })
}
