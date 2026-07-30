import { useEffect } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Sidebar } from './components/Sidebar'
import { ChatPage } from './pages/Chat'
import { ModelsPage } from './pages/Models'
import { McpServersPage } from './pages/McpServers'
import { SettingsPage } from './pages/Settings'
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
          <Route path="/mcp" element={<McpServersPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </main>
    </div>
  )
}
