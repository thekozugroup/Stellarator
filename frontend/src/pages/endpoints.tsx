import { useEffect, useRef, useState } from "react"
import { useNavigate, useSearchParams } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { Plus } from "lucide-react"
import { toast } from "sonner"
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
  const [params, setParams] = useSearchParams()
  const navigate = useNavigate()
  const handledEdit = useRef<string | null>(null)
  const details = endpoints.find((e) => e.id === detailsId) ?? null

  // /endpoints?edit=<id> opens that endpoint's edit dialog (linked from the "key rejected" banner on a run).
  const editParam = params.get("edit")
  const focusParam = params.get("focus")
  const focusField = focusParam === "key" ? "api_key" : focusParam === "url" ? "base_url" : null
  const returnParam = params.get("return")
  const returnTo = returnParam && returnParam.startsWith("/") && !returnParam.startsWith("//") ? returnParam : null
  const clearEdit = () =>
    setParams(
      (prev) => {
        const next = new URLSearchParams(prev)
        next.delete("edit")
        next.delete("focus")
        return next
      },
      { replace: true },
    )
  useEffect(() => {
    if (!editParam || !query.data || handledEdit.current === editParam) return
    handledEdit.current = editParam
    const target = query.data.find((e) => String(e.id) === editParam)
    if (target) setForm({ endpoint: target })
    else {
      toast.error("That endpoint no longer exists.")
      clearEdit()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editParam, query.data])

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
            No endpoints yet. Any OpenAI-compatible server: vLLM, SGLang, llama.cpp, Ollama, LM Studio or a hosted API.
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

      <EndpointFormDialog open={form !== null} endpoint={form?.endpoint ?? null} onOpenChange={(o) => {
          if (o) return
          setForm(null)
          if (returnTo && editParam) navigate(returnTo)
          else if (editParam) clearEdit()
        }}
        focusField={editParam ? focusField : null}
      />
      <EndpointDetailsSheet
        endpoint={details}
        onClose={() => setDetailsId(null)}
        onEdit={(e) => {
          setDetailsId(null)
          setForm({ endpoint: e })
        }}
      />
      <DeleteEndpointDialog endpoint={deleting} onClose={() => setDeleting(null)} />
    </Page>
  )
}
