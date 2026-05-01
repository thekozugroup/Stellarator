"use client";

// Per-tool rendering for assistant tool steps. Each renderer is a small,
// product-aware card; unknown tools fall through to a JSON disclosure.

import { useMemo } from "react";
import {
  AlertCircle,
  AlertTriangle,
  BookOpen,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  FlaskConical,
  Info,
  Loader2,
  PauseCircle,
  PlayCircle,
  Quote,
  Search,
  StopCircle,
  Wrench,
  XCircle,
} from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { ToolStep } from "@/lib/chat/types";
import { cn } from "@/lib/utils";
import { copyToClipboard } from "@/lib/clipboard";

// ----- Status header (shared across renderers) -----------------------------

function StatusBar({ step, label }: { step: ToolStep; label: string }) {
  const dur =
    step.endedAt && step.startedAt
      ? `${Math.max(1, step.endedAt - step.startedAt)}ms`
      : null;
  return (
    <div className="flex items-center gap-2 text-[11px]">
      <StatusIcon status={step.status} />
      <span className="font-medium uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </span>
      <span className="font-mono text-foreground/80">{step.name}</span>
      {dur && <span className="ml-auto font-mono text-muted-foreground">{dur}</span>}
    </div>
  );
}

function StatusIcon({ status }: { status: ToolStep["status"] }) {
  if (status === "running")
    return <Loader2 className="size-3.5 animate-spin text-muted-foreground" aria-label="running" />;
  if (status === "error")
    return <AlertCircle className="size-3.5 text-destructive" aria-label="error" />;
  return <CheckCircle2 className="size-3.5 text-success" aria-label="done" />;
}

function Shell({
  step,
  label,
  children,
  tone = "default",
}: {
  step: ToolStep;
  label: string;
  children: React.ReactNode;
  tone?: "default" | "danger" | "success" | "info";
}) {
  const toneCls = {
    default: "border-border/60 bg-card/60",
    danger: "border-destructive/30 bg-destructive/5",
    success: "border-success/25 bg-success/5",
    info: "border-primary/25 bg-primary/5",
  }[tone];
  return (
    <div className={cn("rounded-lg border px-3 py-2.5 text-sm", toneCls)}>
      <StatusBar step={step} label={label} />
      <div className="mt-2">{children}</div>
      {step.error && (
        <div className="mt-2 rounded border border-destructive/30 bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {step.error}
        </div>
      )}
    </div>
  );
}

// ----- Per-tool renderers --------------------------------------------------

type Renderer = (step: ToolStep) => React.ReactNode;

const createRunR: Renderer = (step) => {
  const args = (step.args ?? {}) as {
    name?: string;
    method?: string;
    base_model?: string;
    gpu?: string;
  };
  const result = (step.result ?? {}) as { id?: string };
  return (
    <Shell step={step} label="Create run" tone="info">
      <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
        <Field label="Name" value={args.name ?? "—"} />
        <Field label="Method" value={args.method ?? "—"} />
        <Field label="Base model" value={args.base_model ?? "—"} mono />
        <Field label="GPU" value={args.gpu ?? "—"} mono />
      </div>
      {result.id && (
        <div className="mt-2 flex items-center justify-between border-t border-border/50 pt-2">
          <span className="font-mono text-xs text-muted-foreground">{result.id}</span>
          <a
            className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
            href={`/runs/${result.id}`}
          >
            Open run <ExternalLink className="size-3" />
          </a>
        </div>
      )}
    </Shell>
  );
};

