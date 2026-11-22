/** Queries, keys and wording shared by the run screens. */
import { keepPreviousData, useQuery } from "@tanstack/react-query"
import { isKeyRejected } from "@/components/endpoints/endpoint-status"
import { api, qs } from "@/lib/api"
import type { Endpoint, Page, PromptSet, Run, RunLive, Sample, SampleSummary, Settings } from "@/lib/types"

export const runKeys = {
  list: ["runs"] as const,
  detail: (id: number) => ["runs", id] as const,
  live: (id: number) => ["runs", id, "live"] as const,
  samples: (id: number) => ["runs", id, "samples"] as const,
  samplePage: (id: number, params: SampleQuery) => ["runs", id, "samples", params] as const,
  sample: (id: number) => ["samples", id] as const,
}

export const isActive = (status: Run["status"] | undefined) => status === "running" || status === "pausing"

export function useRuns() {
  return useQuery({
    queryKey: runKeys.list,
    queryFn: () => api.get<Run[]>("/api/runs"),
    refetchInterval: (q) => (q.state.data?.some((r) => isActive(r.status)) ? 2000 : false),
  })
}

export function useEndpoints(enabled = true) {
  return useQuery({ queryKey: ["endpoints"], queryFn: () => api.get<Endpoint[]>("/api/endpoints"), enabled })
}

export function usePromptSets(enabled = true) {
  return useQuery({ queryKey: ["prompt-sets"], queryFn: () => api.get<PromptSet[]>("/api/prompt-sets"), enabled })
}

export function useSettings(enabled = true) {
  return useQuery({ queryKey: ["settings"], queryFn: () => api.get<Settings>("/api/settings"), enabled })
}

/** The run screen's one poll: every second while it runs, every 10 s while paused, never once finished. */
export function useRunLive(id: number) {
  return useQuery({
    queryKey: runKeys.live(id),
    queryFn: () => api.get<RunLive>(`/api/runs/${id}/live`),
    refetchInterval: (q) => {
      const status = q.state.data?.run.status
      if (isActive(status)) return 1000
      if (status === "paused") return 10_000
      return false
    },
    refetchIntervalInBackground: false,
  })
}

export type SampleFilter = "all" | "done" | "failed" | "truncated" | "pending"
export const SAMPLE_FILTERS: SampleFilter[] = ["all", "done", "failed", "truncated", "pending"]
export const PAGE_SIZE = 50

export interface SampleQuery {
  status: SampleFilter
  q: string
  page: number
}

export function useSamplePage(runId: number, params: SampleQuery, live: boolean) {
  return useQuery({
    queryKey: runKeys.samplePage(runId, params),
    queryFn: () =>
      api.get<Page<SampleSummary>>(
        `/api/runs/${runId}/samples${qs({
          status: params.status === "all" ? null : params.status,
          q: params.q || null,
          offset: (params.page - 1) * PAGE_SIZE,
          limit: PAGE_SIZE,
        })}`,
      ),
    placeholderData: keepPreviousData,
    refetchInterval: live ? 3000 : false,
  })
}

export function useSample(id: number | null) {
  return useQuery({
    queryKey: runKeys.sample(id ?? 0),
    queryFn: () => api.get<Sample>(`/api/samples/${id}`),
    enabled: id !== null,
  })
}

/** "#12", or "#12.2" when the run takes several samples per prompt. Prompt numbers are 1-based. */
export function sampleLabel(promptIdx: number, sampleIdx: number, samplesPerPrompt: number) {
  return samplesPerPrompt > 1 ? `#${promptIdx + 1}.${sampleIdx + 1}` : `#${promptIdx + 1}`
}

/** The finish reason in words. Truncation is explained, never shown as a bare "length". */
export function finishReasonText(reason: string | null | undefined): { text: string; warn: boolean } | null {
  if (!reason) return null
  if (reason === "length") return { text: "Truncated (max tokens)", warn: true }
  if (reason === "stop") return { text: "Stop", warn: false }
  if (reason === "content_filter") return { text: "Stopped by the provider's content filter.", warn: true }
  if (reason === "tool_calls") return { text: "Stopped to call a tool.", warn: false }
  return { text: `Stopped with reason “${reason}”.`, warn: false }
}

export const ROLE_LABEL: Record<string, string> = { system: "System", user: "User", assistant: "Assistant" }

/** "Key rejected" or "Unreachable" when the endpoint's last check failed; null when it passed or never ran. */
export function endpointTrouble(ep: Endpoint | undefined): "Key rejected" | "Unreachable" | null {
  if (!ep || ep.last_ok !== false) return null
  return isKeyRejected(ep.last_error) ? "Key rejected" : "Unreachable"
}

/**
 * The endpoints screen with one endpoint's edit form open, focused on the field that needs fixing
 * (the API key, or the URL when the server was unreachable); Save or Cancel comes back to `returnTo`.
 */
export function editEndpointHref(id: number, returnTo: string, focus: "key" | "url" | null = "key"): string {
  return `/endpoints?edit=${id}${focus ? `&focus=${focus}` : ""}&return=${encodeURIComponent(returnTo).replace(/%2F/g, "/")}`
}

/** Which edit field fixes this trouble. */
export function troubleField(trouble: "Key rejected" | "Unreachable" | null): "key" | "url" | null {
  return trouble === "Key rejected" ? "key" : trouble === "Unreachable" ? "url" : null
}

/** Common seqs values, offered as one segmented row on New run and on a run. */
export const SEQS_CHOICES = [1, 4, 8, 16, 32, 64, 128, 256]

/**
 * A stacked row below 1024 px: straight hairlines between rows, and the highlight (the selected
 * row, or the row whose link has keyboard focus) a --accent shape with 10 px corners that reaches
 * 12 px past the text on both sides, as table rows do. The row's link covers the same shape.
 */
export const STACKED_ROW =
  "relative isolate py-3 before:pointer-events-none before:absolute before:inset-y-0 before:-inset-x-3 before:-z-10 before:rounded-md before:bg-accent before:opacity-0 has-[a:focus-visible]:before:opacity-100"
export const STACKED_LINK =
  "outline-none after:absolute after:inset-y-0 after:-inset-x-3 after:rounded-md after:content-[''] focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-ring"
