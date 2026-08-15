
export type ModelStatus = 'downloading' | 'ready' | 'error'
export type FitVerdict = 'great' | 'fits' | 'tight' | 'wont_fit'

export interface FitEstimate {
  verdict: FitVerdict
  label: string
  estimated_required_bytes: number
  usable_bytes: number
  note: string
}

export interface ModelRecord {
  id: string
  display_name: string
  source: 'curated' | 'huggingface' | 'local'
  hf_repo: string | null
  hf_file: string | null
  quant: string | null
  param_count: string | null
  family: string | null
  capabilities: string[]
  context_length: number | null
  file_path: string | null
  file_size_bytes: number | null
  sha256: string | null
  status: ModelStatus
  error: string | null
  added_at: string
  last_used_at: string | null
}

export interface InstalledModel {
  model: ModelRecord
  loaded: boolean
  fit: FitEstimate | null
  kind: string
  chat: boolean
}

export interface RemoteModel {
  id: string
  name: string
  connector_id: string
  connector_label: string
  provider: string
  pooled: boolean
  pool_size: number
  free: boolean
  specialities: string[]
  params_b: number | null
  unavailable: string | null
  fix_url: string | null
}

export interface HubModel {
  repo: string
  name: string
  owner: string
  downloads: number
  likes: number
  last_modified: string | null
  quants: string[]
  param_count: string | null
  gated: boolean
  split_only: boolean
  fit: FitEstimate | null
  installed: boolean
}

/* ---------- Tools ---------- */

export type ToolGroup = 'web' | 'memory' | 'projects'

export interface ToolGroupInfo {
  id: ToolGroup
  label: string
  blurb: string
}

export interface BuiltinTool {
  name: string
  description: string
  group: ToolGroup
}

export interface SearchProviderOption {
  id: string
  name: string
  note: string
  needsApiKey: boolean
  needsBaseUrl: boolean
  credentialsUrl: string | null
}

export type SkillCategory = 'language' | 'coding' | 'practice' | 'design' | 'writing'

export interface Skill {
  slug: string
  name: string
  blurb: string
  category: SkillCategory
  instructions: string
  approx_tokens: number
}

export interface EssentialSkill {
  slug: string
  name: string
  blurb: string
}

export interface SurfaceSettings {
  autoOrchestrate: boolean
  defaultEffort: Effort
  defaultMode?: WorkspaceMode
}

export interface SkillBudget {
  effort: string
  tokens: number
}

export interface CustomSkill {
  slug: string
  name: string
  blurb: string
  category: string
  approxTokens: number
  source: string
  updatedAt: string
}

export interface SkillImport {
  repo: string
  added: string[]
  skipped: string[]
  needsScripts: string[]
}

export interface ToolsOverview {
  builtins: BuiltinTool[]
  defaultGroups: ToolGroup[]
  groups: ToolGroupInfo[]
  skills: {
    catalogue: Skill[]
    enabled: string[]
    approxTokens: number
    budgets: {
      chat: SkillBudget[]
      code: SkillBudget[]
    }
    essentials: EssentialSkill[]
    custom: CustomSkill[]
  }
  surfaces: { chat: SurfaceSettings; code: SurfaceSettings }
  memory: { preload: boolean; count: number }
  search: {
    provider: string
    baseUrl: string | null
    hasApiKey: boolean
    needsApiKey: boolean
    needsBaseUrl: boolean
    providers: SearchProviderOption[]
  }
}

export interface SearchResult {
  title: string
  url: string
  snippet: string
}

export interface MemoryRecord {
  id: string
  content: string
  tags: string[]
  source: string | null
  created_at: string
}

/* ---------- MCP ---------- */

export type McpTransport = 'stdio' | 'http'
export type McpStatus = 'connected' | 'disconnected' | 'error'

export interface ExposedTool {
  name: string
  remote_name: string
  description: string
}

export interface McpServer {
  id: string
  name: string
  transport: McpTransport
  command: string | null
  args: string[]
  env: Record<string, unknown>
  url: string | null
  headers: Record<string, unknown>
  enabled: boolean
  status: McpStatus
  last_error: string | null
  slug: string | null
  tool_count: number | null
  created_at: string
  tools: ExposedTool[]
  has_auth: boolean
}

