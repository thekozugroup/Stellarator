"use client";

import { Suspense, useEffect } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { toast } from "sonner";
import { useNotifications } from "@/lib/notifications";

/**
 * Mount once at the root: opens an SSE connection for notifications,
 * surfaces sonner toasts on run_finished / alert_error / sandbox_ready,
 * and consumes the ?oauth=… callback param to confirm sign-in.
 *
 * Note: openai_connected is no longer used (OpenAI OAuth removed). Only
 * codex_connected is handled.
 */
export function NotificationListener() {
  return (
    <Suspense fallback={null}>
      <Inner />
    </Suspense>
  );
}

function Inner() {
  useNotifications();
  const sp = useSearchParams();
  const router = useRouter();

  useEffect(() => {
    const oauth = sp.get("oauth");
    if (!oauth) return;
    if (oauth === "codex_connected") {
      toast.success("Signed in with Codex");
    } else if (oauth.endsWith("_failed")) {
      toast.error("Sign-in failed", { description: "Please try again from Settings." });
    }
    // Strip the param without triggering navigation
    const next = new URLSearchParams(sp.toString());
    next.delete("oauth");
    const qs = next.toString();
    const path = typeof window !== "undefined" ? window.location.pathname : "/";
    router.replace(qs ? `${path}?${qs}` : path, { scroll: false });
  }, [sp, router]);

  return null;
}
