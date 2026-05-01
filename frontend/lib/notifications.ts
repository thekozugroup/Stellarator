"use client";

import { useEffect, useRef } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

const API_URL =
  process.env.NEXT_PUBLIC_API_URL?.replace(/\/$/, "") || "http://localhost:8000";

export type NotificationLevel = "info" | "warn" | "error";
export type NotificationType =
  | "run_finished"
  | "alert_error"
  | "sandbox_ready";

export interface NotificationEvent {
  type: NotificationType;
  run_id: string;
  run_name: string;
  message: string;
  level: NotificationLevel;
  ts: string;
}

const ALERTS_LAST_SEEN_KEY = "stellarator.alerts.lastSeenAt";

export function getAlertsLastSeen(): number {
  if (typeof window === "undefined") return 0;
  const v = window.localStorage.getItem(ALERTS_LAST_SEEN_KEY);
  return v ? parseInt(v, 10) || 0 : 0;
}

export function markAlertsSeen(): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(ALERTS_LAST_SEEN_KEY, String(Date.now()));
  window.dispatchEvent(new CustomEvent("stellarator:alerts-seen"));
}

/**
 * Mounts an EventSource subscription to /v1/notifications/stream and renders
 * sonner toasts on each event. Dedupes by (ts|run_id|type).
 */
export function useNotifications(): void {
  const qc = useQueryClient();
  const seen = useRef<Set<string>>(new Set());
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const token = window.localStorage.getItem("stellarator.viewerToken") ?? "";
    // EventSource doesn't support auth headers — pass via query param.
    const qs = token ? `?token=${encodeURIComponent(token)}` : "";
    let cancelled = false;

    function connect(): void {
      if (cancelled) return;
      try {
        const es = new EventSource(`${API_URL}/v1/notifications/stream${qs}`);
        esRef.current = es;
        es.onmessage = (msg) => {
          let evt: NotificationEvent;
          try {
            evt = JSON.parse(msg.data) as NotificationEvent;
          } catch {
            return;
          }
          const key = `${evt.ts}|${evt.run_id}|${evt.type}`;
          if (seen.current.has(key)) return;
          seen.current.add(key);
          if (seen.current.size > 500) {
            // Trim to keep memory bounded.
            seen.current = new Set(Array.from(seen.current).slice(-200));
          }
          dispatchToast(evt);
          // Invalidate downstream queries so badges refresh
          if (evt.type === "alert_error") {
            void qc.invalidateQueries({ queryKey: ["run-alerts", evt.run_id] });
            window.dispatchEvent(
              new CustomEvent<NotificationEvent>("stellarator:notification", { detail: evt }),
            );
          }
          if (evt.type === "run_finished") {
            void qc.invalidateQueries({ queryKey: ["runs"] });
            void qc.invalidateQueries({ queryKey: ["run", evt.run_id] });
          }
          if (evt.type === "sandbox_ready") {
            window.dispatchEvent(
              new CustomEvent<NotificationEvent>("stellarator:sandbox-ready", { detail: evt }),
            );
          }
        };
        es.onerror = () => {
          es.close();
          esRef.current = null;
          // Reconnect after backoff
          if (!cancelled) setTimeout(connect, 4000);
        };
      } catch {
        if (!cancelled) setTimeout(connect, 4000);
      }
    }
    connect();
    return () => {
      cancelled = true;
      esRef.current?.close();
      esRef.current = null;
    };
  }, [qc]);
}

function dispatchToast(evt: NotificationEvent): void {
  const goto = (href: string) => {
    if (typeof window !== "undefined") window.location.assign(href);
  };
  if (evt.type === "run_finished") {
    toast.success(`Run ${evt.run_name} finished`, {
      description: evt.message,
      action: { label: "View run", onClick: () => goto(`/runs/${evt.run_id}`) },
    });
    return;
  }
  if (evt.type === "alert_error") {
    toast.error(`Alert on ${evt.run_name}`, {
      description: evt.message,
      action: { label: "View alerts", onClick: () => goto(`/runs/${evt.run_id}?tab=alerts`) },
    });
    return;
  }
  if (evt.type === "sandbox_ready") {
    toast.info(`Sandbox ${evt.run_name} ready to promote`, {
      description: evt.message,
      action: {
        label: "Promote",
        onClick: () => {
          if (typeof window === "undefined") return;
          window.dispatchEvent(
            new CustomEvent<NotificationEvent>("stellarator:promote-intent", { detail: evt }),
          );
          goto(`/runs/${evt.run_id}`);
        },
      },
    });
  }
}
