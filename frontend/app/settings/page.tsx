"use client";

import { useEffect, useRef, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  BrainCircuit,
  ChevronDown,
  ChevronRight,
  Cpu,
  ExternalLink,
  LogIn,
  LogOut,
  Network,
  Terminal,
} from "lucide-react";
import { useTheme } from "next-themes";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { PasswordInput } from "@/components/ui/password-input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { AgentBadge } from "@/components/agent-badge";
import { PageContainer } from "@/components/ui/page-container";
import { BootstrapPrompt } from "@/components/bootstrap-prompt";
import { useWhoami } from "@/lib/use-whoami";
import { usePrefs, type Density } from "@/lib/local-prefs";
import { db } from "@/lib/chat/db";
import { api } from "@/lib/api";
import {
  useIntegrationKeys,
  useSetIntegrationKey,
  useDeleteIntegrationKey,
  useTestIntegrationKey,
} from "@/lib/integrations";

// ---------------------------------------------------------------------------
// Density toggle
// ---------------------------------------------------------------------------
function DensityToggle({ value, onChange }: { value: Density; onChange: (d: Density) => void }) {
  return (
    <div role="radiogroup" aria-label="Row density" className="flex w-fit items-center rounded-md border border-border/60 p-0.5">
      {(["compact", "comfy"] as const).map((d) => (
        <button
          key={d}
          type="button"
          role="radio"
          aria-checked={value === d}
          onClick={() => onChange(d)}
          className={[
            "rounded px-3 py-1 text-xs uppercase tracking-wider transition-colors",
            value === d ? "bg-accent text-foreground" : "text-muted-foreground hover:text-foreground",
          ].join(" ")}
        >
          {d}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Provider row with collapse/expand
// ---------------------------------------------------------------------------
function ProviderRow({
  icon: Icon,
  label,
  summary,
  defaultOpen = false,
  children,
  id,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  summary: React.ReactNode;
  defaultOpen?: boolean;
  children: React.ReactNode;
  id?: string;
}) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <div id={id} className="border-b border-border/40 last:border-0 scroll-mt-16">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-3 py-3 text-left transition-colors hover:bg-accent/30"
        aria-expanded={open}
      >
        <Icon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium">{label}</p>
          <p className="text-xs text-muted-foreground">{summary}</p>
        </div>
        {open ? (
          <ChevronDown className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        ) : (
          <ChevronRight className="size-4 shrink-0 text-muted-foreground" aria-hidden />
        )}
      </button>
      {open && <div className="pb-4 pl-7">{children}</div>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Budgets card — wired to /v1/cost/budgets + /v1/cost/summary
// ---------------------------------------------------------------------------
function BudgetsCard() {
  const qc = useQueryClient();

  const [scope, setScope] = useState<"agent" | "run">("agent");
  const [scopeId, setScopeId] = useState("");
  const [monthlyLimit, setMonthlyLimit] = useState(1000);
  const [dailyLimit, setDailyLimit] = useState<number | "">("");
  const [thresh, setThresh] = useState(80);

  const budgetsQ = useQuery({
    queryKey: ["budgets"],
    queryFn: () => api.listBudgets(),
  });

  const summaryQ = useQuery({
    queryKey: ["cost-summary"],
    queryFn: () => api.getCostSummary(),
  });

  const createMut = useMutation({
    mutationFn: () =>
      api.createBudget({
        scope,
        scope_id: scopeId || undefined,
        monthly_limit_usd: monthlyLimit,
        daily_limit_usd: dailyLimit !== "" ? dailyLimit : undefined,
        alert_threshold_pct: thresh,
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["budgets"] });
      toast.success("Budget created");
      setScopeId("");
      setMonthlyLimit(1000);
      setDailyLimit("");
      setThresh(80);
    },
    onError: (e: Error) => {
      toast.error("Failed to create budget", { description: e.message });
    },
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => api.deleteBudget(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["budgets"] });
      toast.success("Budget removed");
    },
    onError: (e: Error) => {
      toast.error("Failed to remove budget", { description: e.message });
    },
  });

  const budgets = budgetsQ.data?.budgets ?? [];
  const totalSpend = summaryQ.data?.total_usd ?? 0;
  const spendByScope = summaryQ.data?.by_scope ?? [];

  function getSpend(b: { scope: string; scope_id?: string }): number {
    if (spendByScope.length === 0) return 0;
    const match = spendByScope.find(
      (s) => s.scope === b.scope && (b.scope_id ? s.scope_id === b.scope_id : true),
    );
    return match?.spend_usd ?? totalSpend;
  }

  function handleAdd() {
    if (!monthlyLimit || monthlyLimit < 1) return;
    void createMut.mutateAsync();
  }

  return (
    <Card id="budgets">
      <CardHeader className="pb-3">
        <CardTitle className="text-sm font-semibold">Budgets</CardTitle>
        <CardDescription className="text-xs">
          Monthly spend caps with alert thresholds. Triggers a 402 from the planner.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {budgetsQ.isLoading ? (
          <p className="text-xs text-muted-foreground">Loading…</p>
        ) : budgetsQ.isError ? (
          <p className="text-xs text-destructive">Failed to load budgets.</p>
        ) : budgets.length === 0 ? (
          <p className="text-xs text-muted-foreground">No budgets configured.</p>
        ) : (
          <ul className="space-y-2">
            {budgets.map((b) => {
              const spend = getSpend(b);
              const pct = (spend / b.monthly_limit_usd) * 100;
              const barColor = pct >= 100 ? "bg-destructive" : pct >= 80 ? "bg-warning" : "bg-primary";
              return (
                <li
                  key={b.id}
                  className="flex items-center justify-between gap-3 rounded-md border border-border/60 bg-card/40 px-3 py-2"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium capitalize">{b.scope}</span>
                      {b.scope_id ? (
                        <span className="font-mono text-[10px] text-muted-foreground">{b.scope_id}</span>
                      ) : null}
                    </div>
                    <div className="mt-0.5 text-[11px] text-muted-foreground">
                      ${spend.toFixed(2)} / ${b.monthly_limit_usd} · alert at {b.alert_threshold_pct}%
                    </div>
                    <div className="mt-1.5 h-1 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        className={`h-full transition-all ${barColor}`}
                        style={{ width: `${Math.min(100, Math.max(1, pct))}%` }}
                      />
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7 shrink-0 text-xs text-destructive hover:text-destructive"
                    disabled={deleteMut.isPending}
                    onClick={() => void deleteMut.mutateAsync(b.id)}
                  >
                    Remove
                  </Button>
                </li>
              );
            })}
          </ul>
        )}

        {/* Add form */}
        <div className="flex flex-wrap items-end gap-2 rounded-md border border-dashed border-border/60 px-3 py-2.5">
          <div className="min-w-0">
            <label className="block text-[10px] uppercase tracking-wider text-muted-foreground">
              Scope
            </label>
            <Select value={scope} onValueChange={(v) => setScope(v as "agent" | "run")}>
              <SelectTrigger className="h-8 w-28 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="agent" className="text-xs">Agent</SelectItem>
                <SelectItem value="run" className="text-xs">Run</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-muted-foreground">
              Scope ID
            </label>
            <input
              type="text"
              placeholder="leave blank = all"
              value={scopeId}
              onChange={(e) => setScopeId(e.target.value)}
              className="h-8 w-32 rounded-md border bg-transparent px-2 text-xs placeholder:text-muted-foreground/50"
            />
          </div>
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-muted-foreground">
              Monthly $
            </label>
            <input
              type="number"
              min={1}
              value={monthlyLimit}
              onChange={(e) => setMonthlyLimit(parseInt(e.target.value, 10) || 0)}
              className="h-8 w-24 rounded-md border bg-transparent px-2 text-xs"
            />
          </div>
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-muted-foreground">
              Daily $ (opt.)
            </label>
            <input
              type="number"
              min={1}
              value={dailyLimit}
              onChange={(e) => setDailyLimit(e.target.value === "" ? "" : parseInt(e.target.value, 10) || 0)}
              placeholder="—"
              className="h-8 w-20 rounded-md border bg-transparent px-2 text-xs placeholder:text-muted-foreground/50"
            />
          </div>
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-muted-foreground">
              Alert %
            </label>
            <input
              type="number"
              min={1}
              max={100}
              value={thresh}
              onChange={(e) => setThresh(parseInt(e.target.value, 10) || 0)}
              className="h-8 w-20 rounded-md border bg-transparent px-2 text-xs"
            />
          </div>
          <Button
            size="sm"
            variant="outline"
            className="ml-auto h-8 text-xs"
            disabled={createMut.isPending || !monthlyLimit || monthlyLimit < 1}
            onClick={handleAdd}
          >
            {createMut.isPending ? "Adding…" : "Add budget"}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Tinker API Key card
// ---------------------------------------------------------------------------
function TinkerKeyCard() {
  const { data: keys } = useIntegrationKeys();
  const setKey = useSetIntegrationKey();
  const deleteKey = useDeleteIntegrationKey();
  const testKey = useTestIntegrationKey();

  const existing = keys?.find((k) => k.kind === "tinker");
  const [value, setValue] = useState("");
  const [dirty, setDirty] = useState(false);

  async function handleSave() {
    await setKey.mutateAsync({ kind: "tinker", value });
    setValue("");
    setDirty(false);
    toast.success("Tinker API key saved");
  }

  async function handleDelete() {
    await deleteKey.mutateAsync("tinker");
    toast.success("Tinker API key removed");
  }

  const summary = existing
    ? `Connected · last used ${existing.last_used_at ? new Date(existing.last_used_at).toLocaleDateString() : "recently"}`
    : "Not configured";

  return (
    <ProviderRow id="tinker" icon={Cpu} label="Tinker" summary={summary} defaultOpen={!existing}>
      <div className="space-y-2">
        {existing ? (
          <div className="rounded-md border border-border/60 bg-card/50 px-3 py-2 text-xs text-muted-foreground">
            <span className="font-mono text-foreground">{existing.masked}</span>
            {existing.last_used_at && (
              <span className="ml-2">· last used {new Date(existing.last_used_at).toLocaleDateString()}</span>
            )}
          </div>
        ) : (
          <PasswordInput
            value={value}
            onChange={(e) => {
              setValue(e.target.value);
              setDirty(true);
            }}
            placeholder="tk-..."
          />
        )}
        <div className="flex items-center justify-between">
          <p className="text-[11px] text-muted-foreground">
            Per-agent key encrypted at rest. Falls back to server TINKER_API_KEY if unset.
          </p>
          <div className="flex gap-2">
            {existing ? (
              <>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 text-xs text-destructive hover:text-destructive"
                  onClick={() => void handleDelete()}
                  disabled={deleteKey.isPending}
                >
                  Remove
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 text-xs"
                  onClick={() => testKey.mutate("tinker")}
                  disabled={testKey.isPending}
                >
                  Test
                </Button>
              </>
            ) : (
              <Button
                size="sm"
                variant="outline"
                className="h-7 text-xs"
                disabled={!dirty || setKey.isPending}
                onClick={() => void handleSave()}
              >
                Save
              </Button>
            )}
          </div>
        </div>
      </div>
    </ProviderRow>
  );
}

// ---------------------------------------------------------------------------
// OpenAI API key card (sessionStorage fallback — OAuth removed)
// ---------------------------------------------------------------------------
function OpenAICard() {
  const [apiKey, setApiKey] = useState("");
  const [apiKeyDirty, setApiKeyDirty] = useState(false);
  const savedApiKey = useRef("");

  useEffect(() => {
    const k = sessionStorage.getItem("stellarator.chat.openaiKey") ?? "";
    setApiKey(k);
    savedApiKey.current = k;
  }, []);

  function saveApiKey() {
    if (apiKey) sessionStorage.setItem("stellarator.chat.openaiKey", apiKey);
    else sessionStorage.removeItem("stellarator.chat.openaiKey");
    savedApiKey.current = apiKey;
    setApiKeyDirty(false);
    toast.success("OpenAI key saved for this tab");
  }

  function removeApiKey() {
    sessionStorage.removeItem("stellarator.chat.openaiKey");
    setApiKey("");
    savedApiKey.current = "";
    setApiKeyDirty(false);
    toast.success("OpenAI key removed");
  }

  const summary = apiKey ? "API key configured" : "Not configured";

  return (
    <ProviderRow
      id="openai"
      icon={BrainCircuit}
      label="OpenAI API key (fallback)"
      summary={summary}
      defaultOpen={!apiKey}
    >
      <div className="space-y-2">
        <PasswordInput
          value={apiKey}
          onChange={(e) => {
            setApiKey(e.target.value);
            setApiKeyDirty(e.target.value !== savedApiKey.current);
          }}
          placeholder="sk-..."
        />
        <div className="flex items-center justify-between">
          <p className="text-[11px] text-muted-foreground">
            Stored in sessionStorage — cleared when this tab closes. For persistent access use{" "}
            <strong>Codex</strong> sign-in above.
          </p>
          <div className="flex gap-2">
            {apiKey && (
              <Button
                size="sm"
                variant="ghost"
                className="h-7 text-xs text-destructive hover:text-destructive"
                onClick={removeApiKey}
              >
                Remove
              </Button>
            )}
            <Button
              size="sm"
              variant="outline"
              className="h-7 text-xs"
              disabled={!apiKeyDirty}
              onClick={saveApiKey}
            >
              Save
            </Button>
          </div>
        </div>
      </div>
    </ProviderRow>
  );
}

// ---------------------------------------------------------------------------
// OpenRouter card
// ---------------------------------------------------------------------------
function OpenRouterCard() {
  const { data: keys } = useIntegrationKeys();
  const setKey = useSetIntegrationKey();
  const deleteKey = useDeleteIntegrationKey();
  const testKey = useTestIntegrationKey();

  const existing = keys?.find((k) => k.kind === "openrouter");
  const [value, setValue] = useState("");
  const [dirty, setDirty] = useState(false);

  async function handleSave() {
    await setKey.mutateAsync({ kind: "openrouter", value });
    setValue("");
    setDirty(false);
    toast.success("OpenRouter key saved");
  }

  async function handleDelete() {
    await deleteKey.mutateAsync("openrouter");
    toast.success("OpenRouter key removed");
  }

  const summary = existing
    ? `Connected · last used ${existing.last_used_at ? new Date(existing.last_used_at).toLocaleDateString() : "recently"}`
    : "Not configured";

  return (
    <ProviderRow id="openrouter" icon={Network} label="OpenRouter" summary={summary} defaultOpen={!existing}>
      <div className="space-y-2">
        {existing ? (
          <div className="rounded-md border border-border/60 bg-card/50 px-3 py-2 text-xs text-muted-foreground">
            <span className="font-mono text-foreground">{existing.masked}</span>
            {existing.last_used_at && (
              <span className="ml-2">· last used {new Date(existing.last_used_at).toLocaleDateString()}</span>
            )}
          </div>
        ) : (
          <PasswordInput
            value={value}
            onChange={(e) => {
              setValue(e.target.value);
              setDirty(true);
            }}
            placeholder="sk-or-..."
          />
        )}
        <div className="flex items-center justify-between">
          <p className="text-[11px] text-muted-foreground">
            Encrypted at rest. Get a key at{" "}
            <a
              href="https://openrouter.ai/keys"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-0.5 underline underline-offset-2 hover:text-foreground"
            >
              openrouter.ai/keys <ExternalLink className="size-2.5" />
            </a>
          </p>
          <div className="flex gap-2">
            {existing ? (
              <>
                <Button
                  size="sm"
                  variant="ghost"
                  className="h-7 text-xs text-destructive hover:text-destructive"
                  onClick={() => void handleDelete()}
                  disabled={deleteKey.isPending}
                >
                  Remove
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-7 text-xs"
                  onClick={() => testKey.mutate("openrouter")}
                  disabled={testKey.isPending}
                >
                  Test
                </Button>
              </>
            ) : (
              <Button
                size="sm"
                variant="outline"
                className="h-7 text-xs"
                disabled={!dirty || setKey.isPending}
                onClick={() => void handleSave()}
              >
                Save
              </Button>
            )}
          </div>
        </div>
      </div>
    </ProviderRow>
  );
}

// ---------------------------------------------------------------------------
// Codex card
// ---------------------------------------------------------------------------
function CodexCard({
  codexUser,
  onSignIn,
  onSignOut,
}: {
  codexUser: string | null;
  onSignIn: () => void;
  onSignOut: () => void;
}) {
  const summary = codexUser ? `Connected as ${codexUser}` : "Not configured";

  return (
    <ProviderRow icon={Terminal} label="Codex" summary={summary} defaultOpen={!codexUser}>
      <div className="space-y-2">
        {codexUser ? (
          <div className="flex items-center justify-between rounded-md border border-border/60 bg-card/50 px-3 py-2">
            <p className="text-xs text-muted-foreground">
              Connected as <span className="font-mono text-foreground">{codexUser}</span>
            </p>
            <Button
              size="sm"
              variant="ghost"
              className="h-7 gap-1.5 text-xs text-muted-foreground hover:text-destructive"
              onClick={onSignOut}
            >
              <LogOut className="size-3.5" /> Sign out
            </Button>
          </div>
        ) : (
          <Button
            size="sm"
            variant="outline"
            className="h-8 gap-2 text-xs"
            onClick={onSignIn}
          >
            <LogIn className="size-3.5" /> Sign in with Codex
          </Button>
        )}
      </div>
    </ProviderRow>
  );
}

// ---------------------------------------------------------------------------
// Main settings page
// ---------------------------------------------------------------------------
export default function SettingsPage() {
  const { agent, isLoading: whoamiLoading } = useWhoami();
  const { prefs, setPrefs } = usePrefs();
  const { theme, setTheme } = useTheme();

  // --- Viewer token (localStorage) ---
  const [token, setToken] = useState("");
  const [tokenDirty, setTokenDirty] = useState(false);
  const savedToken = useRef("");

  // --- Codex ---
  const [codexUser, setCodexUser] = useState<string | null>(null);

  useEffect(() => {
    const t = localStorage.getItem("stellarator.viewerToken") ?? "";
    setToken(t);
    savedToken.current = t;
    const cu = sessionStorage.getItem("stellarator.codex.user");
    if (cu) setCodexUser(cu);
  }, []);

  // --- Inline confirmation state ---
  const [confirmPins, setConfirmPins] = useState(false);
  const [confirmThreads, setConfirmThreads] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);

  function saveToken() {
    if (token) localStorage.setItem("stellarator.viewerToken", token);
    else localStorage.removeItem("stellarator.viewerToken");
    savedToken.current = token;
    setTokenDirty(false);
    toast.success("Access token saved");
  }

  async function signInCodex() {
    try {
      const { url } = await api.codexOAuthStart();
      window.location.href = url;
    } catch (e) {
      toast.error("Codex sign-in failed", { description: (e as Error).message });
    }
  }

  function signOutCodex() {
    sessionStorage.removeItem("stellarator.codex.user");
    setCodexUser(null);
    toast.success("Signed out of Codex");
  }

  function clearPins() {
    setPrefs({ pinnedRuns: [] });
    setConfirmPins(false);
    toast.success("Pinned runs cleared");
  }

  async function clearThreads() {
    await db().threads.clear();
    await db().messages.clear();
    setConfirmThreads(false);
    toast.success("Chat threads cleared");
  }

  function resetPrefs() {
    setPrefs({ density: "compact", defaultChatModel: "claude-sonnet-4-6" });
    setConfirmReset(false);
    toast.success("Preferences reset");
  }

  const role = agent === "anonymous" ? "viewer" : "owner";

  return (
    <PageContainer className="max-w-[720px]">
      <header>
        <h1 className="text-xl font-semibold tracking-tight">Settings</h1>
        <p className="mt-0.5 text-sm text-muted-foreground">
          Credentials are held in your browser or encrypted at rest — never stored in plain text.
        </p>
      </header>

      {/* Identity */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm font-semibold">Identity</CardTitle>
          <CardDescription className="text-xs">Your current agent context.</CardDescription>
        </CardHeader>
        <CardContent className="flex items-center gap-3">
          {whoamiLoading ? (
            <span className="text-xs text-muted-foreground">Loading…</span>
          ) : (
            <>
              <AgentBadge agent={agent} />
              <span className="text-xs capitalize text-muted-foreground">{role}</span>
            </>
          )}
        </CardContent>
      </Card>

      {/* Preferences */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm font-semibold">Preferences</CardTitle>
        </CardHeader>
        <CardContent className="space-y-5">
          <Row label="Table density" hint="Controls row height in the runs table.">
            <DensityToggle value={prefs.density} onChange={(d) => setPrefs({ density: d })} />
          </Row>
          <Row label="Default chat model" hint="Used when creating new threads.">
            <Select
              value={prefs.defaultChatModel}
              onValueChange={(v) => setPrefs({ defaultChatModel: v })}
            >
              <SelectTrigger className="h-8 w-[180px] text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="claude-sonnet-4-6" className="text-xs">Claude Sonnet 4.6</SelectItem>
                <SelectItem value="claude-opus-4-7" className="text-xs">Claude Opus 4.7</SelectItem>
                <SelectItem value="claude-haiku-4-5" className="text-xs">Claude Haiku 4.5</SelectItem>
                <SelectItem value="gpt-4o" className="text-xs">GPT-4o</SelectItem>
                <SelectItem value="o1" className="text-xs">o1</SelectItem>
                <SelectItem value="codex" className="text-xs">Codex</SelectItem>
              </SelectContent>
            </Select>
          </Row>
          <Row label="Theme" hint="Interface color scheme.">
            <Select value={theme ?? "system"} onValueChange={setTheme}>
              <SelectTrigger className="h-8 w-[120px] text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="light" className="text-xs">Light</SelectItem>
                <SelectItem value="dark" className="text-xs">Dark</SelectItem>
                <SelectItem value="system" className="text-xs">System</SelectItem>
              </SelectContent>
            </Select>
          </Row>
        </CardContent>
      </Card>

      {/* Stellarator access — standalone card */}
      <Card id="access-token">
        <CardHeader className="pb-3">
          <CardTitle className="text-sm font-semibold">Stellarator access</CardTitle>
          <CardDescription className="text-xs">
            Your agent bearer token for the Stellarator API.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2">
          <PasswordInput
            value={token}
            onChange={(e) => {
              setToken(e.target.value);
              setTokenDirty(e.target.value !== savedToken.current);
            }}
            placeholder="stl-..."
          />
          <div className="flex items-center justify-between">
            <p className="text-[11px] text-muted-foreground">
              One of <code className="font-mono">AGENT_TOKEN_*</code>. Sent as{" "}
              <code className="font-mono">Authorization: Bearer …</code> to <code>/v1/*</code>. Held only in your browser.
            </p>
            <Button size="sm" variant="outline" className="h-7 text-xs" disabled={!tokenDirty} onClick={saveToken}>
              Save
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Budgets */}
      <BudgetsCard />

      {/* AI Providers — separate card */}
      <Card id="ai-providers">
        <CardHeader className="pb-3">
          <CardTitle className="text-sm font-semibold">AI Providers</CardTitle>
          <CardDescription className="text-xs">
            Connect AI providers to use them in the Chat interface.
          </CardDescription>
        </CardHeader>
        <CardContent className="px-4 pb-2 pt-0">
          <TinkerKeyCard />
          <OpenAICard />
          <OpenRouterCard />
          <CodexCard
            codexUser={codexUser}
            onSignIn={() => void signInCodex()}
            onSignOut={signOutCodex}
          />
        </CardContent>
      </Card>

      {/* Local data */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm font-semibold">Local Data</CardTitle>
          <CardDescription className="text-xs">Manage browser-stored data. These actions are permanent.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <LocalAction
            label="Pinned runs"
            hint="Remove all pinned run bookmarks."
            actionLabel="Clear"
            confirm={confirmPins}
            onRequest={() => setConfirmPins(true)}
            onCancel={() => setConfirmPins(false)}
            onConfirm={clearPins}
          />
          <LocalAction
            label="Chat threads"
            hint="Delete all local chat history."
            actionLabel="Clear"
            confirm={confirmThreads}
            onRequest={() => setConfirmThreads(true)}
            onCancel={() => setConfirmThreads(false)}
            onConfirm={() => void clearThreads()}
          />
          <LocalAction
            label="Preferences"
            hint="Reset density, model, and layout preferences."
            actionLabel="Reset"
            confirm={confirmReset}
            onRequest={() => setConfirmReset(true)}
            onCancel={() => setConfirmReset(false)}
            onConfirm={resetPrefs}
          />
        </CardContent>
      </Card>

      {/* Bootstrap an agent */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm font-semibold">Bootstrap an agent</CardTitle>
          <CardDescription className="text-xs">
            Copy one of these prompts and paste it into Claude Code, Cursor, Codex CLI, or the
            OpenAI Playground to connect an agent to this Stellarator instance.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <BootstrapPrompt />
        </CardContent>
      </Card>
    </PageContainer>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function Row({ label, hint, children }: { label: string; hint: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <p className="text-sm font-medium">{label}</p>
        <p className="text-xs text-muted-foreground">{hint}</p>
      </div>
      {children}
    </div>
  );
}

function LocalAction({
  label,
  hint,
  actionLabel,
  confirm,
  onRequest,
  onCancel,
  onConfirm,
}: {
  label: string;
  hint: string;
  actionLabel: string;
  confirm: boolean;
  onRequest: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="min-w-0">
        <p className="text-sm font-medium">{label}</p>
        <p className="text-xs text-muted-foreground">{hint}</p>
      </div>
      {confirm ? (
        <div className="flex gap-2">
          <Button size="sm" variant="ghost" className="h-7 text-xs" onClick={onCancel}>Cancel</Button>
          <Button size="sm" variant="destructive" className="h-7 text-xs" onClick={onConfirm}>Confirm</Button>
        </div>
      ) : (
        <Button size="sm" variant="outline" className="h-7 text-xs" onClick={onRequest}>
          {actionLabel}
        </Button>
      )}
    </div>
  );
}
