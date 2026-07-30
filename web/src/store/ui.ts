import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Effort } from '../lib/api'

type Theme = 'dark' | 'light' | 'system'

interface UiState {
  theme: Theme
  setTheme: (theme: Theme) => void

  /** Model chosen in the composer; null means "let the server decide". */
  selectedModel: string | null
  setSelectedModel: (model: string | null) => void

  effort: Effort
  setEffort: (effort: Effort) => void

  webSearch: boolean
  setWebSearch: (enabled: boolean) => void

  sidebarOpen: boolean
  toggleSidebar: () => void
}

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      theme: 'system',
      setTheme: (theme) => set({ theme }),

      selectedModel: null,
      setSelectedModel: (selectedModel) => set({ selectedModel }),

      effort: 'balanced',
      setEffort: (effort) => set({ effort }),

      webSearch: false,
      setWebSearch: (webSearch) => set({ webSearch }),

      sidebarOpen: true,
      toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
    }),
    { name: 'kuro-ui' },
  ),
)

/** Reflect the theme choice on the document root, where the tokens read it. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement
  if (theme === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', theme)
}
