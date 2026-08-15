import { useEffect } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ErrorBoundary } from './components/ErrorBoundary'
import { Sidebar } from './components/Sidebar'
import { api } from './lib/api'
import { ChatPage } from './pages/Chat'
import { CodePage } from './pages/Code'
import { CloudPage } from './pages/Cloud'
import { FreePage } from './pages/Free'
import { ModelsPage } from './pages/Models'
import { ProjectPage, ProjectsPage } from './pages/Projects'
import { ProvidersPage } from './pages/Providers'
import { SettingsPage } from './pages/Settings'
import { ToolsPage } from './pages/Tools'
import { applyTheme, useUi } from './store/ui'

export function App() {
  const theme = useUi((state) => state.theme)

  useEffect(() => applyTheme(theme), [theme])

  useConfiguredEfforts()

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <Routes>
          <Route path="/" element={<ErrorBoundary label="chat"><ChatPage /></ErrorBoundary>} />
          <Route path="/chat/:id" element={<ErrorBoundary label="chat"><ChatPage /></ErrorBoundary>} />
          <Route path="/code" element={<ErrorBoundary label="code"><CodePage /></ErrorBoundary>} />
          <Route path="/code/:id" element={<ErrorBoundary label="code"><CodePage /></ErrorBoundary>} />
          <Route path="/projects" element={<ErrorBoundary label="projects"><ProjectsPage /></ErrorBoundary>} />
          <Route path="/projects/:id" element={<ErrorBoundary label="projects"><ProjectPage /></ErrorBoundary>} />
          <Route path="/models" element={<ErrorBoundary label="models"><ModelsPage /></ErrorBoundary>} />
          <Route path="/free" element={<ErrorBoundary label="free"><FreePage /></ErrorBoundary>} />
          <Route path="/tools" element={<ErrorBoundary label="tools"><ToolsPage /></ErrorBoundary>} />
          <Route path="/providers" element={<ErrorBoundary label="providers"><ProvidersPage /></ErrorBoundary>} />
          <Route path="/cloud" element={<ErrorBoundary label="cloud"><CloudPage /></ErrorBoundary>} />
          <Route path="/settings" element={<ErrorBoundary label="settings"><SettingsPage /></ErrorBoundary>} />
          {/* The tools page absorbed the old MCP-only screen; keep the link working. */}
          <Route path="/mcp" element={<Navigate to="/tools" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}

function useConfiguredEfforts() {
  const seedEfforts = useUi((state) => state.seedEfforts)
  const settings = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })

  const chat = settings.data?.surfaces.chat.defaultEffort
  const code = settings.data?.surfaces.code.defaultEffort

  useEffect(() => {
    if (chat && code) seedEfforts(chat, code)
  }, [chat, code, seedEfforts])
}
