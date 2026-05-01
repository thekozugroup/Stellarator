"use client";

import * as React from "react";
import { useRouter } from "next/navigation";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  BookText,
  GaugeCircle,
  MessageSquare,
  Pin,
  PinOff,
  PlusCircle,
  Settings,
  Star,
  StopCircle,
  Workflow,
} from "lucide-react";
import { toast } from "sonner";

import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "@/components/ui/command";
import { api } from "@/lib/api";
import type { Run } from "@/lib/types";
import { usePrefs } from "@/lib/local-prefs";

type RunListData = { runs: Run[]; total?: number };

export function CommandPalette(): React.ReactElement {
  const [open, setOpen] = React.useState<boolean>(false);
  const router = useRouter();
  const qc = useQueryClient();
  const { isPinned, togglePin } = usePrefs();

  // Reuse the cached list from the dashboard if present.
  const runsQuery = useQuery<RunListData>({
    queryKey: ["runs"],
    queryFn: () => api.listRuns({ limit: 50 }),
    staleTime: 5_000,
    enabled: open,
  });
  const runs: Run[] = runsQuery.data?.runs ?? [];

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const run = React.useCallback(
    (fn: () => void): void => {
      setOpen(false);
      // Defer so the dialog close animation doesn't swallow the navigation.
      setTimeout(fn, 0);
    },
    [],
  );

  const cancel = async (id: string, name: string): Promise<void> => {
    try {
      await api.cancelRun(id);
      toast.success(`Cancelled ${name}`);
      qc.invalidateQueries({ queryKey: ["runs"] });
    } catch {
      toast.error(`Failed to cancel ${name}`);
    }
  };

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder="Search runs, jump to a page, run an action..." />
      <CommandList>
        <CommandEmpty>No matches.</CommandEmpty>

        <CommandGroup heading="Navigation">
          <CommandItem
            value="dashboard runs"
            onSelect={() => run(() => router.push("/"))}
          >
            <GaugeCircle /> Dashboard
            <CommandShortcut>G D</CommandShortcut>
          </CommandItem>
          <CommandItem value="chat" onSelect={() => run(() => router.push("/chat"))}>
            <MessageSquare /> Chat
          </CommandItem>
          <CommandItem
            value="research papers"
            onSelect={() => run(() => router.push("/research"))}
          >
            <BookText /> Research
          </CommandItem>
          <CommandItem
            value="compare runs"
            onSelect={() => run(() => router.push("/runs/compare"))}
          >
            <Workflow /> Compare runs
          </CommandItem>
          <CommandItem value="settings" onSelect={() => run(() => router.push("/settings"))}>
            <Settings /> Settings
          </CommandItem>
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Actions">
          <CommandItem
            value="new chat plan run"
            onSelect={() => run(() => router.push("/chat"))}
          >
            <PlusCircle /> New chat / plan a run
          </CommandItem>
        </CommandGroup>

        {runs.length > 0 ? (
          <>
            <CommandSeparator />
            <CommandGroup heading={`Runs (${runs.length})`}>
              {runs.slice(0, 30).map((r) => {
                const pinned = isPinned(r.id);
                return (
                  <CommandItem
                    key={`open-${r.id}`}
                    value={`open ${r.name} ${r.id} ${r.base_model} ${r.method}`}
                    onSelect={() => run(() => router.push(`/runs/${r.id}`))}
                  >
                    {pinned ? (
                      <Star className="text-warning" />
                    ) : (
                      <GaugeCircle />
                    )}
                    <span className="truncate">{r.name}</span>
                    <span className="ml-auto truncate font-mono text-[10px] text-muted-foreground">
                      {r.base_model}
                    </span>
                  </CommandItem>
                );
              })}
            </CommandGroup>

            <CommandSeparator />

            <CommandGroup heading="Pin / unpin">
              {runs.slice(0, 20).map((r) => {
                const pinned = isPinned(r.id);
                return (
                  <CommandItem
                    key={`pin-${r.id}`}
                    value={`${pinned ? "unpin" : "pin"} ${r.name} ${r.id}`}
                    onSelect={() =>
                      run(() => {
                        togglePin(r.id);
                        toast.success(`${pinned ? "Unpinned" : "Pinned"} ${r.name}`);
                      })
                    }
                  >
                    {pinned ? <PinOff /> : <Pin />}
                    {pinned ? "Unpin" : "Pin"} {r.name}
                  </CommandItem>
                );
              })}
            </CommandGroup>

            <CommandSeparator />

            <CommandGroup heading="Cancel run">
              {runs
                .filter((r) =>
                  ["queued", "provisioning", "running", "paused"].includes(r.status),
                )
                .slice(0, 20)
                .map((r) => (
                  <CommandItem
                    key={`cancel-${r.id}`}
                    value={`cancel ${r.name} ${r.id}`}
                    onSelect={() => run(() => void cancel(r.id, r.name))}
                  >
                    <StopCircle className="text-destructive" />
                    Cancel {r.name}
                  </CommandItem>
                ))}
            </CommandGroup>
          </>
        ) : null}
      </CommandList>
    </CommandDialog>
  );
}
