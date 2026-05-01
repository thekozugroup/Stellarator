// Chat-specific API helpers. Sits beside lib/api.ts; we don't touch that file.

import type { ChatStreamEvent } from "./types";

const API_URL =
  (process.env.NEXT_PUBLIC_API_URL || "").replace(/\/$/, "");

export function chatApiUrl(): string {
  return API_URL;
}

export interface SendMessageInput {
  sessionId: string;
  model: string;
  driver: string;
  message: string;
  apiKey?: string;
  signal?: AbortSignal;
  onEvent: (ev: ChatStreamEvent) => void;
}

// POST /v1/chat/sessions/{id}/messages, expecting an SSE stream back.
// Reconnects once on transient network error.
export async function sendMessageStream(input: SendMessageInput): Promise<void> {
  await streamOnce(input).catch(async (err) => {
    if (input.signal?.aborted) throw err;
    const transient =
      err instanceof TypeError ||
      (err instanceof Error && /network|fetch|ECONN/i.test(err.message));
    if (!transient) throw err;
    await new Promise((r) => setTimeout(r, 600));
    await streamOnce(input);
  });
}

async function streamOnce(input: SendMessageInput): Promise<void> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    Accept: "text/event-stream",
  };
  if (input.apiKey) headers["X-OpenAI-Key"] = input.apiKey;

  const res = await fetch(
    `${API_URL}/v1/chat/sessions/${encodeURIComponent(input.sessionId)}/messages`,
    {
      method: "POST",
      headers,
      body: JSON.stringify({
        model: input.model,
        driver: input.driver,
        message: input.message,
      }),
      signal: input.signal,
    },
  );

  if (!res.ok || !res.body) {
    const body = await res.text().catch(() => "");
    throw new Error(`HTTP ${res.status}${body ? `: ${body.slice(0, 200)}` : ""}`);
  }

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buf = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const parts = buf.split("\n\n");
    buf = parts.pop() ?? "";
    for (const part of parts) {
      const line = part
        .split("\n")
        .find((l) => l.startsWith("data:"));
      if (!line) continue;
      const payload = line.slice(5).trim();
      if (!payload || payload === "[DONE]") {
        input.onEvent({ type: "done" });
        continue;
      }
      try {
        const parsed = JSON.parse(payload) as ChatStreamEvent;
        input.onEvent(parsed);
      } catch {
        /* skip malformed line */
      }
    }
  }
}

// Best-effort upsert of session metadata to backend. Failure is non-fatal.
export async function upsertSession(opts: {
  id: string;
  title: string;
  model: string;
  driver: string;
}): Promise<void> {
  try {
    await fetch(`${API_URL}/v1/chat/sessions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(opts),
    });
  } catch {
    /* offline-tolerant */
  }
}
