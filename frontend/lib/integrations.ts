"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { z } from "zod";

const API_URL =
  process.env.NEXT_PUBLIC_API_URL?.replace(/\/$/, "") || "http://localhost:8000";

function viewerToken(): string | undefined {
  if (typeof window !== "undefined") {
    const local = window.localStorage.getItem("stellarator.viewerToken");
    if (local) return local;
  }
  return process.env.NEXT_PUBLIC_VIEWER_TOKEN;
}

async function apiFetch<T>(path: string, schema: z.ZodType<T>, init: RequestInit = {}): Promise<T> {
  const token = viewerToken();
  const headers = new Headers(init.headers);
  headers.set("Accept", "application/json");
  if (init.body && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const res = await fetch(`${API_URL}${path}`, { ...init, headers, cache: "no-store" });
  const text = await res.text();
  if (!res.ok) throw new Error(text || `Request failed: ${path}`);
  if (!text) return undefined as unknown as T;
  return schema.parse(JSON.parse(text));
}

// ---------------------------------------------------------------------------
// Schemas
// ---------------------------------------------------------------------------
export const IntegrationKeySchema = z.object({
  kind: z.string(),
  masked: z.string(),
  set_at: z.string().nullable(),
  last_used_at: z.string().nullable(),
});
export type IntegrationKey = z.infer<typeof IntegrationKeySchema>;

const KeysResponseSchema = z.array(IntegrationKeySchema);

const TestResultSchema = z.object({
  ok: z.boolean(),
  latency_ms: z.number().optional(),
  error: z.string().optional(),
});

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

export function useIntegrationKeys() {
  return useQuery({
    queryKey: ["integrations", "keys"],
    queryFn: () => apiFetch("/v1/integrations/keys", KeysResponseSchema),
    staleTime: 30_000,
  });
}

export function useSetIntegrationKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kind, value }: { kind: string; value: string }) =>
      apiFetch(`/v1/integrations/keys/${kind}`, z.unknown(), {
        method: "PUT",
        body: JSON.stringify({ value }),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["integrations", "keys"] });
    },
  });
}

export function useDeleteIntegrationKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (kind: string) =>
      apiFetch(`/v1/integrations/keys/${kind}`, z.unknown(), { method: "DELETE" }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["integrations", "keys"] });
    },
  });
}

export function useTestIntegrationKey() {
  return useMutation({
    mutationFn: (kind: string) =>
      apiFetch(`/v1/integrations/keys/${kind}/test`, TestResultSchema, { method: "POST" }),
    onSuccess: (data) => {
      if (data.ok) {
        toast.success("Connection OK", {
          description: data.latency_ms != null ? `${data.latency_ms} ms` : undefined,
        });
      } else {
        toast.error("Connection failed", { description: data.error });
      }
    },
    onError: (err: Error) => {
      toast.error("Test failed", { description: err.message });
    },
  });
}

// ---------------------------------------------------------------------------
// OpenRouter model list (cached 1h via react-query)
// ---------------------------------------------------------------------------

const OpenRouterModelSchema = z.object({
  id: z.string(),
  name: z.string(),
  context_length: z.number().optional(),
  pricing: z.object({ prompt: z.string().optional() }).optional(),
});

const OpenRouterModelsResponseSchema = z.object({
  data: z.array(OpenRouterModelSchema),
});

export interface OpenRouterModelItem {
  id: string; // e.g. "openai/gpt-4o"
  label: string;
  subtitle?: string;
}

export function useOpenRouterModels(): {
  models: OpenRouterModelItem[];
  isLoading: boolean;
} {
  const { data: keys } = useIntegrationKeys();
  const hasKey = !!keys?.find((k) => k.kind === "openrouter");

  const { data, isLoading } = useQuery({
    queryKey: ["openrouter", "models"],
    queryFn: async (): Promise<OpenRouterModelItem[]> => {
      const res = await fetch("https://openrouter.ai/api/v1/models", {
        headers: { Accept: "application/json" },
        cache: "no-store",
      });
      if (!res.ok) throw new Error("OpenRouter model list unavailable");
      const json = await res.json() as unknown;
      const parsed = OpenRouterModelsResponseSchema.parse(json);
      // Top 30 by index (API returns popularity-sorted)
      return parsed.data.slice(0, 30).map((m) => ({
        id: m.id,
        label: m.name,
        subtitle: m.context_length ? `${Math.round(m.context_length / 1000)}k ctx` : undefined,
      }));
    },
    enabled: hasKey,
    staleTime: 60 * 60 * 1000, // 1h
    gcTime: 2 * 60 * 60 * 1000,
  });

  return { models: data ?? [], isLoading };
}

/** Derives whether OpenAI OAuth is connected from the keys list.
 * @deprecated OpenAI OAuth is removed from the UI — use useIntegrationKeys directly for openrouter/codex.
 */
export function useOpenAIOAuthState() {
  const { data: keys } = useIntegrationKeys();
  const oauthKey = keys?.find((k) => k.kind === "openai-oauth");
  return {
    isSignedIn: !!oauthKey,
    masked: oauthKey?.masked ?? null,
    setAt: oauthKey?.set_at ?? null,
  };
}
