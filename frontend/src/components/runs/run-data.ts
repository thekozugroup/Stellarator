/** Queries, keys and wording shared by the run screens. */
import { keepPreviousData, useQuery } from "@tanstack/react-query"
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

/** Common seqs values, offered as one segmented row on New run and on a run. */
export const SEQS_CHOICES = [1, 4, 8, 16, 32, 64, 128, 256]

/** The chosen option in a segmented control: a tone step, full ink and weight, so it reads without color. */
export const SEGMENT_ON = "data-[state=on]:bg-accent data-[state=on]:font-semibold data-[state=on]:text-foreground"
