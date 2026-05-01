import { z } from "zod";

export const RunStatus = z.enum([
  "queued",
  "provisioning",
  "running",
  "paused",
  "succeeded",
  "failed",
  "cancelled",
]);
export type RunStatus = z.infer<typeof RunStatus>;

export const Hyperparams = z.record(z.union([z.string(), z.number(), z.boolean(), z.null()]));
export type Hyperparams = z.infer<typeof Hyperparams>;

export const DatasetMix = z.object({
  name: z.string(),
  weight: z.number(),
  rows: z.number().optional(),
});

export const Citation = z.object({
  id: z.string(),
  title: z.string(),
  url: z.string().url().optional(),
  source: z.enum(["arxiv", "huggingface", "web"]).optional(),
  authors: z.array(z.string()).optional(),
  added_at: z.string().optional(),
});
export type Citation = z.infer<typeof Citation>;

export const PreflightJson = z.object({
  model: z.string().optional(),
  method: z.string().optional(),
  datasets: z.array(z.object({ name: z.string(), weight: z.number().optional() })).optional(),
  hyperparam_diff: z.record(z.union([z.string(), z.number(), z.boolean(), z.null()])).optional(),
  projected_cost_usd: z.number().optional(),
  citations: z.array(z.string()).optional(),
  errors: z.array(z.string()).optional(),
  validated: z.boolean().optional(),
});
export type PreflightJson = z.infer<typeof PreflightJson>;

const RunRaw = z.object({
  id: z.string(),
  name: z.string(),
  owner_agent: z.string(),
  owner_token_hash: z.string().optional(),
  method: z.string(),
  status: RunStatus,
  base_model: z.string(),
  gpu: z.string(),
  hyperparams: Hyperparams.optional(),
  dataset_mixture: z.array(DatasetMix).optional(),
  user_goal: z.string().optional(),
  agent_plan: z.string().optional(),
  citations: z.array(Citation).optional(),
  cost_so_far_usd: z.number().optional(),
  gpu_hours: z.number().optional(),
  started_at: z.string().nullable().optional(),
  last_metric_step: z.number().nullable().optional(),
  last_metric_loss: z.number().nullable().optional(),
  created_at: z.string(),
  is_sandbox: z.boolean().optional(),
  parent_run_id: z.string().nullable().optional(),
  preflight_json: PreflightJson.nullable().optional(),
});

export interface Run {
  id: string;
  name: string;
  owner_agent: string;
  owner_token_hash?: string;
  method: string;
  status: RunStatus;
  base_model: string;
  gpu: string;
  hyperparams: Hyperparams;
  dataset_mixture: { name: string; weight: number; rows?: number }[];
  user_goal?: string;
  agent_plan?: string;
  citations: Citation[];
  cost_so_far_usd: number;
  gpu_hours: number;
  started_at?: string | null;
  last_metric_step?: number | null;
  last_metric_loss?: number | null;
  created_at: string;
  is_sandbox?: boolean;
  parent_run_id?: string | null;
  preflight_json?: PreflightJson | null;
}

export const Run: z.ZodType<Run> = RunRaw.transform((r) => ({
  ...r,
  hyperparams: r.hyperparams ?? {},
  dataset_mixture: r.dataset_mixture ?? [],
  citations: r.citations ?? [],
  cost_so_far_usd: r.cost_so_far_usd ?? 0,
  gpu_hours: r.gpu_hours ?? 0,
})) as unknown as z.ZodType<Run>;

// ----- Alert ---------------------------------------------------------------

export const RunAlert = z.object({
  level: z.enum(["error", "warn", "info"]),
  title: z.string(),
  body: z.string(),
  created_at: z.string(),
  source: z.string().optional(),
});
export type RunAlert = z.infer<typeof RunAlert>;

export const RunAlertList = z.object({ alerts: z.array(RunAlert) });

// ----- Research transcript -------------------------------------------------

export const ResearchTranscript = z.object({
  id: z.string().optional(),
  agent: z.string(),
  task: z.string(),
  context: z.string().optional(),
  result_summary: z.string().optional(),
  citations_count: z.number().optional(),
  result_json: z.unknown().optional(),
  started_at: z.string(),
  run_id: z.string().nullable().optional(),
});
export type ResearchTranscript = z.infer<typeof ResearchTranscript>;

export const ResearchTranscriptList = z.object({
  transcripts: z.array(ResearchTranscript),
});

export const RunListResponse = z.object({
  runs: z.array(Run),
  total: z.number().optional(),
});

export const RunMetric = z.object({
  run_id: z.string(),
  step: z.number(),
  loss: z.number().nullable().optional(),
  eval_loss: z.number().nullable().optional(),
  lr: z.number().nullable().optional(),
  tokens: z.number().nullable().optional(),
  ts: z.string(),
});
export type RunMetric = z.infer<typeof RunMetric>;

export const RunNote = z.object({
  id: z.string(),
  run_id: z.string(),
  kind: z.enum(["agent", "system", "user", "tool", "metric", "decision"]),
  author: z.string(),
  body: z.string(),
  created_at: z.string(),
});
export type RunNote = z.infer<typeof RunNote>;

export const StatsSummary = z.object({
  active_runs: z.number(),
  spend_today_usd: z.number(),
  gpu_hours_today: z.number(),
  avg_loss_delta: z.number(),
});
export type StatsSummary = z.infer<typeof StatsSummary>;

export const Paper = z.object({
  id: z.string(),
  title: z.string(),
  abstract: z.string().optional(),
  authors: z.array(z.string()).optional(),
  url: z.string().optional(),
  source: z.enum(["arxiv", "huggingface"]),
  published_at: z.string().optional(),
  upvotes: z.number().optional(),
});
export type Paper = Omit<z.infer<typeof Paper>, "authors"> & { authors: string[] };

export type WSMetricEvent =
  | { type: "metric"; data: RunMetric }
  | { type: "note"; data: RunNote }
  | { type: "status"; data: { status: RunStatus } };
