import { useRef, useState, type FormEvent } from "react"
import { Link, useNavigate } from "react-router"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { Download, MoreHorizontal, Pause, Play, RotateCcw } from "lucide-react"
import { toast } from "sonner"
import { Field } from "@/components/shared/field"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { ApiError, api } from "@/lib/api"
import { count, int } from "@/lib/format"
import type { Run, RunLive } from "@/lib/types"
import { runKeys } from "./run-data"

function useRunUpdate(run: Run) {
  const queryClient = useQueryClient()
  return (updated: Run) => {
    queryClient.setQueryData<RunLive>(runKeys.live(run.id), (old) => (old ? { ...old, run: updated } : old))
    void queryClient.invalidateQueries({ queryKey: runKeys.live(run.id) })
    void queryClient.invalidateQueries({ queryKey: runKeys.list })
  }
}

/** Dialogs opened from the menu give focus back to the menu button when they close. */
type ReturnFocus = { returnFocus: () => void }

function RenameDialog({ run, open, onOpenChange, returnFocus }: { run: Run; open: boolean; onOpenChange: (o: boolean) => void } & ReturnFocus) {
  const [name, setName] = useState(run.name)
  const [error, setError] = useState<string | null>(null)
  const update = useRunUpdate(run)
  const rename = useMutation({
    mutationFn: (n: string) => api.patch<Run>(`/api/runs/${run.id}`, { name: n }),
    onSuccess: (r) => {
      update(r)
      onOpenChange(false)
      toast.success("Run renamed")
    },
    onError: (e: Error) => setError(e instanceof ApiError ? (e.fields.name ?? e.message) : e.message),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (rename.isPending) return
    const n = name.trim()
    if (!n) {
      setError("Enter a name.")
      document.getElementById("rename-run")?.focus()
      return
    }
    rename.mutate(n)
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (o) {
          setName(run.name)
          setError(null)
        }
        onOpenChange(o)
      }}
    >
      <DialogContent
        className="sm:max-w-md"
        onCloseAutoFocus={(e) => {
          e.preventDefault()
          returnFocus()
        }}
      >
        <form onSubmit={submit} noValidate className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Rename run</DialogTitle>
          </DialogHeader>
          <Field id="rename-run" label="Name" error={error}>
            <Input
              id="rename-run"
              value={name}
              maxLength={300}
              onChange={(e) => {
                setName(e.target.value)
                setError(null)
              }}
              aria-invalid={!!error || undefined}
              aria-describedby={error ? "rename-run-error" : undefined}
              className="pointer-coarse:h-11"
            />
          </Field>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" aria-disabled={rename.isPending || undefined}>
              {rename.isPending ? "Saving…" : "Save name"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function DeleteDialog({ run, open, onOpenChange, returnFocus }: { run: Run; open: boolean; onOpenChange: (o: boolean) => void } & ReturnFocus) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const del = useMutation({
    mutationFn: () => api.del(`/api/runs/${run.id}`),
    onSuccess: () => {
      queryClient.removeQueries({ queryKey: ["runs", run.id] })
      void queryClient.invalidateQueries({ queryKey: runKeys.list })
      toast.success(`Deleted “${run.name}”`)
      navigate("/runs")
    },
  })
  return (
    <AlertDialog open={open} onOpenChange={(o) => !del.isPending && onOpenChange(o)}>
      <AlertDialogContent
        onCloseAutoFocus={(e) => {
          e.preventDefault()
          returnFocus()
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="break-words">
            Delete run “{run.name}” and its {count(run.total, "sample", "samples")}?
          </AlertDialogTitle>
          <AlertDialogDescription>Responses and thinking traces are deleted for good. Exported files are not affected.</AlertDialogDescription>
        </AlertDialogHeader>
        {del.isError ? (
          <p role="alert" className="text-sm text-destructive">
            Couldn't delete the run. {del.error.message}
          </p>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={del.isPending}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={(e) => {
              e.preventDefault()
              if (!del.isPending) del.mutate()
            }}
          >
            {del.isPending ? "Deleting…" : "Delete run"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

/**
 * Pause or resume, retry failures, export, and the rarer actions in a menu. The one near-black
 * button follows the state: Resume on a paused run, Export once a finished run has records to
 * export, Retry failed when it has none. A running run has no primary.
 */
export function RunActions({ run, onExport }: { run: Run; onExport: () => void }) {
  const update = useRunUpdate(run)
  const [renameOpen, setRenameOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const menuButton = useRef<HTMLButtonElement>(null)
  const returnFocus = () => menuButton.current?.focus()

  const action = useMutation({
    mutationFn: (what: "pause" | "resume" | "retry-failed") => api.post<Run>(`/api/runs/${run.id}/${what}`),
    onSuccess: (r, what) => {
      update(r)
      if (what === "retry-failed") toast.success(`Retrying ${count(run.failed, "failed sample", "failed samples")}`)
    },
    onError: (e: Error, what) => {
      const verb = what === "pause" ? "pause" : what === "resume" ? "resume" : "retry the failed samples of"
      toast.error(`Couldn't ${verb} the run. ${e.message}`)
    },
  })
  const busy = action.isPending ? action.variables : null

  const paused = run.status === "paused"
  const completed = run.status === "completed"
  const exportable = run.done > 0
  const canRetry = run.failed > 0 && (paused || completed)
  const primary = paused ? "resume" : completed ? (exportable ? "export" : canRetry ? "retry" : null) : null

  return (
    <>
      {canRetry ? (
        <Button
          variant={primary === "retry" ? "default" : "outline"}
          disabled={busy === "retry-failed"}
          onClick={() => action.mutate("retry-failed")}
          className="pointer-coarse:h-11"
        >
          {/* Without its icon on phones, so a paused run's actions and menu fit one row at 390 px. */}
          <RotateCcw aria-hidden="true" className="max-sm:hidden" />
          {busy === "retry-failed" ? "Retrying…" : paused ? "Retry failed and resume" : `Retry failed (${int(run.failed)})`}
        </Button>
      ) : null}
      {exportable ? (
        <Button variant={primary === "export" ? "default" : "outline"} onClick={onExport} className="pointer-coarse:h-11">
          <Download aria-hidden="true" />
          Export dataset…
        </Button>
      ) : null}
      {run.status === "running" ? (
        <Button variant="outline" disabled={busy === "pause"} onClick={() => action.mutate("pause")} className="pointer-coarse:h-11">
          <Pause aria-hidden="true" />
          {busy === "pause" ? "Pausing…" : "Pause run"}
        </Button>
      ) : run.status === "pausing" ? (
        <Button variant="outline" disabled className="pointer-coarse:h-11">
          <Pause aria-hidden="true" />
          Pausing…
        </Button>
      ) : paused ? (
        <Button disabled={busy === "resume"} onClick={() => action.mutate("resume")} className="pointer-coarse:h-11">
          <Play aria-hidden="true" />
          {busy === "resume" ? "Resuming…" : "Resume run"}
        </Button>
      ) : null}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button ref={menuButton} variant="ghost" size="icon" aria-label="More actions for this run" className="pointer-coarse:size-11">
            <MoreHorizontal aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-44">
          <DropdownMenuItem asChild>
            <Link to={`/runs/new?from=${run.id}`}>Run again…</Link>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => setRenameOpen(true)}>Rename…</DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onSelect={() => setDeleteOpen(true)}>
            Delete run…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <RenameDialog run={run} open={renameOpen} onOpenChange={setRenameOpen} returnFocus={returnFocus} />
      <DeleteDialog run={run} open={deleteOpen} onOpenChange={setDeleteOpen} returnFocus={returnFocus} />
    </>
  )
}
