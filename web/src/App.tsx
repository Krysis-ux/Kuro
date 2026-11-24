import { lazy, Suspense, useEffect, useRef } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ErrorBoundary } from './components/ErrorBoundary'
import { Sidebar } from './components/Sidebar'
import { api } from './lib/api'
import { ChatPage } from './pages/Chat'
import { CodePage } from './pages/Code'
import { ModelsPage } from './pages/Models'
import { ProjectPage, ProjectsPage } from './pages/Projects'
import { SettingsPage, SettingsSidebar } from './pages/Settings'
import { applyTheme, useUi } from './store/ui'

// Loaded on first visit: the model-card renderer parses and sanitizes HTML,
// which no other page needs.
const ModelDetailPage = lazy(() =>
  import('./pages/ModelDetail').then((module) => ({ default: module.ModelDetailPage })),
)

export function App() {
  const theme = useUi((state) => state.theme)
  const location = useLocation()
  const navigate = useNavigate()
  const inSettings = location.pathname.startsWith('/settings')

  // Where "Back to app" returns to: the last page that was not Settings.
  const lastAppPath = useRef('/')
  useEffect(() => {
    if (!inSettings) lastAppPath.current = location.pathname + location.search
  }, [inSettings, location.pathname, location.search])

  useEffect(() => applyTheme(theme), [theme])

  useConfiguredEfforts()

  return (
    <div className="app">
      {inSettings ? <SettingsSidebar onBack={() => navigate(lastAppPath.current)} /> : <Sidebar />}
      <main className="main">
        <Routes>
          <Route path="/" element={<ErrorBoundary label="chat"><ChatPage /></ErrorBoundary>} />
          <Route path="/chat/:id" element={<ErrorBoundary label="chat"><ChatPage /></ErrorBoundary>} />
          <Route path="/code" element={<ErrorBoundary label="code"><CodePage /></ErrorBoundary>} />
          <Route path="/code/:id" element={<ErrorBoundary label="code"><CodePage /></ErrorBoundary>} />
          <Route path="/projects" element={<ErrorBoundary label="projects"><ProjectsPage /></ErrorBoundary>} />
          <Route path="/projects/:id" element={<ErrorBoundary label="projects"><ProjectPage /></ErrorBoundary>} />
          <Route path="/models" element={<ErrorBoundary label="models"><ModelsPage /></ErrorBoundary>} />
          <Route path="/models/:owner/:name" element={<ErrorBoundary label="model"><Suspense><ModelDetailPage /></Suspense></ErrorBoundary>} />
          <Route path="/settings/:pane" element={<ErrorBoundary label="settings"><SettingsPage /></ErrorBoundary>} />
          <Route path="/settings" element={<Navigate to="/settings/general" replace />} />
          {/* Pages that folded into Settings; old links still land somewhere sensible. */}
          <Route path="/free" element={<Navigate to="/settings/providers?show=free" replace />} />
          <Route path="/providers" element={<Navigate to="/settings/providers" replace />} />
          <Route path="/cloud" element={<Navigate to="/settings/providers?show=add" replace />} />
          <Route path="/tools" element={<Navigate to="/settings/skills" replace />} />
          <Route path="/mcp" element={<Navigate to="/settings/mcp" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}

/**
 * Start the two effort dials where Settings says they should start.
 *
 * The server has always held these — `chat.defaultEffort` and
 * `code.defaultEffort` — and until now nothing read them: the composer sent its
 * own persisted value with every message, so the server's fallback never
 * applied and the two controls in Settings changed nothing anybody could see.
 *
 * Seeded once, here, rather than in each composer, so both surfaces get it and
 * a page that gains a composer later does too. A dial the user has turned is
 * left alone — see `effortChosen` in the store.
 */
function useConfiguredEfforts() {
  const seedEfforts = useUi((state) => state.seedEfforts)
  const settings = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })

  const chat = settings.data?.surfaces.chat.defaultEffort
  const code = settings.data?.surfaces.code.defaultEffort

  useEffect(() => {
    if (chat && code) seedEfforts(chat, code)
  }, [chat, code, seedEfforts])
}