const listRunsR: Renderer = (step) => {
  const result = (step.result ?? {}) as {
    runs?: { id: string; name: string; status: string; method?: string }[];
  };
  const rows = result.runs ?? [];
  return (
    <Shell step={step} label="List runs">
      {rows.length === 0 ? (
        <div className="text-xs text-muted-foreground">No runs returned.</div>
      ) : (
        <div className="overflow-hidden rounded border border-border/50">
          <table className="w-full text-xs">
            <thead className="bg-muted/40 text-[10px] uppercase tracking-wider text-muted-foreground">
              <tr>
                <th className="px-2 py-1.5 text-left font-medium">ID</th>
                <th className="px-2 py-1.5 text-left font-medium">Name</th>
                <th className="px-2 py-1.5 text-left font-medium">Method</th>
                <th className="px-2 py-1.5 text-left font-medium">Status</th>
              </tr>
            </thead>
            <tbody>
              {rows.slice(0, 12).map((r) => (
                <tr key={r.id} className="border-t border-border/40">
                  <td className="px-2 py-1.5 font-mono text-foreground/80">{r.id.slice(0, 8)}</td>
                  <td className="px-2 py-1.5">{r.name}</td>
                  <td className="px-2 py-1.5 text-muted-foreground">{r.method ?? "—"}</td>
                  <td className="px-2 py-1.5">
                    <StatusPill status={r.status} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {rows.length > 12 && (
            <div className="border-t border-border/40 bg-muted/20 px-2 py-1 text-[10px] text-muted-foreground">
              + {rows.length - 12} more
            </div>
          )}
        </div>
      )}
    </Shell>
  );
};

const getRunR: Renderer = (step) => {
  const result = (step.result ?? {}) as {
    id?: string;
    name?: string;
    status?: string;
    metrics?: { step: number; loss?: number | null }[];
  };
  return (
    <Shell step={step} label="Get run">
      <div className="flex items-center justify-between">
        <div>
          <div className="text-sm font-medium">{result.name ?? result.id ?? "run"}</div>
          <div className="font-mono text-[11px] text-muted-foreground">{result.id}</div>
        </div>
        {result.status && <StatusPill status={result.status} />}
      </div>
      {result.metrics && result.metrics.length > 0 && (
        <Sparkline points={result.metrics.slice(-50).map((m) => m.loss ?? 0)} />
      )}
    </Shell>
  );
};

const lifecycleR =
  (label: string, Icon: typeof PauseCircle, tone: "default" | "danger" | "success"): Renderer =>
  (step) => {
    const args = (step.args ?? {}) as { run_id?: string };
    const result = (step.result ?? {}) as { ok?: boolean };
    return (
      <Shell step={step} label={label} tone={tone}>
        <div className="flex items-center gap-2 text-xs">
          <Icon className="size-4 text-muted-foreground" />
          <span className="text-muted-foreground">Run</span>
          <span className="font-mono">{args.run_id ?? "—"}</span>
          {result.ok !== undefined && (
            <span
              className={cn(
                "ml-auto rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider",
                result.ok
                  ? "bg-success/15 text-success"
                  : "bg-destructive/15 text-destructive",
              )}
            >
              {result.ok ? "acknowledged" : "failed"}
            </span>
          )}
        </div>
      </Shell>
    );
  };

const addNoteR: Renderer = (step) => {
  const args = (step.args ?? {}) as { body?: string; kind?: string };
  return (
    <Shell step={step} label="Add note">
      <div className="flex items-start gap-2">
        <Quote className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
        <div className="flex-1">
          <div className="mb-1">
            <Badge variant="outline" className="font-normal text-[10px] uppercase tracking-wider">
              {args.kind ?? "note"}
            </Badge>
          </div>
          <div className="text-xs italic text-foreground/85">
            {args.body ?? "(empty)"}
          </div>
        </div>
      </div>
    </Shell>
  );
};

const searchPapersR: Renderer = (step) => {
  const result = (step.result ?? {}) as {
    results?: {
      id: string;
      title: string;
      authors?: string[];
      source: "arxiv" | "huggingface";
      url?: string;
    }[];
  };
  const rows = (result.results ?? []).slice(0, 3);
  return (
    <Shell step={step} label="Search papers">
      <div className="space-y-2">
        {rows.length === 0 && (
          <div className="text-xs text-muted-foreground">No matching papers.</div>
        )}
        {rows.map((p) => (
          <PaperCard key={p.id} paper={p} />
        ))}
      </div>
    </Shell>
  );
};

const getPaperR: Renderer = (step) => {
  const result = (step.result ?? {}) as {
    id?: string;
    title?: string;
    authors?: string[];
    abstract?: string;
    source?: "arxiv" | "huggingface";
    url?: string;
  };
  return (
    <Shell step={step} label="Get paper">
      <PaperCard
        paper={{
          id: result.id ?? "",
          title: result.title ?? "Untitled",
          authors: result.authors,
          source: result.source ?? "arxiv",
          url: result.url,
        }}
        expanded
        abstract={result.abstract}
      />
    </Shell>
  );
};

const citePaperR: Renderer = (step) => {
  const args = (step.args ?? {}) as { run_id?: string; paper_id?: string };
  return (
    <Shell step={step} label="Cite paper" tone="success">
      <div className="flex items-center gap-2 text-xs">
        <CheckCircle2 className="size-4 text-success" />
        <span>
          Cited <span className="font-mono">{args.paper_id}</span> on run{" "}
          <span className="font-mono">{args.run_id}</span>
        </span>
      </div>
    </Shell>
  );
};

const researchR: Renderer = (step) => {
  const args = (step.args ?? {}) as { task?: string };
  const result = (step.result ?? {}) as {
    datasets_found?: string[];
    hyperparams?: Record<string, unknown>;
    citations_count?: number;
    summary?: string;
  };
  const [expandJson, setExpandJson] = useState(false);
  return (
    <Shell step={step} label="Research" tone="info">
      <div className="flex items-start gap-2 text-xs">
        <Search className="mt-0.5 size-3.5 shrink-0 text-primary" />
        <div className="min-w-0 flex-1 space-y-2">
          {args.task ? (
            <p className="font-medium text-foreground leading-snug">{args.task}</p>
          ) : null}
          {step.status !== "running" ? (
            <div className="grid grid-cols-3 gap-2 rounded border border-border/50 bg-background/40 px-3 py-2">
              <div>
                <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
                  Datasets
                </div>
                <div className="font-semibold text-foreground">
                  {result.datasets_found?.length ?? "—"}
                </div>
              </div>
              <div>
                <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
                  Hyperparams
                </div>
                <div className="font-semibold text-foreground">
                  {result.hyperparams ? Object.keys(result.hyperparams).length : "—"}
                </div>
              </div>
              <div>
                <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
                  Citations
                </div>
                <div className="font-semibold text-foreground">
                  {result.citations_count ?? "—"}
                </div>
              </div>
            </div>
          ) : null}
          {result.summary ? (
            <p className="text-xs text-muted-foreground leading-relaxed">{result.summary}</p>
          ) : null}
          {step.result !== undefined ? (
            <button
              type="button"
              onClick={() => setExpandJson((v) => !v)}
              className="flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground outline-none rounded focus-visible:ring-2 focus-visible:ring-ring"
            >
              {expandJson ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
              {expandJson ? "Hide full result" : "Show full result JSON"}
            </button>
          ) : null}
          {expandJson ? (
            <pre className="overflow-x-auto rounded border border-border/50 bg-background/60 p-2 text-[11px] text-foreground/90">
              {safeJSON(step.result)}
            </pre>
          ) : null}
        </div>
      </div>
    </Shell>
  );
};

const sandboxCreateR: Renderer = (step) => {
  const args = (step.args ?? {}) as {
    name?: string;
    method?: string;
    base_model?: string;
    gpu?: string;
  };
  const result = (step.result ?? {}) as { id?: string };
  return (
    <Shell step={step} label="Sandbox" tone="default">
      <div className="flex items-start gap-2">
        <FlaskConical className="mt-0.5 size-3.5 shrink-0 text-warning" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="rounded-full border border-warning/40 bg-warning/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-warning">
              Sandbox
            </span>
            {args.name ? (
              <span className="font-medium text-sm">{args.name}</span>
            ) : null}
          </div>
          <div className="mt-1.5 grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
            <Field label="Method" value={args.method ?? "—"} />
            <Field label="Model" value={args.base_model ?? "—"} mono />
            <Field label="GPU" value={args.gpu ?? "—"} mono />
          </div>
          {result.id ? (
            <div className="mt-2 flex items-center justify-between border-t border-border/50 pt-2">
              <span className="font-mono text-xs text-muted-foreground">{result.id}</span>
              <a
                href={`/runs/${result.id}`}
                className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
              >
                Open <ExternalLink className="size-3" />
              </a>
            </div>
          ) : null}
        </div>
      </div>
    </Shell>
  );
};

const submitPreflightR: Renderer = (step) => {
  const args = (step.args ?? {}) as {
    model?: string;
    method?: string;
    projected_cost_usd?: number;
    hyperparams?: Record<string, unknown>;
    sandbox_run_id?: string;
    errors?: string[];
    validated?: boolean;
  };
  const validated = args.validated ?? (!args.errors || args.errors.length === 0);
  return (
    <Shell step={step} label="Submit pre-flight" tone={validated ? "success" : "danger"}>
      <div className="space-y-2 text-xs">
        <div className="flex items-center gap-2">
          {validated ? (
            <CheckCircle2 className="size-4 text-success" />
          ) : (
            <XCircle className="size-4 text-destructive" />
          )}
          <span className="font-medium">{validated ? "Validated" : "Validation errors"}</span>
        </div>
        <div className="grid grid-cols-2 gap-x-4 gap-y-1 rounded border border-border/50 bg-background/40 px-3 py-2">
          {args.model ? <Field label="Model" value={args.model} mono /> : null}
          {args.method ? <Field label="Method" value={args.method} /> : null}
          {args.projected_cost_usd != null ? (
            <Field label="Projected cost" value={`$${args.projected_cost_usd.toFixed(2)}`} />
          ) : null}
          {args.sandbox_run_id ? (
            <Field label="Sandbox run" value={args.sandbox_run_id.slice(0, 8)} mono />
          ) : null}
        </div>
        {args.errors && args.errors.length > 0 ? (
          <div className="rounded border border-destructive/30 bg-destructive/5 p-2 space-y-1">
            {args.errors.map((e, i) => (
              <p key={i} className="text-xs text-destructive">{e}</p>
            ))}
          </div>
        ) : null}
      </div>
    </Shell>
  );
};

const readAlertsR: Renderer = (step) => {
  const result = (step.result ?? {}) as {
    alerts?: { level: string; title: string; source?: string }[];
  };
  const alerts = result.alerts ?? [];
  const levelIcon = (level: string) => {
    if (level === "error") return <AlertCircle className="size-3.5 text-destructive shrink-0" />;
    if (level === "warn") return <AlertTriangle className="size-3.5 text-warning shrink-0" />;
    return <Info className="size-3.5 text-primary shrink-0" />;
  };
  return (
    <Shell step={step} label="Read alerts">
      {alerts.length === 0 ? (
        <div className="text-xs text-muted-foreground">No alerts.</div>
      ) : (
        <div className="space-y-1.5">
          {alerts.slice(0, 8).map((a, i) => (
            <div key={i} className="flex items-start gap-2 text-xs">
              {levelIcon(a.level)}
              <span className="font-medium text-foreground">{a.title}</span>
              {a.source ? (
                <span className="ml-auto rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                  {a.source}
                </span>
              ) : null}
            </div>
          ))}
          {alerts.length > 8 ? (
            <div className="text-[10px] text-muted-foreground">+ {alerts.length - 8} more</div>
          ) : null}
        </div>
      )}
    </Shell>
  );
};

// ----- Registry ------------------------------------------------------------

const REGISTRY: Record<string, Renderer> = {
  stellarator_create_run: createRunR,
  stellarator_list_runs: listRunsR,
  stellarator_get_run: getRunR,
  stellarator_cancel_run: lifecycleR("Cancel run", StopCircle, "danger"),
  stellarator_pause_run: lifecycleR("Pause run", PauseCircle, "default"),
  stellarator_resume_run: lifecycleR("Resume run", PlayCircle, "success"),
  stellarator_add_note: addNoteR,
  stellarator_search_papers: searchPapersR,
  stellarator_get_paper: getPaperR,
  stellarator_cite_paper: citePaperR,
  research: researchR,
  stellarator_sandbox_create: sandboxCreateR,
  sandbox_create: sandboxCreateR,
  stellarator_submit_preflight: submitPreflightR,
  submit_preflight: submitPreflightR,
  stellarator_read_alerts: readAlertsR,
  read_alerts: readAlertsR,
};

export function ToolStepRenderer({ step }: { step: ToolStep }) {
  // Specialized renderers for HTTP-status-shaped error payloads — 412/402 take
  // precedence over the per-tool renderer so the user sees an actionable card.
  const errorPayload = parseStatusError(step);
  if (errorPayload?.status === 412) {
    return <PreflightRejectedCard step={step} payload={errorPayload} />;
  }
  if (errorPayload?.status === 402) {
    return <BudgetExceededCard step={step} payload={errorPayload} />;
  }
  const renderer = REGISTRY[step.name];
  if (renderer) return <>{renderer(step)}</>;
  return <FallbackRenderer step={step} />;
}

interface StatusErrorPayload {
  status: number;
  error?: string;
  hint?: string;
  missing_fields?: string[];
  current_spend?: number;
  monthly_limit?: number;
  projected?: number;
}

function parseStatusError(step: ToolStep): StatusErrorPayload | null {
  // The runtime might place the error JSON in step.error (string) or step.result.
  const candidates: unknown[] = [step.result, step.error];
  for (const c of candidates) {
    if (!c) continue;
    let obj: unknown = c;
    if (typeof c === "string") {
      try {
        obj = JSON.parse(c);
      } catch {
        continue;
      }
    }
    if (obj && typeof obj === "object" && "status" in obj) {
      const s = (obj as { status?: unknown }).status;
      if (s === 412 || s === 402) return obj as StatusErrorPayload;
    }
  }
  return null;
}

function PreflightRejectedCard({
  step,
  payload,
}: {
  step: ToolStep;
  payload: StatusErrorPayload;
}) {
  function reemitTemplate() {
    const template = {
      tool: step.name,
      missing: payload.missing_fields ?? [],
      hint: payload.hint ?? "Fill the missing fields and resubmit.",
    };
    const text = JSON.stringify(template, null, 2);
    if (typeof navigator !== "undefined") {
      void copyToClipboard(text);
    }
    window.dispatchEvent(
      new CustomEvent("stellarator:cite-intent", { detail: { paperId: text } }),
    );
  }
  return (
    <div className="rounded-lg border border-destructive/40 bg-destructive/5 px-3 py-2.5 text-sm">
      <div className="flex items-center gap-2 text-[11px]">
        <XCircle className="size-3.5 text-destructive" />
        <span className="font-semibold uppercase tracking-[0.14em] text-destructive">
          Pre-flight rejected
        </span>
        <span className="ml-auto font-mono text-[10px] text-destructive/80">412</span>
      </div>
      <p className="mt-2 text-xs text-foreground">
        {payload.error ?? "The pre-flight validator rejected this run."}
      </p>
      {payload.hint && (
        <p className="mt-1 text-[11px] italic text-muted-foreground">{payload.hint}</p>
      )}
      {payload.missing_fields && payload.missing_fields.length > 0 && (
        <div className="mt-2 space-y-1">
          <p className="text-[10px] uppercase tracking-wider text-muted-foreground">
            Missing fields
          </p>
          <ul className="space-y-0.5 text-xs">
            {payload.missing_fields.map((f) => (
              <li key={f} className="font-mono text-destructive">
                · {f}
              </li>
            ))}
          </ul>
        </div>
      )}
      <div className="mt-3 flex justify-end">
        <Button
          size="sm"
          variant="outline"
          className="h-7 gap-1.5 text-xs"
          onClick={reemitTemplate}
        >
          <Wrench className="size-3" />
          Submit preflight
        </Button>
      </div>
    </div>
  );
}

function BudgetExceededCard({
  step: _step,
  payload,
}: {
  step: ToolStep;
  payload: StatusErrorPayload;
}) {
  const fmt = (n?: number) =>
    n != null ? `$${n.toFixed(2)}` : "—";
  return (
    <div className="rounded-lg border border-warning/40 bg-warning/10 px-3 py-2.5 text-sm">
      <div className="flex items-center gap-2 text-[11px]">
        <AlertTriangle className="size-3.5 text-warning" />
        <span className="font-semibold uppercase tracking-[0.14em] text-warning">
          Budget exceeded
        </span>
        <span className="ml-auto font-mono text-[10px] text-warning/80">402</span>
      </div>
      <p className="mt-2 text-xs text-foreground">
        {payload.error ?? "This run would exceed your configured monthly budget."}
      </p>
      <div className="mt-2 grid grid-cols-3 gap-2 rounded border border-warning/30 bg-background/40 px-3 py-2">
        <BudgetMetric label="Current spend" value={fmt(payload.current_spend)} />
        <BudgetMetric label="Monthly limit" value={fmt(payload.monthly_limit)} />
        <BudgetMetric label="Projected" value={fmt(payload.projected)} tone="danger" />
      </div>
      <div className="mt-3 flex justify-end">
        <Button asChild size="sm" variant="outline" className="h-7 gap-1.5 text-xs">
          <a href="/settings#budgets">Raise budget</a>
        </Button>
      </div>
    </div>
  );
}

function BudgetMetric({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone?: "danger";
}) {
  return (
    <div>
      <div className="text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div
        className={cn(
          "font-mono text-sm font-semibold tabular-nums",
          tone === "danger" ? "text-destructive" : "text-foreground",
        )}
      >
        {value}
      </div>
    </div>
  );
}

function FallbackRenderer({ step }: { step: ToolStep }) {
  const argsStr = useMemo(() => safeJSON(step.args), [step.args]);
  const resultStr = useMemo(
    () => (step.result === undefined ? "" : safeJSON(step.result)),
    [step.result],
  );
  return (
    <details className="group rounded-lg border border-border/60 bg-card/60 px-3 py-2 text-xs">
      <summary
        className="flex cursor-pointer items-center gap-2 font-mono outline-none"
        aria-label={`Tool ${step.name} details`}
      >
        <ChevronRight className="size-3.5 transition-transform group-open:rotate-90" />
        <Wrench className="size-3.5 text-primary" />
        <StatusIcon status={step.status} />
        <span>{step.name}</span>
      </summary>
      <div className="mt-2 space-y-2">
        <Section label="args">{argsStr}</Section>
        {resultStr && <Section label="result">{resultStr}</Section>}
        {step.error && <Section label="error">{step.error}</Section>}
      </div>
    </details>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-1 text-[10px] uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <pre className="overflow-x-auto rounded bg-background/60 p-2 text-foreground/90">
        {children}
      </pre>
    </div>
  );
}

// ----- Sub-components ------------------------------------------------------

function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <div className="text-[10px] uppercase tracking-wider text-muted-foreground">{label}</div>
      <div className={cn("truncate", mono && "font-mono text-[11px]")}>{value}</div>
    </div>
  );
}

function StatusPill({ status }: { status: string }) {
  const tone =
    status === "running" || status === "provisioning"
      ? "bg-warning/15 text-warning"
      : status === "succeeded"
        ? "bg-success/15 text-success"
        : status === "failed" || status === "cancelled"
          ? "bg-destructive/15 text-destructive"
          : "bg-muted/40 text-foreground/80";
  return (
    <span className={cn("rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider", tone)}>
      {status}
    </span>
  );
}

function Sparkline({ points }: { points: number[] }) {
  if (points.length < 2) return null;
  const min = Math.min(...points);
  const max = Math.max(...points);
  const range = max - min || 1;
  const w = 240;
  const h = 36;
  const dx = w / (points.length - 1);
  const path = points
    .map((p, i) => `${i === 0 ? "M" : "L"} ${(i * dx).toFixed(1)} ${(h - ((p - min) / range) * h).toFixed(1)}`)
    .join(" ");
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="mt-2 h-9 w-full" aria-label="Loss sparkline">
      <path d={path} fill="none" stroke="currentColor" strokeWidth={1.25} className="text-primary" />
    </svg>
  );
}

function PaperCard({
  paper,
  expanded,
  abstract,
}: {
  paper: {
    id: string;
    title: string;
    authors?: string[];
    source: "arxiv" | "huggingface";
    url?: string;
  };
  expanded?: boolean;
  abstract?: string;
}) {
  const sourceTone =
    paper.source === "arxiv"
      ? "bg-destructive/15 text-destructive"
      : "bg-warning/15 text-warning";
  return (
    <div className="rounded border border-border/50 bg-background/40 p-2.5">
      <div className="flex items-start gap-2">
        <BookOpen className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium leading-snug">{paper.title}</div>
          {paper.authors && paper.authors.length > 0 && (
            <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
              {paper.authors.slice(0, 4).join(", ")}
              {paper.authors.length > 4 ? " et al." : ""}
            </div>
          )}
          {expanded && abstract && (
            <p className="mt-2 line-clamp-4 text-xs leading-5 text-foreground/80">{abstract}</p>
          )}
          <div className="mt-2 flex items-center gap-2">
            <span className={cn("rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider", sourceTone)}>
              {paper.source}
            </span>
            {paper.url && (
              <a
                href={paper.url}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-[11px] text-muted-foreground hover:text-foreground"
              >
                Open <ExternalLink className="size-3" />
              </a>
            )}
            <Button
              size="sm"
              variant="outline"
              className="ml-auto h-6 px-2 text-[11px]"
              onClick={() =>
                window.dispatchEvent(
                  new CustomEvent("stellarator:cite-intent", { detail: { paperId: paper.id } }),
                )
              }
            >
              Cite
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function safeJSON(v: unknown): string {
  try {
    return typeof v === "string" ? v : JSON.stringify(v, null, 2);
  } catch {
    return String(v);
  }
}
