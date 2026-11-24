import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Effort, ToolGroup, WorkspaceMode } from '../lib/api'

type Theme = 'dark' | 'light' | 'system'

interface UiState {
  theme: Theme
  setTheme: (theme: Theme) => void

  selectedModel: string | null
  setSelectedModel: (model: string | null) => void

  codeModel: string | null
  setCodeModel: (model: string | null) => void

  effort: Effort
  setEffort: (effort: Effort) => void

  codeEffort: Effort
  setCodeEffort: (effort: Effort) => void

  effortChosen: boolean
  codeEffortChosen: boolean

  seedEfforts: (chat: Effort, code: Effort) => void

  webSearch: boolean
  setWebSearch: (enabled: boolean) => void

  memory: boolean
  setMemory: (enabled: boolean) => void

  projects: boolean
  setProjects: (enabled: boolean) => void

  codePanel: 'files' | 'changes'
  setCodePanel: (panel: UiState['codePanel']) => void

  filesOpen: boolean
  setFilesOpen: (open: boolean) => void

  runningOpen: boolean
  setRunningOpen: (open: boolean) => void

  runningView: 'terminal' | 'browser'
  setRunningView: (view: UiState['runningView']) => void

  filesWidth: number
  setFilesWidth: (width: number) => void
  runningWidth: number
  setRunningWidth: (width: number) => void

  drafts: Record<string, string>
  setDraft: (key: string, text: string) => void

  lastCodeMode: WorkspaceMode | null
  setLastCodeMode: (mode: WorkspaceMode) => void

  sidebarOpen: boolean
  toggleSidebar: () => void
}

export const PANEL_LIMITS = { min: 180, max: 640 } as const

export const PANEL_DEFAULTS = { files: 260, running: 400 } as const

export function clampPanel(width: number): number {
  return Math.max(PANEL_LIMITS.min, Math.min(PANEL_LIMITS.max, Math.round(width)))
}

export const useUi = create<UiState>()(
  persist(
    (set) => ({
      theme: 'system',
      setTheme: (theme) => set({ theme }),

      selectedModel: null,
      setSelectedModel: (selectedModel) => set({ selectedModel }),

      codeModel: null,
      setCodeModel: (codeModel) => set({ codeModel }),

      effort: 'balanced',
      setEffort: (effort) => set({ effort, effortChosen: true }),

      codeEffort: 'high',
      setCodeEffort: (codeEffort) => set({ codeEffort, codeEffortChosen: true }),

      effortChosen: false,
      codeEffortChosen: false,
      seedEfforts: (chat, code) =>
        set((state) => ({
          effort: state.effortChosen ? state.effort : chat,
          codeEffort: state.codeEffortChosen ? state.codeEffort : code,
        })),

      webSearch: false,
      setWebSearch: (webSearch) => set({ webSearch }),

      memory: true,
      setMemory: (memory) => set({ memory }),

      projects: true,
      setProjects: (projects) => set({ projects }),

      codePanel: 'files',
      setCodePanel: (codePanel) => set({ codePanel }),

      filesOpen: true,
      setFilesOpen: (filesOpen) => set({ filesOpen }),

      runningOpen: false,
      setRunningOpen: (runningOpen) => set({ runningOpen }),

      runningView: 'terminal',
      setRunningView: (runningView) => set({ runningView }),

      filesWidth: PANEL_DEFAULTS.files,
      setFilesWidth: (width) => set({ filesWidth: clampPanel(width) }),

      runningWidth: PANEL_DEFAULTS.running,
      setRunningWidth: (width) => set({ runningWidth: clampPanel(width) }),

      drafts: {},
      setDraft: (key, text) =>
        set((state) => {
          const drafts = { ...state.drafts }
          if (text.trim() === '') delete drafts[key]
          else drafts[key] = text
          return { drafts }
        }),

      lastCodeMode: null,
      setLastCodeMode: (lastCodeMode) => set({ lastCodeMode }),

      sidebarOpen: true,
      toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
    }),
    {
      name: 'kuro-ui',
      version: 2,
      migrate: (persisted, version) => {
        const held = (persisted ?? {}) as Record<string, unknown>
        if (version >= 2) return held as unknown as UiState

        const panel = held.codePanel
        return {
          ...held,
          codePanel: panel === 'changes' ? 'changes' : 'files',
          runningOpen: panel === 'preview',
        } as unknown as UiState
      },
    },
  ),
)

export function activeToolGroups(
  state: Pick<UiState, 'webSearch' | 'memory' | 'projects'>,
): ToolGroup[] {
  const groups: ToolGroup[] = []
  if (state.webSearch) groups.push('web')
  if (state.memory) groups.push('memory')
  if (state.projects) groups.push('projects')
  return groups
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement
  if (theme === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', theme)
}
