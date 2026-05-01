"use client";

import { useEffect, useRef, useState } from "react";
import { wsUrl } from "./api";
import type { WSMetricEvent } from "./types";

export type WSStatus = "connecting" | "open" | "closed" | "error";

export function useRunStream(runId: string | undefined) {
  const [status, setStatus] = useState<WSStatus>("connecting");
  const [events, setEvents] = useState<WSMetricEvent[]>([]);
  const ref = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);

  useEffect(() => {
    if (!runId) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    function connect() {
      if (cancelled) return;
      setStatus("connecting");
      let ws: WebSocket;
      try {
        ws = new WebSocket(wsUrl(runId!));
      } catch {
        scheduleRetry();
        return;
      }
      ref.current = ws;
      ws.onopen = () => {
        retryRef.current = 0;
        setStatus("open");
      };
      ws.onmessage = (msg) => {
        try {
          const parsed = JSON.parse(msg.data) as WSMetricEvent;
          setEvents((prev) => [...prev.slice(-499), parsed]);
        } catch {
          /* ignore */
        }
      };
      ws.onerror = () => setStatus("error");
      ws.onclose = () => {
        setStatus("closed");
        scheduleRetry();
      };
    }

    function scheduleRetry() {
      if (cancelled) return;
      const backoff = Math.min(15000, 500 * 2 ** retryRef.current++);
      timer = setTimeout(connect, backoff);
    }

    connect();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
      ref.current?.close();
    };
  }, [runId]);

  return { status, events };
}
