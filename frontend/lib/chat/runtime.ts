// Lightweight assistant-ui-style runtime. Owns: thread state, streaming
// reducer, tool-step lifecycle, persistence to Dexie. Exposes a hook
// (useChatRuntime) for the panel to consume.

"use client";

import { nanoid } from "nanoid";
import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import { db } from "./db";
import { sendMessageStream, upsertSession } from "./api";
import type {
  ChatMessage,
  ChatStreamEvent,
  ChatThread,
  ModelId,
  ToolStep,
} from "./types";
import { driverFor } from "./types";
import { readKey } from "./key-store";

type State = {
  thread: ChatThread | null;
  messages: ChatMessage[];
  busy: boolean;
  error: string | null;
};

type Action =
  | { type: "hydrate"; thread: ChatThread; messages: ChatMessage[] }
  | { type: "thread/update"; patch: Partial<ChatThread> }
  | { type: "msg/append"; message: ChatMessage }
  | { type: "msg/patch"; id: string; patch: Partial<ChatMessage> }
  | { type: "msg/tool/upsert"; assistantId: string; step: ToolStep }
  | { type: "msg/tool/patch"; assistantId: string; stepId: string; patch: Partial<ToolStep> }
  | { type: "busy"; busy: boolean }
  | { type: "error"; error: string | null };

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "hydrate":
      return { ...state, thread: action.thread, messages: action.messages };
    case "thread/update":
      return state.thread
        ? { ...state, thread: { ...state.thread, ...action.patch, updatedAt: Date.now() } }
        : state;
    case "msg/append":
      return { ...state, messages: [...state.messages, action.message] };
    case "msg/patch":
      return {
        ...state,
        messages: state.messages.map((m) =>
          m.id === action.id ? { ...m, ...action.patch } : m,
        ),
      };
    case "msg/tool/upsert":
      return {
        ...state,
        messages: state.messages.map((m) => {
          if (m.id !== action.assistantId) return m;
          const steps = m.toolSteps ?? [];
          const idx = steps.findIndex((s) => s.id === action.step.id);
          const next = idx === -1 ? [...steps, action.step] : steps.map((s, i) => (i === idx ? action.step : s));
          return { ...m, toolSteps: next };
        }),
      };
    case "msg/tool/patch":
      return {
        ...state,
        messages: state.messages.map((m) => {
          if (m.id !== action.assistantId) return m;
          const steps = (m.toolSteps ?? []).map((s) =>
            s.id === action.stepId ? { ...s, ...action.patch } : s,
          );
          return { ...m, toolSteps: steps };
        }),
      };
    case "busy":
      return { ...state, busy: action.busy };
    case "error":
      return { ...state, error: action.error };
  }
}

const initialState: State = { thread: null, messages: [], busy: false, error: null };

export function useChatRuntime(threadId: string) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const abortRef = useRef<AbortController | null>(null);

  // Hydrate from Dexie when the thread id changes.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const t = await db().threads.get(threadId);
      if (!t || cancelled) return;
      const ms = await db().messages.where("threadId").equals(threadId).sortBy("createdAt");
      if (cancelled) return;
      dispatch({ type: "hydrate", thread: t, messages: ms });
    })();
    return () => {
      cancelled = true;
      abortRef.current?.abort();
    };
  }, [threadId]);

  const setModel = useCallback(async (model: ModelId) => {
    const driver = driverFor(model);
    dispatch({ type: "thread/update", patch: { model, driver } });
    await db().threads.update(threadId, { model, driver, updatedAt: Date.now() });
  }, [threadId]);

  const renameThread = useCallback(async (title: string) => {
    dispatch({ type: "thread/update", patch: { title } });
    await db().threads.update(threadId, { title, updatedAt: Date.now() });
  }, [threadId]);

  const send = useCallback(async (text: string) => {
    const t = state.thread;
    if (!t || !text.trim()) return;

    const now = Date.now();
    const userMsg: ChatMessage = {
      id: nanoid(),
      threadId,
      role: "user",
      content: text,
      createdAt: now,
    };
    const assistantMsg: ChatMessage = {
      id: nanoid(),
      threadId,
      role: "assistant",
      content: "",
      toolSteps: [],
      model: t.model,
      driver: t.driver,
      createdAt: now + 1,
      streaming: true,
    };
    dispatch({ type: "msg/append", message: userMsg });
    dispatch({ type: "msg/append", message: assistantMsg });
    dispatch({ type: "busy", busy: true });
    dispatch({ type: "error", error: null });

    await db().messages.bulkPut([userMsg, assistantMsg]);

    // First-message title heuristic.
    if (t.title === "New thread") {
      const title = text.slice(0, 60).replace(/\s+/g, " ").trim();
      await db().threads.update(threadId, { title, updatedAt: Date.now() });
      dispatch({ type: "thread/update", patch: { title } });
      void upsertSession({ id: threadId, title, model: t.model, driver: t.driver });
    } else {
      void upsertSession({ id: threadId, title: t.title, model: t.model, driver: t.driver });
    }

    const ac = new AbortController();
    abortRef.current = ac;

    const apiKey = t.driver === "openai" ? readKey() : undefined;
    let acc = "";

    const persistAssistant = async () => {
      // Pull latest from state via closure on next tick — but we already have content in `acc`,
      // and tool steps are mirrored via dispatches. Persist a snapshot.
      const snap = await db().messages.get(assistantMsg.id);
      if (!snap) return;
      await db().messages.put({ ...snap, content: acc, streaming: false });
    };

    try {
      await sendMessageStream({
        sessionId: threadId,
        model: t.model,
        driver: t.driver,
        message: text,
        apiKey,
        signal: ac.signal,
        onEvent: (ev: ChatStreamEvent) => {
          if (ev.type === "delta") {
            acc += ev.text;
            dispatch({ type: "msg/patch", id: assistantMsg.id, patch: { content: acc } });
          } else if (ev.type === "tool_call") {
            const step: ToolStep = {
              id: ev.id,
              name: ev.name,
              args: ev.args,
              status: "running",
              startedAt: Date.now(),
            };
            dispatch({ type: "msg/tool/upsert", assistantId: assistantMsg.id, step });
          } else if (ev.type === "tool_result") {
            dispatch({
              type: "msg/tool/patch",
              assistantId: assistantMsg.id,
              stepId: ev.id,
              patch: {
                result: ev.result,
                error: ev.error,
                status: ev.error ? "error" : "done",
                endedAt: Date.now(),
              },
            });
          } else if (ev.type === "error") {
            dispatch({ type: "error", error: ev.message });
          }
        },
      });
    } catch (e) {
      if (!ac.signal.aborted) {
        const msg = e instanceof Error ? e.message : "Stream failed";
        dispatch({ type: "error", error: msg });
      }
    } finally {
      dispatch({ type: "msg/patch", id: assistantMsg.id, patch: { streaming: false } });
      dispatch({ type: "busy", busy: false });
      await persistAssistant();
      await db().threads.update(threadId, { updatedAt: Date.now() });
    }
  }, [state.thread, threadId]);

  const cancel = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  return useMemo(
    () => ({
      thread: state.thread,
      messages: state.messages,
      busy: state.busy,
      error: state.error,
      send,
      cancel,
      setModel,
      renameThread,
    }),
    [state, send, cancel, setModel, renameThread],
  );
}
