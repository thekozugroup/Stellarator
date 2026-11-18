import { Link } from "react-router"
import { useQuery } from "@tanstack/react-query"
import { Plus, Upload } from "lucide-react"
import { buttonVariants } from "@/components/ui/button"
import { Page } from "@/components/shared/page"
import { PhoneToolbarAction } from "@/components/shared/phone-chrome"
import { EmptyBlock, ErrorBlock } from "@/components/shared/block-states"
import { PromptSetTable, PromptSetTableSkeleton } from "@/components/prompts/prompt-set-table"
import { importingInterval } from "@/components/prompts/set-status"
import { api } from "@/lib/api"
import { TOOLBAR_QUERY, useMediaQuery } from "@/lib/media"
import type { PromptSet } from "@/lib/types"

export function PromptSetsPage() {
  const query = useQuery({ queryKey: ["prompt-sets"], queryFn: () => api.get<PromptSet[]>("/api/prompt-sets"),
    refetchInterval: (q) => importingInterval(q.state.data),
  })
  const sets = query.data ?? []
  const phone = !useMediaQuery(TOOLBAR_QUERY)
  const importLink = (
    <Link to="/prompts/import" className={buttonVariants()}>
      <Upload aria-hidden="true" />
      Import prompts
    </Link>
  )
  const hasSets = !!query.data && sets.length > 0
  return (
    <Page title="Prompt sets" actions={(query.data && sets.length === 0) || phone ? null : importLink}>
      {hasSets ? (
        <PhoneToolbarAction>
          <Link to="/prompts/import" aria-label="Import prompts" title="Import prompts" className={buttonVariants({ variant: "outline", size: "icon" })}>
            <Plus aria-hidden="true" />
          </Link>
        </PhoneToolbarAction>
      ) : null}
      {query.isPending ? (
        <PromptSetTableSkeleton />
      ) : query.isError ? (
        <ErrorBlock message={`Couldn’t load prompt sets. ${query.error.message}`} onRetry={() => query.refetch()} />
      ) : sets.length === 0 ? (
        <div className="border-y">
          <EmptyBlock action={importLink}>
            No prompt sets yet. Import from Hugging Face, a file or pasted text.
          </EmptyBlock>
        </div>
      ) : (
        <PromptSetTable sets={sets} />
      )}
    </Page>
  )
}
