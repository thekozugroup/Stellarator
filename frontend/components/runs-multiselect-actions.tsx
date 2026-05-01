"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Pin, PinOff, Square, Workflow, X } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { usePrefs } from "@/lib/local-prefs";
import { useWhoami } from "@/lib/use-whoami";
import type { Run } from "@/lib/types";

const COMPARE_MAX = 6;

export function RunsMultiselectActions({
  selected,
  runs,
  onClear,
}: {
  selected: string[];
  runs: Run[];
  onClear: () => void;
}) {
  const router = useRouter();
  const { setPrefs, prefs } = usePrefs();
  const { agent } = useWhoami();
  const [confirmCancel, setConfirmCancel] = useState(false);

  if (selected.length === 0) return null;

  const selectedRuns = runs.filter((r) => selected.includes(r.id));
  const allOwnerRunning = selectedRuns.every(
    (r) => r.owner_agent === agent && r.status === "running",
  );
  const overCompare = selected.length > COMPARE_MAX;

  function compare() {
    if (overCompare) {
      toast.error(`Compare supports up to ${COMPARE_MAX} runs`);
      return;
    }
    router.push(`/runs/compare?ids=${selected.join(",")}`);
  }

  function pinAll() {
    const set = new Set(prefs.pinnedRuns);
    selected.forEach((id) => set.add(id));
    setPrefs({ pinnedRuns: [...set] });
    toast.success(`Pinned ${selected.length} runs`);
  }

  function unpinAll() {
    const set = new Set(prefs.pinnedRuns);
    selected.forEach((id) => set.delete(id));
    setPrefs({ pinnedRuns: [...set] });
    toast.success(`Unpinned ${selected.length} runs`);
  }

  async function cancelAll() {
    setConfirmCancel(false);
    let okCount = 0;
    for (const id of selected) {
      try {
        await api.cancelRun(id);
        okCount += 1;
      } catch {
        /* ignore individual failures */
      }
    }
    toast.success(`Cancelled ${okCount} of ${selected.length} runs`);
    onClear();
  }

  return (
    <>
      <div className="flex flex-wrap items-center gap-2 rounded-lg border border-primary/30 bg-primary/5 px-3 py-2 text-xs">
        <span className="font-medium text-foreground">
          {selected.length} selected
        </span>
        <div className="ml-auto flex flex-wrap items-center gap-1.5">
          <Button
            size="sm"
            variant="outline"
            className="h-7 gap-1 text-xs"
            onClick={compare}
            disabled={overCompare || selected.length < 2}
            title={overCompare ? `Max ${COMPARE_MAX}` : undefined}
          >
            <Workflow className="size-3.5" />
            Compare selected
          </Button>
          <Button size="sm" variant="ghost" className="h-7 gap-1 text-xs" onClick={pinAll}>
            <Pin className="size-3.5" /> Pin
          </Button>
          <Button size="sm" variant="ghost" className="h-7 gap-1 text-xs" onClick={unpinAll}>
            <PinOff className="size-3.5" /> Unpin
          </Button>
          {allOwnerRunning && (
            <Button
              size="sm"
              variant="ghost"
              className="h-7 gap-1 text-xs text-destructive hover:text-destructive"
              onClick={() => setConfirmCancel(true)}
            >
              <Square className="size-3.5" /> Cancel all
            </Button>
          )}
          <button
            type="button"
            onClick={onClear}
            aria-label="Clear selection"
            className="grid size-7 place-items-center rounded text-muted-foreground hover:text-foreground"
          >
            <X className="size-3.5" />
          </button>
        </div>
      </div>

      <Dialog open={confirmCancel} onOpenChange={setConfirmCancel}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Cancel {selected.length} runs?</DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            This will request cancellation on all selected running jobs. Cannot be undone.
          </p>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setConfirmCancel(false)}>
              Keep running
            </Button>
            <Button variant="destructive" size="sm" onClick={() => void cancelAll()}>
              Cancel runs
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