export type McpRequirement = 'none' | 'api_key' | 'local_runtime'

export interface McpRegistryEntry {
  slug: string
  name: string
  blurb: string
  detail: string
  transport: McpTransport
  url: string | null
  command: string | null
  args: string[]
  requirement: McpRequirement
  credentials_url: string | null
  homepage: string
  recommended: boolean
  installed: boolean
}

export interface ConnectionResult {
  ok: boolean
  toolCount?: number
  error?: string
}

/* ---------- Providers ---------- */

export type ProviderStatus = 'untested' | 'ok' | 'error'
export type PresetKind = 'aggregator' | 'first_party' | 'rented_gpu' | 'custom'

export type Surface = 'provider' | 'cloud'

export interface ProviderPreset {
  slug: string
  name: string
  base_url: string
  blurb: string
  kind: PresetKind
  credentials_url: string | null
  key_hint: string | null
  needs_url: boolean
  surface: Surface
}

export interface Provider {
  id: string
  provider: string
  label: string
  base_url: string
  status: ProviderStatus
  last_tested_at: string | null
  last_error: string | null
  enabled: boolean
  models: string[]
  created_at: string
  hasKey: boolean
  surface: Surface
}

/* ---------- Projects ---------- */

export interface Project {
  id: string
  name: string
  description: string
  instructions: string
  model_id: string | null
  tool_groups: string[] | null
  created_at: string
  updated_at: string
  conversation_count: number
}

/* ---------- Coding workspaces ---------- */

export type WorkspaceMode = 'ask' | 'plan' | 'agent' | 'bypass'

export interface Workspace {
  id: string
  name: string
  root_path: string
  model_id: string | null
  mode: WorkspaceMode
  created_at: string
  updated_at: string
  root_exists: boolean
  conversation_count: number
}

export interface WorkspaceModeInfo {
  id: WorkspaceMode
  label: string
  blurb: string
  tools: string[]
}

export type ToolRisk = 'read' | 'write' | 'execute'

export interface CodingTool {
  name: string
  description: string
  risk: ToolRisk
  risk_label: string
}

/* ---------- Long-running processes ---------- */

export interface RunningProcess {
  id: string
  workspace_id: string
  command: string
  pid: number | null
  started_at: string
  url: string | null
  running: boolean
  exit_code: number | null
}

/* ---------- Browsing this machine's folders ---------- */

export interface FolderEntry {
  name: string
  path: string
  has_children: boolean
}

export interface FolderListing {
  path: string
  name: string
  parent: string | null
  entries: FolderEntry[]
  truncated: boolean
  shortcuts: { label: string; path: string }[]
}

/* ---------- Free-tier pool ---------- */

export interface FreeProvider {
  slug: string
  name: string
  baseUrl: string
  credentialsUrl: string
  allowance: string
  keyHint: string | null
  models: string[]
  hasKey: boolean
  trouble: 'rate_limited' | 'rejected' | 'model_gone' | null
  tier: 'recurring' | 'keyless' | 'starter_credit' | 'expiring_trial'
  keyless: boolean
  privacy: { logged: boolean; trains: boolean }
  expired: boolean
  limit: FreeLimit | null
}

export interface FreeLimit {
  tokensPerDay?: number
  tokensPerMonth?: number
}

export interface FreeProviderUsage {
  providerSlug: string
  name: string
  turns: number
  promptTokens: number
  completionTokens: number
  totalTokens: number
  unreportedTurns: number
  limit: FreeLimit | null
}

export interface FreeWindow {
  from: string
  to: string
  providers: FreeProviderUsage[]
  turns: number
  unreportedTurns: number
  totalTokens: number
}

export interface FreeUsage {
  day: FreeWindow
  month: FreeWindow
  averages: {
    tokensPerTurn: number | null
    tokensPerDayThisMonth: number | null
  }
}

export interface FreeFlavour {
  id: string
  flavour: string
  label: string
  blurb: string
  available: boolean
}

export interface FreeOverview {
  providers: FreeProvider[]
  flavours: FreeFlavour[]
  keyCount: number
  availableCount: number
  allowKeyless: boolean
}

