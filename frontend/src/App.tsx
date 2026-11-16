import { lazy } from "react"
import { Navigate, Route, Routes } from "react-router"
import { AppShell } from "@/components/app-shell"

// Each screen is its own chunk, so the first load only fetches the screen you open.
const EndpointsPage = lazy(() => import("@/pages/endpoints").then((m) => ({ default: m.EndpointsPage })))
const NotFoundPage = lazy(() => import("@/pages/not-found").then((m) => ({ default: m.NotFoundPage })))
const PreferenceReviewPage = lazy(() => import("@/pages/preference-review").then((m) => ({ default: m.PreferenceReviewPage })))
const PreferencesPage = lazy(() => import("@/pages/preferences-list").then((m) => ({ default: m.PreferencesPage })))
const ImportPromptsPage = lazy(() => import("@/pages/prompt-import").then((m) => ({ default: m.ImportPromptsPage })))
const PromptSetDetailPage = lazy(() => import("@/pages/prompt-set-detail").then((m) => ({ default: m.PromptSetDetailPage })))
const PromptSetsPage = lazy(() => import("@/pages/prompt-sets").then((m) => ({ default: m.PromptSetsPage })))
const RunDetailPage = lazy(() => import("@/pages/run-detail").then((m) => ({ default: m.RunDetailPage })))
const NewRunPage = lazy(() => import("@/pages/run-new").then((m) => ({ default: m.NewRunPage })))
const RunsListPage = lazy(() => import("@/pages/runs-list").then((m) => ({ default: m.RunsListPage })))
const SettingsPage = lazy(() => import("@/pages/settings").then((m) => ({ default: m.SettingsPage })))

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
