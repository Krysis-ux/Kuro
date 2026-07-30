import { useEffect } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Sidebar } from './components/Sidebar'
import { ChatPage } from './pages/Chat'
import { ModelsPage } from './pages/Models'
import { ProvidersPage } from './pages/Providers'
import { SettingsPage } from './pages/Settings'
import { ToolsPage } from './pages/Tools'
import { applyTheme, useUi } from './store/ui'

export function App() {
  const theme = useUi((state) => state.theme)

  useEffect(() => applyTheme(theme), [theme])

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <Routes>
          <Route path="/" element={<ChatPage />} />
          <Route path="/chat/:id" element={<ChatPage />} />
          <Route path="/models" element={<ModelsPage />} />
          <Route path="/tools" element={<ToolsPage />} />
          <Route path="/providers" element={<ProvidersPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          {/* The tools page absorbed the old MCP-only screen; keep the link working. */}
          <Route path="/mcp" element={<Navigate to="/tools" replace />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}
