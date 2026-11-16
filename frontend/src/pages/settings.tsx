import { useState, type ReactNode } from "react"
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
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { Bar, ErrorBlock } from "@/components/shared/block-states"
import { CopyButton } from "@/components/shared/copy-button"
import { Field } from "@/components/shared/field"
import { Page } from "@/components/shared/page"
import { ApiError, api } from "@/lib/api"
import type { Settings } from "@/lib/types"

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-labelledby={`s-${title}`} className="grid max-w-[40rem] gap-3 border-t py-6 first:border-t-0 first:pt-0">
      <h2 id={`s-${title}`} className="text-base font-semibold">
        {title}
      </h2>
      {children}
    </section>
  )
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-0.5 sm:grid-cols-[9rem_1fr] sm:gap-4">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  )
}

export function SettingsPage() {
  const query = useQuery({ queryKey: ["settings"], queryFn: () => api.get<Settings>("/api/settings") })
  return (
    <Page title="Settings">
      {query.isError ? (
        <ErrorBlock className="max-w-[40rem]" message={`Couldn’t load settings. ${query.error.message}`} onRetry={() => query.refetch()} />
      ) : (
        <div>
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

  return (
    <Section title="Hugging Face">
      <dl className="grid gap-2 text-sm">
        <Row label="Token">
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
        </Row>
      </dl>

      {settings && !settings.hf_token_from_env && !showForm ? (
        <div className="flex flex-wrap gap-2">
          <Button
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
            <Button variant="ghost" onClick={() => setConfirmRemove(true)}>
              Remove token
            </Button>
          ) : null}
        </div>
      ) : null}

      {showForm ? (
        <form
          noValidate
          className="grid gap-3"
          onSubmit={(ev) => {
            ev.preventDefault()
            if (write.isPending) return
            if (!token.trim()) {
              setError("Paste a token that starts with hf_.")
              document.getElementById("hf-token")?.focus()
              return
            }
            setError(null)
            write.mutate(token.trim())
          }}
        >
          <Field id="hf-token" label="Access token" error={error}>
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
                className="pr-10 font-mono text-[13px]"
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
          </Field>
          <div className="flex gap-2">
            <Button type="submit" aria-disabled={write.isPending}>
              {write.isPending ? <Loader2 className="animate-spin" aria-hidden="true" /> : null}
              {write.isPending ? "Saving…" : "Save token"}
            </Button>
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
          </div>
        </form>
      ) : error ? (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      ) : null}

      <p className="text-xs text-muted-foreground">
        For gated datasets and Hub pushes.{" "}
        <a
          href="https://huggingface.co/settings/tokens"
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex min-h-6 items-center rounded-sm text-foreground underline underline-offset-4 focus-visible:ring-3 focus-visible:ring-ring focus-visible:outline-none"
        >
          Create a token
        </a>
      </p>

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
            <Button variant="destructive" disabled={write.isPending} onClick={() => write.mutate("")}>
              {write.isPending ? "Removing…" : "Remove token"}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Section>
  )
}

function StorageSection({ settings }: { settings?: Settings }) {
  return (
    <Section title="Storage">
      <dl className="grid gap-2 text-sm">
        <Row label="Data directory">
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
        </Row>
      </dl>
      <p className="text-xs text-muted-foreground">Back up this folder to keep your data.</p>
    </Section>
  )
}

function AppearanceSection() {
  const { theme, setTheme } = useTheme()
  return (
    <Section title="Appearance">
      <fieldset className="grid gap-1.5">
        <legend className="sr-only">Theme</legend>
        <RadioGroup value={theme ?? "system"} onValueChange={setTheme} className="grid-cols-1 gap-1 sm:max-w-md sm:grid-cols-3">
          {(
            [
              ["light", "Light"],
              ["dark", "Dark"],
              ["system", "System"],
            ] as const
          ).map(([value, label]) => (
            <Label key={value} htmlFor={`theme-${value}`} className="flex min-h-9 items-center gap-2.5 rounded-md border px-3 text-sm font-normal">
              <RadioGroupItem id={`theme-${value}`} value={value} />
              {label}
            </Label>
          ))}
        </RadioGroup>
      </fieldset>
    </Section>
  )
}

function AboutSection({ settings }: { settings?: Settings }) {
  return (
    <Section title="About">
      <dl className="grid gap-2 text-sm">
        <Row label="Version">{settings ? <span className="font-mono text-[13px]">{settings.version}</span> : <Bar className="w-12" />}</Row>
      </dl>
    </Section>
  )
}
