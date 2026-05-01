"use client";

// /chat — redirect to most recent thread, or create one and route to it.

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Loader2 } from "lucide-react";
import { ensureThread } from "@/lib/chat/threads";

export default function ChatIndexPage() {
  const router = useRouter();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const id = await ensureThread();
        if (!cancelled) router.replace(`/chat/${id}`);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Failed to open chat");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [router]);

  return (
    <div className="grid h-full place-items-center text-sm text-muted-foreground">
      {error ? (
        <p className="text-destructive">{error}</p>
      ) : (
        <span className="inline-flex items-center gap-2">
          <Loader2 className="size-4 animate-spin" />
          Opening chat…
        </span>
      )}
    </div>
  );
}
