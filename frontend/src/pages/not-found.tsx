import { Link } from "react-router"
import { Page } from "@/components/shared/page"

export function NotFoundPage() {
  return (
    <Page title="Page not found">
      <p className="text-muted-foreground">
        Nothing lives at this address. <Link className="text-foreground underline" to="/runs">Go to runs</Link>.
      </p>
    </Page>
  )
}
