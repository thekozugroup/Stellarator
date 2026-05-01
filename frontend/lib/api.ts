import {
  Paper,
  Run,
  RunAlertList,
  RunListResponse,
  ResearchTranscriptList,
  RunMetric,
  RunNote,
  StatsSummary,
} from "./types";
import { z } from "zod";

const API_URL =
  (process.env.NEXT_PUBLIC_API_URL || "").replace(/\/$/, "");

function viewerToken(): string | undefined {
  if (typeof window !== "undefined") {
    const local = window.localStorage.getItem("stellarator.viewerToken");
    if (local) return local;
  }
  return process.env.NEXT_PUBLIC_VIEWER_TOKEN;
}

export class ApiError extends Error {
  constructor(public status: number, public body: string, message?: string) {
    super(message || `HTTP ${status}`);
  }
}

async function request<T>(
  path: string,
  schema: z.ZodType<T>,
  init: RequestInit = {},
): Promise<T> {
  const token = viewerToken();
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (token) headers.set("Authorization", `Bearer ${token}`);

  const res = await fetch(`${API_URL}${path}`, { ...init, headers, cache: "no-store" });
  const text = await res.text();
  if (!res.ok) throw new ApiError(res.status, text, `Request failed: ${path}`);
  if (!text) return undefined as unknown as T;
  const json = JSON.parse(text);
  return schema.parse(json);
}

export const api = {
  apiUrl: () => API_URL,
  viewerToken,

  listRuns: (params: { status?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    if (params.status) q.set("status", params.status);
    if (params.limit) q.set("limit", String(params.limit));
    const qs = q.toString();
    return request(`/v1/runs${qs ? `?${qs}` : ""}`, RunListResponse);
  },

  getRun: (id: string) => request(`/v1/runs/${id}`, Run),

  getRunMetrics: (id: string, since?: number) => {
    const qs = since != null ? `?since=${since}` : "";
    return request(`/v1/runs/${id}/metrics${qs}`, z.object({ metrics: z.array(RunMetric) }));
  },

  getRunNotes: (id: string) =>
    request(`/v1/runs/${id}/notes`, z.object({ notes: z.array(RunNote) })),

  cancelRun: (id: string) =>
    request(`/v1/runs/${id}/cancel`, z.object({ ok: z.boolean() }), { method: "POST" }),
  pauseRun: (id: string) =>
    request(`/v1/runs/${id}/pause`, z.object({ ok: z.boolean() }), { method: "POST" }),
  resumeRun: (id: string) =>
    request(`/v1/runs/${id}/resume`, z.object({ ok: z.boolean() }), { method: "POST" }),

  stats: () => request(`/v1/stats/summary`, StatsSummary),

  searchPapers: async (q: string, source: "huggingface" | "arxiv" | "both" = "both") => {
    const r = await request(
      `/v1/research/papers/search?q=${encodeURIComponent(q)}&source=${source}`,
      z.object({ results: z.array(Paper) }),
    );
    return { results: r.results.map((p) => ({ ...p, authors: p.authors ?? [] })) };
  },

  citeToRun: (runId: string, paperId: string) =>
    request(
      `/v1/research/runs/${runId}/cite`,
      z.object({ ok: z.boolean() }),
      { method: "POST", body: JSON.stringify({ paper_id: paperId }) },
    ),

  codexOAuthStart: () =>
    request(`/v1/oauth/codex/start`, z.object({ url: z.string() }), { method: "POST" }),

  getRunAlerts: (id: string, since?: string) => {
    const qs = since ? `?since=${encodeURIComponent(since)}` : "";
    return request(`/v1/runs/${id}/alerts${qs}`, RunAlertList);
  },

  getResearchTranscripts: (params: { agent?: string; run_id?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    if (params.agent) q.set("agent", params.agent);
    if (params.run_id) q.set("run_id", params.run_id);
    if (params.limit) q.set("limit", String(params.limit));
    const qs = q.toString();
    return request(`/v1/research/transcripts${qs ? `?${qs}` : ""}`, ResearchTranscriptList);
  },

  // Cost / budgets
  listBudgets: () =>
    request(`/v1/cost/budgets`, z.object({ budgets: z.array(z.object({
      id: z.string(),
      scope: z.enum(["agent", "run"]),
      scope_id: z.string().optional(),
      monthly_limit_usd: z.number(),
      daily_limit_usd: z.number().optional(),
      alert_threshold_pct: z.number(),
    })) })),

  createBudget: (body: {
    scope: "agent" | "run";
    scope_id?: string;
    monthly_limit_usd: number;
    daily_limit_usd?: number;
    alert_threshold_pct: number;
  }) =>
    request(`/v1/cost/budgets`, z.object({ id: z.string() }), {
      method: "POST",
      body: JSON.stringify(body),
    }),

  deleteBudget: (id: string) =>
    request(`/v1/cost/budgets/${id}`, z.object({ ok: z.boolean() }), { method: "DELETE" }),

  getCostSummary: () =>
    request(`/v1/cost/summary`, z.object({
      total_usd: z.number(),
      by_scope: z.array(z.object({
        scope: z.string(),
        scope_id: z.string().optional(),
        spend_usd: z.number(),
      })).optional(),
    })),
};

export function wsUrl(runId: string): string {
  const base = API_URL.replace(/^http/, "ws");
  const token = viewerToken();
  const qs = token ? `?token=${encodeURIComponent(token)}` : "";
  return `${base}/v1/runs/${runId}/stream${qs}`;
}
