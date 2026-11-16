import { Navigate, Route, Routes } from "react-router"
import { AppShell } from "@/components/app-shell"
import { EndpointsPage } from "@/pages/endpoints"
import { NotFoundPage } from "@/pages/not-found"
import { PreferenceReviewPage } from "@/pages/preference-review"
import { PreferencesPage } from "@/pages/preferences-list"
import { ImportPromptsPage } from "@/pages/prompt-import"
import { PromptSetDetailPage } from "@/pages/prompt-set-detail"
import { PromptSetsPage } from "@/pages/prompt-sets"
import { RunDetailPage } from "@/pages/run-detail"
import { NewRunPage } from "@/pages/run-new"
import { RunsListPage } from "@/pages/runs-list"
import { SettingsPage } from "@/pages/settings"

export default function App() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<Navigate to="/runs" replace />} />
        <Route path="runs" element={<RunsListPage />} />
        <Route path="runs/new" element={<NewRunPage />} />
        <Route path="runs/:runId" element={<RunDetailPage />} />
        <Route path="prompts" element={<PromptSetsPage />} />
        <Route path="prompts/import" element={<ImportPromptsPage />} />
        <Route path="prompts/:setId" element={<PromptSetDetailPage />} />
        <Route path="endpoints" element={<EndpointsPage />} />
        <Route path="preferences" element={<PreferencesPage />} />
        <Route path="preferences/:prefId" element={<PreferenceReviewPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  )
}
