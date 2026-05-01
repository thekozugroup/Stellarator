"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import { useLiveQuery } from "dexie-react-hooks";
import { MoreHorizontal, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { db } from "@/lib/chat/db";
import { createThread, deleteThread } from "@/lib/chat/threads";
import type { ChatThread } from "@/lib/chat/types";
import { cn } from "@/lib/utils";

// Manifest.build-style dense thread list. Search at top, "+ New thread"
// always visible, hover reveals row actions, active row gets left border accent.
export function ThreadList() {
  const router = useRouter();
  const params = useParams<{ threadId?: string }>();
  const activeId = params?.threadId;
  const [query, setQuery] = useState("");

  const threads = useLiveQuery<ChatThread[]>(
    async () => {
      const rows = await db().threads.where("archived").equals(0).toArray();
      return rows.sort((a, b) => b.updatedAt - a.updatedAt);
    },
    [],
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = threads ?? [];
    if (!q) return list;
    return list.filter((t) => t.title.toLowerCase().includes(q));
  }, [threads, query]);

  async function handleNew() {
    const t = await createThread();
    router.push(`/chat/${t.id}`);
  }

  async function handleDelete(id: string) {
    await deleteThread(id);
    if (activeId === id) router.push("/chat");
  }

  async function handleRename(id: string, current: string) {
    const next = window.prompt("Rename thread", current);
    if (next === null) return;
    const trimmed = next.trim();
    if (!trimmed) return;
    await db().threads.update(id, { title: trimmed, updatedAt: Date.now() });
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-border/50 p-2">
        <div className="relative mb-2">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search threads"
            className="h-8 pl-8 text-xs"
            aria-label="Search threads"
          />
        </div>
        <Button
          onClick={handleNew}
          variant="outline"
          size="sm"
          className="h-8 w-full justify-start gap-2 text-xs"
        >
          <Plus className="size-3.5" />
          New thread
        </Button>
      </div>

      <nav className="flex-1 overflow-y-auto px-1 py-1.5" aria-label="Threads">
        {(filtered ?? []).length === 0 ? (
          <div className="px-3 py-6 text-center text-xs text-muted-foreground">
            {query ? "No matches." : "No threads yet."}
          </div>
        ) : (
          <ul className="space-y-px">
            {filtered.map((t) => {
              const active = t.id === activeId;
              return (
                <li key={t.id} className="group relative">
                  <Link
                    href={`/chat/${t.id}`}
                    className={cn(
                      "block rounded-md py-1.5 pl-3 pr-8 text-sm transition-colors",
                      "border-l-2 border-transparent",
                      active
                        ? "border-primary bg-accent/40 text-foreground"
                        : "text-foreground/75 hover:bg-accent/25 hover:text-foreground",
                    )}
                  >
                    <div className="truncate text-[13px] leading-5">{t.title}</div>
                    <div className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
                      <span className="font-mono">{t.model}</span>
                      <span>·</span>
                      <span>{relTime(t.updatedAt)}</span>
                    </div>
                  </Link>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild>
                      <button
                        className={cn(
                          "absolute right-1 top-1.5 grid size-6 place-items-center rounded text-muted-foreground transition-opacity",
                          "opacity-0 group-hover:opacity-100 focus:opacity-100 focus-visible:opacity-100 hover:bg-accent/60 hover:text-foreground",
                        )}
                        aria-label={`Actions for ${t.title}`}
                      >
                        <MoreHorizontal className="size-3.5" />
                      </button>
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" className="w-36">
                      <DropdownMenuItem onClick={() => handleRename(t.id, t.title)}>
                        <Pencil className="mr-2 size-3.5" /> Rename
                      </DropdownMenuItem>
                      <DropdownMenuItem
                        onClick={() => handleDelete(t.id)}
                        className="text-destructive focus:text-destructive"
                      >
                        <Trash2 className="mr-2 size-3.5" /> Delete
                      </DropdownMenuItem>
                    </DropdownMenuContent>
                  </DropdownMenu>
                </li>
              );
            })}
          </ul>
        )}
      </nav>
    </div>
  );
}

function relTime(ts: number): string {
  const s = Math.floor((Date.now() - ts) / 1000);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  return new Date(ts).toLocaleDateString();
}
