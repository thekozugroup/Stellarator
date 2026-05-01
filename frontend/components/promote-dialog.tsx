"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useMutation } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, Rocket } from "lucide-react";
import { toast } from "sonner";
import { z } from "zod";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Run } from "@/lib/types";
import { cn } from "@/lib/utils";

const API_URL =
  process.env.NEXT_PUBLIC_API_URL?.replace(/\/$/, "") || "http://localhost:8000";

const GPU_TYPES = ["H100", "A100", "L40S", "H200", "MI300X"] as const;

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sandbox: Run;
}

export function PromoteDialog({ open, onOpenChange, sandbox }: Props) {
  const router = useRouter();
  const [name, setName] = useState(`${sandbox.name} (production)`);
  const [gpuType, setGpuType] = useState<string>("H100");
  const [gpuCount, setGpuCount] = useState(4);
  const [overridesText, setOverridesText] = useState("");
  const [overridesOpen, setOverridesOpen] = useState(false);
  const [userGoal, setUserGoal] = useState(sandbox.user_goal ?? "");

  useEffect(() => {
    setName(`${sandbox.name} (production)`);
    setUserGoal(sandbox.user_goal ?? "");
  }, [sandbox]);

  const promote = useMutation({
    mutationFn: async () => {
      let overrides: Record<string, unknown> | undefined;
      if (overridesText.trim()) {
        try {
          overrides = JSON.parse(overridesText) as Record<string, unknown>;
        } catch (e) {
          throw new Error(`Invalid JSON in overrides: ${(e as Error).message}`);
        }
      }
      const token =
        typeof window !== "undefined"
          ? window.localStorage.getItem("stellarator.viewerToken") ?? ""
          : "";
      const headers: Record<string, string> = {
        "Content-Type": "application/json",
        Accept: "application/json",
      };
      if (token) headers.Authorization = `Bearer ${token}`;
      const res = await fetch(`${API_URL}/v1/runs/${sandbox.id}/promote`, {
        method: "POST",
        headers,
        body: JSON.stringify({
          name,
          gpu_type: gpuType,
          gpu_count: gpuCount,
          hyperparams_overrides: overrides,
          user_goal: userGoal || undefined,
        }),
      });
      const text = await res.text();
      if (!res.ok) throw new Error(text || `HTTP ${res.status}`);
      return Run.parse(JSON.parse(text));
    },
    onSuccess: (run) => {
      toast.success("Promoted to production", {
        description: `Created run ${run.name}`,
      });
      onOpenChange(false);
      router.push(`/runs/${run.id}`);
    },
    onError: (e: Error) => {
      toast.error("Promotion failed", { description: e.message });
    },
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Rocket className="size-4 text-primary" />
            Promote sandbox to production
          </DialogTitle>
        </DialogHeader>
        <div className="space-y-4 pt-1">
          <Field label="Name">
            <Input value={name} onChange={(e) => setName(e.target.value)} />
          </Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="GPU type">
              <Select value={gpuType} onValueChange={setGpuType}>
                <SelectTrigger className="h-9">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {GPU_TYPES.map((g) => (
                    <SelectItem key={g} value={g}>
                      {g}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
            <Field label="GPU count">
              <Input
                type="number"
                min={1}
                max={64}
                value={gpuCount}
                onChange={(e) => setGpuCount(parseInt(e.target.value, 10) || 1)}
              />
            </Field>
          </div>
          <Field label="User goal" hint="Override the goal for production. Optional.">
            <textarea
              value={userGoal}
              onChange={(e) => setUserGoal(e.target.value)}
              rows={3}
              className="w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
              placeholder="Reach <0.4 eval loss on the held-out set."
            />
          </Field>
          <div>
            <button
              type="button"
              onClick={() => setOverridesOpen((v) => !v)}
              className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
            >
              {overridesOpen ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
              Hyperparam overrides (JSON)
            </button>
            {overridesOpen && (
              <textarea
                value={overridesText}
                onChange={(e) => setOverridesText(e.target.value)}
                rows={5}
                spellCheck={false}
                placeholder={'{ "lr": 1e-5, "warmup_steps": 200 }'}
                className={cn(
                  "mt-2 w-full resize-y rounded-md border border-input bg-background/40 p-2 font-mono text-xs leading-relaxed outline-none focus-visible:ring-2 focus-visible:ring-ring",
                )}
              />
            )}
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => promote.mutate()}
            disabled={promote.isPending || !name.trim()}
          >
            <Rocket className="size-3.5" />
            {promote.isPending ? "Promoting…" : "Promote"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <label className="mb-1 block text-xs font-medium text-foreground">{label}</label>
      {children}
      {hint && <p className="mt-1 text-[11px] text-muted-foreground">{hint}</p>}
    </div>
  );
}
