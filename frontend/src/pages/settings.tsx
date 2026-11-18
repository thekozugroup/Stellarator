import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useTheme } from "@/lib/theme"
import { Eye, EyeOff, Loader2 } from "lucide-react"
import { toast } from "sonner"
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { CopyButton } from "@/components/shared/copy-button"
import { FormGroup, FormItem, FormRow } from "@/components/shared/form-group"
import { Page } from "@/components/shared/page"
import { ApiError, api } from "@/lib/api"
import type { Settings } from "@/lib/types"

export function SettingsPage() {
  const query = useQuery({ queryKey: ["settings"], queryFn: () => api.get<Settings>("/api/settings") }) // taste-ignore: missing-states -- settings always exist; loading shows Bar placeholders and errors an ErrorBlock
  return (
    <Page title="Settings">
      {query.isError ? (
        <ErrorBlock className="max-w-[40rem]" message={`Couldn’t load settings. ${query.error.message}`} onRetry={() => query.refetch()} />
      ) : (
        <div className="grid max-w-[40rem] gap-6">
          <HuggingFaceSection settings={query.data} />
          <StorageSection settings={query.data} />
          <AppearanceSection />
          <AboutSection settings={query.data} />
        </div>
      )}
    </Page>
  )
}

function HuggingFaceSection({ settings }: { settings?: Settings }) {
  const qc = useQueryClient()
  const [editing, setEditing] = useState(false)
  const [token, setToken] = useState("")
  const [show, setShow] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [confirmRemove, setConfirmRemove] = useState(false)

  const write = useMutation({
    mutationFn: (hf_token: string) => api.put<Settings>("/api/settings", { hf_token }),
    onSuccess: (s, sent) => {
      qc.setQueryData(["settings"], s)
      toast.success(sent ? "Hugging Face token saved." : "Hugging Face token removed.")
      setEditing(false)
      setToken("")
      setConfirmRemove(false)
    },
    onError: (e: Error) => {
      setError(e instanceof ApiError ? (e.fields.hf_token ?? e.message) : e.message)
      setConfirmRemove(false)
    },
  })

  const showForm = editing
  const linkClass =
    "inline-flex items-center rounded-sm text-foreground underline underline-offset-4 focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"

  return (
    <form
      noValidate
      onSubmit={(ev) => {
        ev.preventDefault()
        if (!showForm || write.isPending) return
        if (!token.trim()) {
          setError("Paste a token that starts with hf_.")
          document.getElementById("hf-token")?.focus()
          return
        }
        setError(null)
        write.mutate(token.trim())
      }}
    >
      <FormGroup
        title="Hugging Face"
        footnote={
          <>
            For gated datasets and Hub pushes.{" "}
            <a href="https://huggingface.co/settings/tokens" target="_blank" rel="noopener noreferrer" data-touch="inline" className={linkClass}>
              Create a token
              <span className="sr-only"> (opens in a new tab)</span>
            </a>
          </>
        }
      >
        <FormRow group label="Token">
          <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
            <div className="flex min-h-7 min-w-0 items-center pointer-coarse:min-h-0">
              {!settings ? (
                <Bar className="w-48" />
              ) : settings.hf_token_from_env ? (
                <span>
                  Set by the <span className="font-mono text-[13px]">HF_TOKEN</span> environment variable
                  {settings.hf_token_hint ? <span className="text-muted-foreground"> ({settings.hf_token_hint})</span> : null}
                  <span className="mt-0.5 block text-xs text-muted-foreground">Set where the container starts.</span>
                </span>
              ) : settings.hf_token_set ? (
                <span>
                  A token is saved <span className="font-mono text-[13px] text-muted-foreground">({settings.hf_token_hint})</span>
                </span>
              ) : (
                <span className="text-muted-foreground">No token</span>
              )}
            </div>
            {settings && !settings.hf_token_from_env && !showForm ? (
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => {
                    setError(null)
                    setEditing(true)
                    window.requestAnimationFrame(() => document.getElementById("hf-token")?.focus())
                  }}
                >
                  {settings.hf_token_set ? "Replace token" : "Add token"}
                </Button>
                {settings.hf_token_set ? (
                  <Button type="button" variant="ghost" onClick={() => setConfirmRemove(true)}>
                    Remove token
                  </Button>
                ) : null}
              </div>
            ) : null}
          </div>
          {!showForm && error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
        </FormRow>

        {showForm ? (
          <>
            <FormRow id="hf-token" label="Access token" error={error}>
              <div className="relative">
                <Input
                  id="hf-token"
                  name="hf-access-token"
                  type={show ? "text" : "password"}
                  value={token}
                  placeholder="hf_012345…"
                  autoComplete="off"
                  spellCheck={false}
                  autoCapitalize="none"
                  className="pr-10 font-mono pointer-fine:text-[13px]"
                  aria-invalid={!!error}
                  aria-describedby={error ? "hf-token-error" : undefined}
                  onChange={(e) => {
                    setToken(e.target.value)
                    setError(null)
                  }}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className="absolute top-0.5 right-0.5"
                  aria-label={show ? "Hide token" : "Show token"}
                  aria-pressed={show}
                  onClick={() => setShow((s) => !s)}
                >
                  {show ? <EyeOff aria-hidden="true" /> : <Eye aria-hidden="true" />}
                </Button>
              </div>
            </FormRow>
            <FormItem className="flex flex-wrap justify-end gap-2">
              <Button
                type="button"
                variant="outline"
                disabled={write.isPending}
                onClick={() => {
                  setEditing(false)
                  setToken("")
                  setError(null)
                }}
              >
                Cancel
              </Button>
              <Button type="submit" aria-disabled={write.isPending}>
                {write.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
                {write.isPending ? "Saving…" : "Save token"}
              </Button>
            </FormItem>
          </>
        ) : null}
      </FormGroup>

      <AlertDialog open={confirmRemove} onOpenChange={(o) => !write.isPending && setConfirmRemove(o)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove the saved token?</AlertDialogTitle>
            <AlertDialogDescription>
              Gated imports and Hub pushes stop working.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={write.isPending}>Cancel</AlertDialogCancel>
            <Button type="button" variant="destructive" disabled={write.isPending} onClick={() => write.mutate("")}>
              {write.isPending ? "Removing…" : "Remove token"}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </form>
  )
}

function StorageSection({ settings }: { settings?: Settings }) {
  return (
    <FormGroup title="Storage" footnote="Back up this folder to keep your data.">
      <FormRow group label="Data directory">
        {settings ? (
          <span className="flex min-w-0 items-center gap-0.5">
            <span title={settings.data_dir} className="min-w-0 truncate font-mono text-[13px]">
              {settings.data_dir}
            </span>
            <CopyButton value={settings.data_dir} label="Copy data directory" />
          </span>
        ) : (
          <Bar className="w-56" />
        )}
      </FormRow>
    </FormGroup>
  )
}

const THEMES = [
  ["light", "Light"],
  ["dark", "Dark"],
  ["system", "System"],
] as const

function AppearanceSection() {
  const { theme, setTheme } = useTheme()
  return (
    <FormGroup title="Appearance">
      <FormRow group label="Theme" align="end">
        <ToggleGroup
          type="single"
          variant="outline"
          spacing={0}
          value={theme ?? "system"}
          onValueChange={(v) => v && setTheme(v as (typeof THEMES)[number][0])}
          aria-label="Theme"
          className="w-full @md/fg:w-fit"
        >
          {THEMES.map(([value, label]) => (
            <ToggleGroupItem key={value} value={value} className="flex-1 px-3">
              {label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </FormRow>
    </FormGroup>
  )
}

const MODE: Record<Settings["mode"], string> = { desktop: "Desktop app", docker: "Docker", native: "Server" }

function AboutSection({ settings }: { settings?: Settings }) {
  return (
    <FormGroup title="About">
      <FormRow group label="Version">
        <span className="flex min-h-7 items-center">{settings ? <span className="font-mono text-[13px]">{settings.version}</span> : <Bar className="w-12" />}</span>
      </FormRow>
      {settings ? (
        <FormRow group label="Mode">
          <span className="flex min-h-7 items-center">{MODE[settings.mode]}</span>
        </FormRow>
      ) : null}
    </FormGroup>
  )
}