export interface WorkspaceChange {
  id: string
  path: string
  kind: 'edit' | 'write'
  conversationId: string | null
  createdAt: string
  undone: boolean
  undoable: boolean
  created: boolean
  beforeLines: number | null
  afterLines: number | null
}

export type Purpose = 'coding' | 'vision' | 'audio' | 'text' | 'all_round'

export interface PurposeInfo {
  id: Purpose
  label: string
  blurb: string
}

export interface RecommendedModel {
  id: string
  slug: string
  displayName: string
  repo: string
  defaultQuant: string
  quants: string[]
  paramCount: string
  family: string
  capabilities: string[]
  purposes: Purpose[]
  primaryPurpose: Purpose
  contextLength: number
  approxSizeBytes: number
  blurb: string
  installed: boolean
  status: ModelStatus | null
  fit: FitEstimate
}

export interface LoadedEngine {
  model_id: string
  port: number
  pid: number
  loaded_at: string
  idle_seconds: number
}

export interface DownloadRecord {
  id: string
  kind: 'model' | 'engine_binary'
  target_id: string
  label: string
  total_bytes: number | null
  downloaded_bytes: number
  status: 'queued' | 'downloading' | 'paused' | 'verifying' | 'completed' | 'failed' | 'cancelled'
  error: string | null
  started_at: string
  updated_at: string
}

export interface Conversation {
  id: string
  title: string
  title_mode: string
  model_id: string | null
  pinned: boolean
  archived: boolean
  created_at: string
  updated_at: string
  forked_from_id: string | null
}

export const OPTIMISTIC_ID_PREFIX = 'optimistic-'

export function isOptimistic(messageId: string): boolean {
  return messageId.startsWith(OPTIMISTIC_ID_PREFIX)
}

export interface Message {
  id: string
  conversation_id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  reasoning_content: string | null
  tool_calls:
    | { name: string; ok: boolean; preview: string; arguments?: Record<string, unknown> }[]
    | null
  used_web_search: boolean
  web_sources: { title: string; url: string }[] | null
  model_id: string | null
  usage_prompt_tokens: number | null
  usage_completion_tokens: number | null
  timing_ttft_ms: number | null
  timing_total_ms: number | null
  timing_tokens_per_sec: number | null
  finish_reason: string | null
  created_at: string
}

export interface HardwareInfo {
  os: string
  arch: string
  chip: string | null
  total_memory_bytes: number
  physical_cores: number
  logical_cores: number
  gpu_available: boolean
  gpu_backend: string
  recommended: { context_size: number; gpu_layers: number; threads: number }
}

export interface ServerStatus {
  name: string
  version: string
  status: string
  host: string
  port: number
  address: string
  uptimeSeconds: number
  startedAt: string
  dataDirectory: string
  loadedModels: LoadedEngine[]
}

export interface PullPreview {
  id: string
  displayName: string
  repo: string
  file: string
  quant: string | null
  sizeBytes: number
  verifiable: boolean
  fit: FitEstimate
}

export type Effort = 'low' | 'balanced' | 'high' | 'max' | 'ultra'

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...init?.headers },
  })

  const text = await response.text()

  if (!response.ok) {
    let message = `Request failed (${response.status})`
    try {
      const parsed = JSON.parse(text)
      if (typeof parsed?.error?.message === 'string') message = parsed.error.message
    } catch {
      if (text.trim()) message = text.trim()
    }
    throw new ApiError(message, response.status)
  }

  return text.trim() ? (JSON.parse(text) as T) : (undefined as T)
}

const post = <T>(path: string, body?: unknown) =>
  request<T>(path, { method: 'POST', body: JSON.stringify(body ?? {}) })

