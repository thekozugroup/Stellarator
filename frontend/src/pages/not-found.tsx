import type { ReactNode } from "react"
import { Link } from "react-router"
import { Page } from "@/components/shared/page"
import { Plasma } from "@/components/shared/plasma"
import { Button } from "@/components/ui/button"

/**
 * A quiet empty state for an address with nothing behind it: a pixel tile, one sentence and the
 * way back. Also used by a detail screen whose object is gone (a deleted run).
 */
export function Missing({ seed, to, action, children }: { seed: string; to: string; action: string; children: ReactNode }) {
  return (
    <div className="flex max-w-[40rem] flex-col items-start gap-4 rounded-xl bg-muted p-6 text-sm text-muted-foreground">
      <Plasma seed={seed} cells={8} className="size-14 rounded-md" />
      <p className="max-w-[60ch]">{children}</p>
      <Button asChild variant="secondary" className="bg-background hover:bg-background/80">
        <Link to={to}>{action}</Link>
      </Button>
    </div>
  )
}

export function NotFoundPage() {
  return (
    <Page title="Page not found">
      <Missing seed="not-found" to="/runs" action="Go to runs">
        Nothing lives at this address.
      </Missing>
    </Page>
  )
}
