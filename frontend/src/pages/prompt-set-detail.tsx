import { TextModeToggle } from "@/components/shared/model-text"
import { useEffect, useRef, useState } from "react"
import { Link, useParams, useSearchParams } from "react-router"
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import { ChevronLeft, ChevronRight, MoreHorizontal, Search } from "lucide-react"
import { Button, buttonVariants } from "@/components/ui/button"
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { Page } from "@/components/shared/page"
import { DeletePromptSetDialog } from "@/components/prompts/delete-prompt-set-dialog"
import { PromptRow, PromptRowsSkeleton } from "@/components/prompts/prompt-row"
import { RenamePromptSetDialog } from "@/components/prompts/rename-prompt-set-dialog"
import { describeSource } from "@/components/prompts/source"
import { api, qs } from "@/lib/api"
import { count, int } from "@/lib/format"
import type { Page as PageOf, Prompt, PromptSet } from "@/lib/types"

const PAGE_SIZE = 50

export function PromptSetDetailPage() {
  const { setId } = useParams()
  const id = Number(setId)
  const [params, setParams] = useSearchParams()
  const q = params.get("q") ?? ""
  const page = Math.max(1, Number(params.get("page")) || 1)
  const [draft, setDraft] = useState(q)
  const [renaming, setRenaming] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const listTop = useRef<HTMLDivElement>(null)

  const setQuery = useQuery({ // taste-ignore: missing-states -- loading renders Bar placeholders for the title, meta and rows (lines 90-98, PromptRowsSkeleton)
    queryKey: ["prompt-set", id],
    queryFn: () => api.get<PromptSet>(`/api/prompt-sets/${id}`),
    enabled: Number.isFinite(id),
  })
  const prompts = useQuery({
    queryKey: ["prompts", id, q, page],
    queryFn: () => api.get<PageOf<Prompt>>(`/api/prompt-sets/${id}/prompts${qs({ offset: (page - 1) * PAGE_SIZE, limit: PAGE_SIZE, q })}`),
    placeholderData: keepPreviousData,
    enabled: Number.isFinite(id),
  })

  // Debounce the search box into the URL, and restart at page 1.
  useEffect(() => {
    if (draft === q) return
    const t = window.setTimeout(() => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev)
          if (draft.trim()) next.set("q", draft.trim())
          else next.delete("q")
          next.delete("page")
          return next
        },
        { replace: true },
      )
    }, 300)
    return () => window.clearTimeout(t)
  }, [draft, q, setParams])

  const goToPage = (p: number) => {
    setParams((prev) => {
      const next = new URLSearchParams(prev)
      if (p <= 1) next.delete("page")
      else next.set("page", String(p))
      return next
    })
    listTop.current?.scrollIntoView({ block: "start" })
  }

  const set = setQuery.data
  const data = prompts.data
  const total = data?.total ?? 0
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE))
  const first = total ? (page - 1) * PAGE_SIZE + 1 : 0
  const last = Math.min(total, page * PAGE_SIZE)

  if (setQuery.isError) {
    return (
      <Page title="Prompt set" back={{ to: "/prompts", label: "Prompt sets" }}>
        <ErrorBlock message={setQuery.error.message} onRetry={() => setQuery.refetch()} />
      </Page>
    )
  }

  return (
    <Page
      back={{ to: "/prompts", label: "Prompt sets" }}
      title={set ? set.name : <Bar className="h-7 w-64 max-w-full align-middle" />}
      meta={
        set ? (
          <span>
            {describeSource(set)} · <span className="tabular">{count(set.prompt_count, "prompt", "prompts")}</span> ·{" "}
            <span className="tabular">{count(set.run_count, "run", "runs")}</span>
          </span>
        ) : (
          <Bar className="w-72 max-w-full" />
        )
      }
      actions={
        set ? (
          <>
            <Link to={`/runs/new?prompt_set=${set.id}`} className={buttonVariants()}>
              New run
            </Link>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="outline" size="icon" aria-label="More actions">
                  <MoreHorizontal aria-hidden="true" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-44">
                <DropdownMenuItem onSelect={() => setRenaming(true)}>Rename…</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem variant="destructive" onSelect={() => setDeleting(true)}>
                  Delete…
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </>
        ) : null
      }
    >
      <div ref={listTop} className="mb-3 flex scroll-mt-4 flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <div className="relative w-full sm:w-80">
          <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-muted-foreground" aria-hidden="true" />
          <Input
            type="search"
            aria-label="Search prompts"
            placeholder="Search prompts…"
            autoComplete="off"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            className="pl-8"
          />
        </div>
        <div className="flex flex-wrap items-center gap-3">
          {total > PAGE_SIZE || (q && total > 0) ? (
            <p className="text-sm text-muted-foreground tabular" aria-live="polite">
              Showing {int(first)}–{int(last)} of {int(total)}
            </p>
          ) : null}
          <TextModeToggle />
        </div>
      </div>

      {prompts.isError && !data ? (
        <ErrorBlock message={`Couldn’t load prompts. ${prompts.error.message}`} onRetry={() => prompts.refetch()} />
      ) : !data ? (
        <PromptRowsSkeleton />
      ) : data.items.length === 0 ? (
        <div className="border-y py-6 text-sm text-muted-foreground">
          {q ? (
            <p>
              No prompt contains “{q}”.{" "}
              <button
                type="button"
                className="rounded-sm text-foreground underline underline-offset-4 focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
                onClick={() => setDraft("")}
              >
                Clear search
              </button>
            </p>
          ) : total > 0 ? (
            <p>
              There is no page {int(page)}.{" "}
              <button
                type="button"
                className="rounded-sm text-foreground underline underline-offset-4 focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
                onClick={() => goToPage(1)}
              >
                Go to page 1
              </button>
            </p>
          ) : (
            <p>This prompt set has no prompts.</p>
          )}
        </div>
      ) : (
        <ul className={`divide-y border-y transition-opacity duration-150 ${prompts.isFetching ? "opacity-60" : ""}`}>
          {data.items.map((p) => (
            <PromptRow key={p.id} prompt={p} />
          ))}
        </ul>
      )}

      {data && total > PAGE_SIZE ? (
        <nav aria-label="Pages" className="mt-4 flex items-center justify-between gap-3">
          <Button variant="outline" disabled={page <= 1} onClick={() => goToPage(page - 1)}>
            <ChevronLeft aria-hidden="true" />
            Previous
          </Button>
          <span className="text-sm text-muted-foreground tabular">
            Page {int(Math.min(page, pages))} of {int(pages)}
          </span>
          <Button variant="outline" disabled={page >= pages} onClick={() => goToPage(page + 1)}>
            Next
            <ChevronRight aria-hidden="true" />
          </Button>
        </nav>
      ) : null}

      {set ? (
        <>
          <RenamePromptSetDialog set={set} open={renaming} onOpenChange={setRenaming} />
          <DeletePromptSetDialog set={set} open={deleting} onOpenChange={setDeleting} />
        </>
      ) : null}
    </Page>
  )
}
