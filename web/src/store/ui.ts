import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Effort, ToolGroup, WorkspaceMode } from '../lib/api'

type Theme = 'dark' | 'light' | 'system'

/**
 * Chat and coding keep their own model and effort.
 *
 * They were shared, and the sharing was the bug: picking a 30B coding model in a
 * workspace also made it the model answering "what's the weather like", and
 * turning the effort down for a quick question quietly halved the tool budget of
 * the next coding turn. The two surfaces are used for different things, often in
 * the same minute, and neither should be able to reconfigure the other.
 */
interface UiState {
  theme: Theme
  setTheme: (theme: Theme) => void

  /** Model chosen in the chat composer; null means "let the server decide". */
  selectedModel: string | null
  setSelectedModel: (model: string | null) => void

  /** Model chosen on the Code page. Kept apart from the chat one. */
  codeModel: string | null
  setCodeModel: (model: string | null) => void

  effort: Effort
  setEffort: (effort: Effort) => void

  /** Coding starts higher: the first rounds of a coding turn are spent reading. */
  codeEffort: Effort
  setCodeEffort: (effort: Effort) => void

  /**
   * Whether the user has picked an effort on this surface themselves.
   *
   * Until they have, the composer follows the starting effort configured in
   * Settings. Without this flag the two settings did nothing at all: the store
   * shipped its own hardcoded defaults, persisted them on first load, and then
   * sent them with every message — so the server's configured default was
   * always overridden by a value the user had never chosen.
   *
   * Once they pick, their pick wins and stays won. A setting called "starting
   * effort" should not reach back and change a dial somebody has already turned.
   */
  effortChosen: boolean
  codeEffortChosen: boolean

  /** Apply the configured starting efforts to whichever dials are untouched. */
  seedEfforts: (chat: Effort, code: Effort) => void

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

  /**
   * Let a chat read the folders opened on the Code page.
   *
   * On by default. It reads only what the user themselves opened, it cannot
   * write, and the alternative — a chat that says it cannot see a project the
   * user is plainly looking at — is the behaviour this replaced.
   */
  projects: boolean
  setProjects: (enabled: boolean) => void

  /** Which panel the Code page is showing beside the conversation. */
  codePanel: 'files' | 'changes'
  setCodePanel: (panel: UiState['codePanel']) => void

  /** Whether the file panel is showing at all. */
  filesOpen: boolean
  setFilesOpen: (open: boolean) => void

  /**
   * Whether what-is-running is showing.
   *
   * Closed by default and opened by the page itself the moment something starts.
   * It used to be a third tab beside Files and Changes, which meant a dev server
   * could start, serve, and print a stack trace without anything on screen
   * changing — the only way to find out was to already suspect it and go
   * looking.
   */
  runningOpen: boolean
  setRunningOpen: (open: boolean) => void

  /** Panel widths, in pixels, as the user last dragged them. */
  filesWidth: number
  setFilesWidth: (width: number) => void
  runningWidth: number
  setRunningWidth: (width: number) => void

  /**
   * Half-written messages, by the conversation or workspace they belong to.
   *
   * The composer used to hold its text in local state, so it died with the
   * component: following "Tools" out of the menu — or clicking any other
   * conversation and coming back — threw the message away with no warning and
   * no undo. Typing is the one thing in this application the user cannot get
   * back, so it is the one thing that should outlive a render.
   *
   * Persisted with the rest of the store, which also means a draft survives a
   * reload. Empty drafts are dropped rather than stored, so this cannot grow a
   * key for every conversation ever opened.
   */
  drafts: Record<string, string>
  setDraft: (key: string, text: string) => void

  /** Last mode used, so a workspace reopens the way it was left. */
  lastCodeMode: WorkspaceMode | null
  setLastCodeMode: (mode: WorkspaceMode) => void

  sidebarOpen: boolean
  toggleSidebar: () => void
}

/** How far a panel may be dragged, before the layout stops making sense. */
export const PANEL_LIMITS = { min: 180, max: 640 } as const

/** Starting widths, and what a double-click on a divider restores. */
export const PANEL_DEFAULTS = { files: 260, running: 400 } as const

/** Keep a dragged width inside the limits. */
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

      filesWidth: PANEL_DEFAULTS.files,
      setFilesWidth: (width) => set({ filesWidth: clampPanel(width) }),

      runningWidth: PANEL_DEFAULTS.running,
      setRunningWidth: (width) => set({ runningWidth: clampPanel(width) }),

      drafts: {},
      setDraft: (key, text) =>
        set((state) => {
          const drafts = { ...state.drafts }
          // An empty draft is an absent one. Keeping the key would leave a
          // record of every conversation the user has ever typed in.
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
      // Bumped when `codePanel` lost its third value. Anyone whose browser
      // still holds `'preview'` would otherwise land on a Code page rendering
      // neither panel — a stored setting for a tab that no longer exists.
      version: 2,
      migrate: (persisted, version) => {
        const held = (persisted ?? {}) as Record<string, unknown>
        if (version >= 2) return held as unknown as UiState

        const panel = held.codePanel
        return {
          ...held,
          // Running is its own panel now, so somebody who left that tab open
          // gets it back as a panel rather than losing where they were.
          codePanel: panel === 'changes' ? 'changes' : 'files',
          runningOpen: panel === 'preview',
        } as unknown as UiState
      },
    },
  ),
)

/**
 * The tool groups the current switches add up to.
 *
 * There is no file switch here, and there is deliberately no way to add one. The
 * `projects` group reads folders the user opened on the Code page and cannot
 * write to any of them; writing lives on the Code page, inside a workspace, and
 * a chat has no folder to scope a write to.
 */
export function activeToolGroups(
  state: Pick<UiState, 'webSearch' | 'memory' | 'projects'>,
): ToolGroup[] {
  const groups: ToolGroup[] = []
  if (state.webSearch) groups.push('web')
  if (state.memory) groups.push('memory')
  if (state.projects) groups.push('projects')
  return groups
}

/** Reflect the theme choice on the document root, where the tokens read it. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement
  if (theme === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', theme)
}
