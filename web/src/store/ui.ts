import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Effort, ToolGroup } from '../lib/api'

type Theme = 'dark' | 'light' | 'system'

interface UiState {
  theme: Theme
  setTheme: (theme: Theme) => void

  /** Model chosen in the composer; null means "let the server decide". */
  selectedModel: string | null
  setSelectedModel: (model: string | null) => void

  effort: Effort
  setEffort: (effort: Effort) => void

  /**
   * Search the web before answering.
   *
   * Off by default and never turned on automatically: switching it on is the
   * moment a question leaves the machine, which is the user's decision to make.
   */
  webSearch: boolean
  setWebSearch: (enabled: boolean) => void

  /**
   * Read and write durable facts.
   *
   * On by default, unlike web search, because memory only ever touches what the
   * user themselves asked to be saved and never leaves the machine.
   */
  memory: boolean
  setMemory: (enabled: boolean) => void

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

      memory: true,
      setMemory: (memory) => set({ memory }),

      sidebarOpen: true,
      toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
    }),
    { name: 'kuro-ui' },
  ),
)

/** The tool groups the current switches add up to. */
export function activeToolGroups(state: Pick<UiState, 'webSearch' | 'memory'>): ToolGroup[] {
  const groups: ToolGroup[] = []
  if (state.webSearch) groups.push('web')
  if (state.memory) groups.push('memory')
  return groups
}

/** Reflect the theme choice on the document root, where the tokens read it. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement
  if (theme === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', theme)
}
