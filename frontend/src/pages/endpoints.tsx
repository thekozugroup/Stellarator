import { useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { Plus } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Page } from "@/components/shared/page"
import { EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { DeleteEndpointDialog } from "@/components/endpoints/delete-endpoint-dialog"
import { EndpointDetailsSheet } from "@/components/endpoints/endpoint-details-sheet"
import { EndpointFormDialog } from "@/components/endpoints/endpoint-form-dialog"
import { EndpointList, EndpointListSkeleton } from "@/components/endpoints/endpoint-list"
import { api } from "@/lib/api"
import type { Endpoint } from "@/lib/types"

export function EndpointsPage() {
  const query = useQuery({ queryKey: ["endpoints"], queryFn: () => api.get<Endpoint[]>("/api/endpoints") })
  const [form, setForm] = useState<{ endpoint: Endpoint | null } | null>(null)
  const [detailsId, setDetailsId] = useState<number | null>(null)
  const [deleting, setDeleting] = useState<Endpoint | null>(null)
  const endpoints = query.data ?? []
  const details = endpoints.find((e) => e.id === detailsId) ?? null

  const add = (
    <Button onClick={() => setForm({ endpoint: null })}>
      <Plus aria-hidden="true" />
      Add endpoint
    </Button>
  )

  return (
    <Page title="Endpoints" actions={query.data && endpoints.length === 0 ? null : add}>
      {query.isPending ? (
        <EndpointListSkeleton />
      ) : query.isError ? (
        <ErrorBlock message={`Couldn’t load endpoints. ${query.error.message}`} onRetry={() => query.refetch()} />
      ) : endpoints.length === 0 ? (
        <div className="border-y">
          <EmptyBlock action={add}>
            Add the server you want to collect data from: a vLLM, SGLang, llama.cpp, Ollama or LM Studio server on your network, or a hosted API.
          </EmptyBlock>
        </div>
      ) : (
        <EndpointList
          endpoints={endpoints}
          onOpen={(e) => setDetailsId(e.id)}
          onEdit={(e) => setForm({ endpoint: e })}
          onDelete={setDeleting}
        />
      )}

      <EndpointFormDialog open={form !== null} endpoint={form?.endpoint ?? null} onOpenChange={(o) => !o && setForm(null)} />
      <EndpointDetailsSheet endpoint={details} onClose={() => setDetailsId(null)} />
      <DeleteEndpointDialog endpoint={deleting} onClose={() => setDeleting(null)} />
    </Page>
  )
}