export const api = {
  status: () => request<ServerStatus>('/api/status'),
  shutdown: () => post<{ stopping: boolean; unloadingEngines: number }>('/api/shutdown'),
  restart: () => post<{ restarting: boolean; port: number }>('/api/restart'),
  waitUntilHealthy: async (timeoutMs = 30_000): Promise<void> => {
    const deadline = Date.now() + timeoutMs
    while (Date.now() < deadline) {
      try {
        const response = await fetch('/api/health', { cache: 'no-store' })
        if (response.ok) return
      } catch {
      }
      await new Promise((resolve) => setTimeout(resolve, 400))
    }
    throw new Error('Kuro did not come back. Check the terminal it was started from.')
  },
  hardware: () =>
    request<{ hardware: HardwareInfo; effectiveEngineSettings: Record<string, number> }>(
      '/api/hardware',
    ),

  models: {
    list: () =>
      request<{ models: InstalledModel[]; remote: RemoteModel[] }>('/api/models'),
    recommended: () =>
      request<{ models: RecommendedModel[]; purposes: PurposeInfo[] }>(
        '/api/models/recommended',
      ),
    searchHub: (query: string, limit?: number) => {
      const params = new URLSearchParams()
      if (query.trim()) params.set('q', query.trim())
      if (limit) params.set('limit', String(limit))
      const suffix = params.toString()
      return request<{ models: HubModel[] }>(
        `/api/models/search${suffix ? `?${suffix}` : ''}`,
      )
    },
    loaded: () => request<{ loaded: LoadedEngine[] }>('/api/models/loaded'),
    preview: (model: string) => post<PullPreview>('/api/models/preview', { model }),
    pull: (model: string) => post<{ downloadId: string }>('/api/models/pull', { model }),
    remove: (id: string) => request<void>(`/api/models/${encodeURIComponent(id)}`, { method: 'DELETE' }),
    load: (id: string) => post<{ port: number }>(`/api/models/${encodeURIComponent(id)}/load`),
    unload: (id: string) => post<{ unloaded: boolean }>(`/api/models/${encodeURIComponent(id)}/unload`),
  },

  downloads: {
    list: () => request<{ downloads: DownloadRecord[] }>('/api/downloads'),
    cancel: (id: string) => post<void>(`/api/downloads/${id}/cancel`),
  },

  conversations: {
    list: (query?: string) =>
      request<{ conversations: Conversation[] }>(
        `/api/conversations${query ? `?q=${encodeURIComponent(query)}` : ''}`,
      ),
    create: (modelId?: string) => post<Conversation>('/api/conversations', { model_id: modelId }),
    get: (id: string) => request<Conversation>(`/api/conversations/${id}`),
    update: (id: string, patch: Partial<Pick<Conversation, 'title' | 'pinned' | 'archived'>> & { model_id?: string }) =>
      request<Conversation>(`/api/conversations/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
    remove: (id: string) => request<void>(`/api/conversations/${id}`, { method: 'DELETE' }),
    messages: (id: string) => request<{ messages: Message[] }>(`/api/conversations/${id}/messages`),
    fork: (id: string, upToMessageId?: string) =>
      post<Conversation>(`/api/conversations/${id}/fork`, { up_to_message_id: upToMessageId }),
  },

  workspaces: {
    list: () =>
      request<{
        workspaces: Workspace[]
        modes: WorkspaceModeInfo[]
        tools: CodingTool[]
      }>('/api/workspaces'),
    create: (name: string, rootPath: string) =>
      post<Workspace>('/api/workspaces', { name, root_path: rootPath }),
    get: (id: string) =>
      request<{ workspace: Workspace; conversations: Conversation[] }>(`/api/workspaces/${id}`),
    update: (id: string, patch: { name?: string; mode?: WorkspaceMode; model_id?: string | null }) =>
      request<Workspace>(`/api/workspaces/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
    remove: (id: string) => request<void>(`/api/workspaces/${id}`, { method: 'DELETE' }),
    tree: (id: string) => request<{ entries: string[] }>(`/api/workspaces/${id}/tree`),
    file: (id: string, path: string) =>
      request<{ path: string; content: string }>(
        `/api/workspaces/${id}/file?path=${encodeURIComponent(path)}`,
      ),
    changes: (id: string) =>
      request<{ changes: WorkspaceChange[] }>(`/api/workspaces/${id}/changes`),
    change: (id: string, changeId: string) =>
      request<{
        id: string
        path: string
        kind: 'edit' | 'write'
        createdAt: string
        undone: boolean
        created: boolean
        before: string | null
        after: string | null
      }>(`/api/workspaces/${id}/changes/${changeId}`),
    undo: (id: string, changeId: string) =>
      post<{ undone: boolean; path: string; removed: boolean }>(
        `/api/workspaces/${id}/changes/${changeId}/undo`,
      ),
    newConversation: (id: string) =>
      post<Conversation>(`/api/workspaces/${id}/conversations`),

    processes: (id: string) =>
      request<{ processes: RunningProcess[] }>(`/api/workspaces/${id}/processes`),
    startProcess: (id: string, command: string) =>
      post<{ process: RunningProcess }>(`/api/workspaces/${id}/processes`, { command }),
    processLog: (id: string, processId: string) =>
      request<{ process: RunningProcess; lines: string[] }>(
        `/api/workspaces/${id}/processes/${processId}/log`,
      ),
    forgetProcess: (id: string, processId: string) =>
      request<{ forgotten: boolean }>(`/api/workspaces/${id}/processes/${processId}`, {
        method: 'DELETE',
      }),
    clearProcesses: (id: string) =>
      request<{ cleared: number }>(`/api/workspaces/${id}/processes`, { method: 'DELETE' }),
    stopProcess: (id: string, processId: string) =>
      post<{ stopped: boolean; command: string }>(
        `/api/workspaces/${id}/processes/${processId}/stop`,
      ),
  },

  fs: {
    browse: (path?: string, showHidden = false) => {
      const params = new URLSearchParams()
      if (path) params.set('path', path)
      if (showHidden) params.set('show_hidden', 'true')
      const suffix = params.toString()
      return request<FolderListing>(`/api/fs/browse${suffix ? `?${suffix}` : ''}`)
    },
    choose: () =>
      post<{
        available: boolean
        cancelled?: boolean
        path: string | null
        reason?: string | null
      }>('/api/fs/choose'),
  },

  free: {
    overview: () => request<FreeOverview>('/api/free'),
    setKey: (slug: string, apiKey: string) =>
      post<{ slug: string; hasKey: boolean }>(`/api/free/${slug}/key`, { apiKey }),
    removeKey: (slug: string) =>
      request<{ slug: string; hasKey: boolean }>(`/api/free/${slug}/key`, { method: 'DELETE' }),
    test: (slug: string) =>
      post<{ ok: boolean; error?: string }>(`/api/free/${slug}/test`),
    usage: () => request<FreeUsage>('/api/free/usage'),
    setKeyless: (enabled: boolean) =>
      post<{ allowKeyless: boolean }>('/api/free/keyless', { enabled }),
    setLimit: (slug: string, limit: FreeLimit) =>
      request<{ slug: string; limit: FreeLimit }>(`/api/free/${slug}/allowance`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(limit),
      }),
    clearLimit: (slug: string) =>
      request<{ slug: string }>(`/api/free/${slug}/allowance`, { method: 'DELETE' }),
  },

  settings: {
    get: () => request<Record<string, unknown>>('/api/settings'),
    patch: (patch: Record<string, unknown>) =>
      request<Record<string, unknown>>('/api/settings', {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
  },

  projects: {
    list: () => request<{ projects: Project[] }>('/api/projects'),
    get: (id: string) =>
      request<{ project: Project; conversations: Conversation[] }>(`/api/projects/${id}`),
    create: (body: {
      name: string
      description?: string
      instructions?: string
      modelId?: string
      toolGroups?: ToolGroup[]
    }) => post<{ project: Project }>('/api/projects', body),
    update: (
      id: string,
      patch: {
        name?: string
        description?: string
        instructions?: string
        modelId?: string | null
        toolGroups?: ToolGroup[] | null
      },
    ) =>
      request<{ project: Project }>(`/api/projects/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
    remove: (id: string) => request<void>(`/api/projects/${id}`, { method: 'DELETE' }),
    moveConversation: (conversationId: string, projectId: string | null) =>
      post<{ projectId: string | null }>(`/api/conversations/${conversationId}/project`, {
        projectId,
      }),
  },

  tools: {
    overview: () => request<ToolsOverview>('/api/tools'),
    setDefaults: (patch: { groups?: ToolGroup[]; memoryPreload?: boolean }) =>
      post<ToolsOverview>('/api/tools/defaults', patch),
    setSkills: (enabled: string[]) => post<ToolsOverview>('/api/tools/skills', { enabled }),
    uploadSkill: (filename: string, content: string) =>
      post<ToolsOverview>('/api/tools/skills/upload', { filename, content }),
    importSkills: (url: string) =>
      post<ToolsOverview & { imported: SkillImport }>('/api/tools/skills/import', { url }),
    deleteSkill: (slug: string) =>
      request<ToolsOverview>(`/api/tools/skills/${encodeURIComponent(slug)}`, {
        method: 'DELETE',
      }),
    configureSearch: (patch: { provider?: string; baseUrl?: string; apiKey?: string }) =>
      post<ToolsOverview>('/api/tools/search', patch),
    testSearch: (query?: string) =>
      post<{ ok: boolean; provider: string; results?: SearchResult[]; error?: string }>(
        '/api/tools/search/test',
        { query },
      ),
  },

  memories: {
    list: (query?: string) =>
      request<{ memories: MemoryRecord[] }>(
        `/api/memories${query ? `?q=${encodeURIComponent(query)}` : ''}`,
      ),
    create: (content: string, tags: string[] = []) =>
      post<{ memory: MemoryRecord }>('/api/memories', { content, tags }),
    remove: (id: string) => request<void>(`/api/memories/${id}`, { method: 'DELETE' }),
  },

  mcp: {
    servers: (connect = false) =>
      request<{ servers: McpServer[] }>(`/api/mcp/servers${connect ? '?connect=true' : ''}`),
    registry: () => request<{ entries: McpRegistryEntry[] }>('/api/mcp/registry'),
    add: (body: {
      slug?: string
      name?: string
      transport?: McpTransport
      url?: string
      command?: string
      args?: string[]
      env?: Record<string, string>
      headers?: Record<string, string>
      authToken?: string
    }) => post<{ server: McpServer; connection: ConnectionResult }>('/api/mcp/servers', body),
    refresh: (id: string) => post<ConnectionResult>(`/api/mcp/servers/${id}/refresh`),
    setEnabled: (id: string, enabled: boolean) =>
      post<{ enabled: boolean }>(`/api/mcp/servers/${id}/enabled`, { enabled }),
    setAuth: (id: string, authToken: string) =>
      post<ConnectionResult>(`/api/mcp/servers/${id}/auth`, { authToken }),
    remove: (id: string) => request<void>(`/api/mcp/servers/${id}`, { method: 'DELETE' }),
  },

  providers: {
    list: () => request<{ providers: Provider[]; presets: ProviderPreset[] }>('/api/providers'),
    add: (body: { provider: string; label?: string; baseUrl?: string; apiKey: string }) =>
      post<{ provider: Provider }>('/api/providers', body),
    test: (id: string) =>
      post<{ ok: boolean; models?: string[]; error?: string }>(`/api/providers/${id}/test`),
    replaceKey: (id: string, apiKey: string) =>
      post<{ status: ProviderStatus; last_error: string | null; models: string[] }>(
        `/api/providers/${id}/key`,
        { apiKey },
      ),
    setEnabled: (id: string, enabled: boolean) =>
      post<{ enabled: boolean }>(`/api/providers/${id}/enabled`, { enabled }),
    remove: (id: string) => request<void>(`/api/providers/${id}`, { method: 'DELETE' }),
  },
}

/* ---------- Streaming ---------- */

export interface WebSource {
  title: string
  url: string
}

export type ChatEvent =
  | { type: 'token'; content: string }
  | { type: 'reasoning'; content: string }
  | { type: 'error'; message: string }
  | { type: 'notice'; message: string }
  | { type: 'tool_call'; name: string; arguments: Record<string, unknown> }
  | { type: 'tool_result'; name: string; ok: boolean; preview: string }
  | {
      type: 'done'
      messageId: string
      modelId: string
      finishReason: string | null
      usage: { promptTokens: number | null; completionTokens: number | null }
      timings: { ttftMs: number | null; totalMs: number | null; tokensPerSecond: number | null }
      sources: WebSource[]
      toolRounds: number
    }

export interface TurnRequest {
  content: string
  model?: string
  effort?: Effort
  tools?: ToolGroup[]
  web_search?: boolean
}

export function streamMessage(
  conversationId: string,
  body: TurnRequest,
  signal?: AbortSignal,
): AsyncGenerator<ChatEvent> {
  return streamTurn(`/api/conversations/${conversationId}/messages`, 'POST', body, signal)
}

export function streamEditMessage(
  conversationId: string,
  messageId: string,
  body: TurnRequest,
  signal?: AbortSignal,
): AsyncGenerator<ChatEvent> {
  return streamTurn(
    `/api/conversations/${conversationId}/messages/${messageId}`,
    'PATCH',
    body,
    signal,
  )
}

async function* streamTurn(
  url: string,
  method: 'POST' | 'PATCH',
  body: TurnRequest,
  signal?: AbortSignal,
): AsyncGenerator<ChatEvent> {
  const response = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  })

  if (!response.ok || !response.body) {
    const text = await response.text()
    let message = `Request failed (${response.status})`
    try {
      const parsed = JSON.parse(text)
      if (typeof parsed?.error?.message === 'string') message = parsed.error.message
    } catch {
      /* keep the generic message */
    }
    throw new ApiError(message, response.status)
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''

  while (true) {
    const { done, value } = await reader.read()
    if (done) break

    buffer += decoder.decode(value, { stream: true })

    let separator = buffer.indexOf('\n\n')
    while (separator !== -1) {
      const block = buffer.slice(0, separator)
      buffer = buffer.slice(separator + 2)

      const event = parseBlock(block)
      if (event) yield event

      separator = buffer.indexOf('\n\n')
    }
  }
}

function parseBlock(block: string): ChatEvent | null {
  let name: string | null = null
  const dataLines: string[] = []

  for (const line of block.split('\n')) {
    if (line.startsWith('event:')) name = line.slice(6).trim()
    else if (line.startsWith('data:')) dataLines.push(line.slice(5).replace(/^ /, ''))
  }

  if (!name || dataLines.length === 0) return null

  try {
    const payload = JSON.parse(dataLines.join('\n'))
    return { type: name, ...payload } as ChatEvent
  } catch {
    return null
  }
}

/* ---------- Formatting ---------- */

export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0) return '—'
  const units = [
    ['TB', 1024 ** 4],
    ['GB', 1024 ** 3],
    ['MB', 1024 ** 2],
    ['KB', 1024],
  ] as const

  for (const [label, size] of units) {
    if (bytes >= size) return `${(bytes / size).toFixed(1)} ${label}`
  }
  return `${bytes} B`
}

const REMOTE_PREFIX = 'cloud:'
const FREE_PREFIX = 'free:'

const FREE_FLAVOURS = ['auto', 'coding', 'reasoning', 'fast']

export function isFreeModel(id: string): boolean {
  const rest = id.startsWith(FREE_PREFIX) ? id.slice(FREE_PREFIX.length) : null
  return rest !== null && FREE_FLAVOURS.includes(rest)
}

export function isRemoteModel(id: string): boolean {
  if (isFreeModel(id)) return true
  return id.startsWith(REMOTE_PREFIX) && id.slice(REMOTE_PREFIX.length).includes('/')
}

export function friendlyModelName(id: string): string {
  if (isFreeModel(id)) {
    const flavour = id.slice(FREE_PREFIX.length)
    return flavour === 'auto' ? 'Kuro Free' : `Kuro Free · ${flavour}`
  }
  if (isRemoteModel(id)) {
    const rest = id.slice(REMOTE_PREFIX.length)
    return rest.slice(rest.indexOf('/') + 1)
  }
  return id.split(':')[0] ?? id
}

export function quantOf(id: string): string | null {
  if (isRemoteModel(id)) return null
  const parts = id.split(':')
  return parts.length > 1 ? (parts[1]?.toUpperCase() ?? null) : null
}

export function publisherOf(id: string): string | null {
  const name = friendlyModelName(id)
  const slash = name.indexOf('/')
  return slash > 0 ? name.slice(0, slash) : null
}

export function formatCount(value: number): string {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1)}M`
  if (value >= 1_000) return `${Math.round(value / 1_000)}k`
  return String(value)
}

export function relativeTime(iso: string): string {
  const then = new Date(iso).getTime()
  if (Number.isNaN(then)) return ''

  const seconds = Math.max(0, Math.floor((Date.now() - then) / 1000))
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days}d ago`
  return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}
