/**
 * Client for the Kuro daemon.
 *
 * Field names mirror the Rust structs exactly rather than being remapped, so a
 * backend change surfaces as a type error here instead of as a silently
 * undefined value at runtime.
 */

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
}

/**
 * A model belonging to a connected provider.
 *
 * `id` carries a `cloud:` prefix and is usable anywhere a local model id is, so
 * the composer and the conversation store need no notion of "remote".
 */
export interface RemoteModel {
  id: string
  name: string
  connector_id: string
  connector_label: string
  provider: string
}

/** A GGUF repository found on Hugging Face. */
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
  /** Published only as multi-part shards, which cannot be loaded yet. */
  split_only: boolean
  fit: FitEstimate | null
  installed: boolean
}

/* ---------- Tools ---------- */

export type ToolGroup = 'web' | 'memory' | 'files'

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

export type SkillCategory = 'language' | 'practice' | 'design' | 'writing'

/** An instruction pack appended to the system prompt when switched on. */
export interface Skill {
  slug: string
  name: string
  blurb: string
  category: SkillCategory
  instructions: string
  approx_tokens: number
}

export interface ToolsOverview {
  builtins: BuiltinTool[]
  defaultGroups: ToolGroup[]
  skills: {
    catalogue: Skill[]
    enabled: string[]
    /** Rough context cost of what is on, so a user can see it adding up. */
    approxTokens: number
  }
  memory: { preload: boolean; count: number }
  files: FileSettings
  search: {
    provider: string
    baseUrl: string | null
    /** Whether a key is stored. The key itself is never sent to the browser. */
    hasApiKey: boolean
    needsApiKey: boolean
    needsBaseUrl: boolean
    providers: SearchProviderOption[]
  }
}

/** How much of the filesystem the model may touch, and where. */
export type FileAccess = 'off' | 'read' | 'write'

export interface FileTier {
  id: FileAccess
  name: string
  /** Plain words for what this tier allows, shown next to the control. */
  description: string
}

export interface FileSettings {
  access: FileAccess
  /** Folders granted, as the user entered them. */
  roots: string[]
  /** Whether the tools would actually be offered: a tier with no folders is not. */
  usable: boolean
  tiers: FileTier[]
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
  /** The name the model is given, after collision handling. */
  name: string
  /** The name the server itself uses. */
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
  /** Whether a bearer token is stored. Never the token. */
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

/** Result of a connection attempt, reported alongside the row it belongs to. */
export interface ConnectionResult {
  ok: boolean
  toolCount?: number
  error?: string
}

/* ---------- Providers ---------- */

export type ProviderStatus = 'untested' | 'ok' | 'error'
export type PresetKind = 'aggregator' | 'first_party' | 'rented_gpu' | 'custom'

/**
 * Which screen an endpoint belongs on.
 *
 * `provider` is someone else's model billed per token; `cloud` is your own model
 * on hardware you rent, billed by the hour. Same mechanism, different decision.
 */
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

/**
 * A project: standing instructions plus a grouping of conversations.
 *
 * The instructions are appended to the model's brief for every chat in the
 * project, which is the substance of the feature.
 */
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
  /** The conversation this one was branched from, when it was. */
  forked_from_id: string | null
}

/**
 * Prefix on the stand-in row shown while a message is still being sent.
 *
 * Such a row has no server-side id yet, so anything that addresses a message by
 * id — forking, editing — has to wait for the real one.
 */
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
  /** The tool trail for this turn, as recorded by the server. */
  tool_calls: { name: string; ok: boolean; preview: string }[] | null
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

export type Effort = 'low' | 'balanced' | 'high' | 'max'

/** Error carrying the server's own message, so the UI never invents wording. */
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
  /** Unload every engine and stop the daemon. */
  shutdown: () => post<{ stopping: boolean; unloadingEngines: number }>('/api/shutdown'),
  /** Relaunch the daemon. The successor starts before this one exits. */
  restart: () => post<{ restarting: boolean; port: number }>('/api/restart'),
  /**
   * Resolve once the server answers again, or reject on timeout.
   *
   * Used after a restart: the browser cannot know when the successor is ready, and
   * a fixed delay would either be wrong or feel slow.
   */
  waitUntilHealthy: async (timeoutMs = 30_000): Promise<void> => {
    const deadline = Date.now() + timeoutMs
    while (Date.now() < deadline) {
      try {
        const response = await fetch('/api/health', { cache: 'no-store' })
        if (response.ok) return
      } catch {
        // Expected while the port is between owners.
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
    recommended: () => request<{ models: RecommendedModel[] }>('/api/models/recommended'),
    /** Search Hugging Face. An empty query returns the most-downloaded. */
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
    /** Branch a chat into a new one, up to and including `upToMessageId`. */
    fork: (id: string, upToMessageId?: string) =>
      post<Conversation>(`/api/conversations/${id}/fork`, { up_to_message_id: upToMessageId }),
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
    /** `null` moves the conversation out of any project. */
    moveConversation: (conversationId: string, projectId: string | null) =>
      post<{ projectId: string | null }>(`/api/conversations/${conversationId}/project`, {
        projectId,
      }),
  },

  tools: {
    overview: () => request<ToolsOverview>('/api/tools'),
    setDefaults: (patch: { groups?: ToolGroup[]; memoryPreload?: boolean }) =>
      post<ToolsOverview>('/api/tools/defaults', patch),
    /** The whole set is sent, not a diff, so concurrent toggles cannot disagree. */
    setSkills: (enabled: string[]) => post<ToolsOverview>('/api/tools/skills', { enabled }),
    configureSearch: (patch: { provider?: string; baseUrl?: string; apiKey?: string }) =>
      post<ToolsOverview>('/api/tools/search', patch),
    /** Run a real search, so a user never has to send a message to find out. */
    testSearch: (query?: string) =>
      post<{ ok: boolean; provider: string; results?: SearchResult[]; error?: string }>(
        '/api/tools/search/test',
        { query },
      ),
    /** Folders are sent whole, not as a diff, and checked to exist server-side. */
    configureFiles: (patch: { access?: FileAccess; roots?: string[] }) =>
      post<ToolsOverview>('/api/tools/files', patch),
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
    /** `connect` dials every enabled server; off by default so loads are instant. */
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
  /** Something went wrong that did not stop the turn — a failed search, say. */
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

/** What a turn is asked to do, whether it is a new message or a rewritten one. */
export interface TurnRequest {
  content: string
  model?: string
  effort?: Effort
  /** Tool groups on for this message. */
  tools?: ToolGroup[]
  /** Search before answering, rather than hoping the model asks. */
  web_search?: boolean
}

/**
 * Send a message and yield events as they arrive.
 *
 * `EventSource` cannot issue a POST, so the stream is read from the response
 * body directly. Bytes are decoded incrementally and only complete events are
 * emitted, which keeps multi-byte characters intact across chunk boundaries.
 */
export function streamMessage(
  conversationId: string,
  body: TurnRequest,
  signal?: AbortSignal,
): AsyncGenerator<ChatEvent> {
  return streamTurn(`/api/conversations/${conversationId}/messages`, 'POST', body, signal)
}

/**
 * Rewrite a message and answer again from that point.
 *
 * The server drops the edited message and everything after it before
 * generating, so the events that arrive here describe a transcript that now
 * ends where the edit was made.
 */
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

    // `stream: true` holds back a partial multi-byte character until the rest
    // of it arrives.
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

/** Prefix marking a model as belonging to a provider rather than this machine. */
const REMOTE_PREFIX = 'cloud:'

export function isRemoteModel(id: string): boolean {
  return id.startsWith(REMOTE_PREFIX) && id.slice(REMOTE_PREFIX.length).includes('/')
}

/** Strip the quantization suffix so lists stay readable. */
export function friendlyModelName(id: string): string {
  if (isRemoteModel(id)) {
    // Everything after the connector id is the provider's own name for it,
    // which may itself contain slashes.
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

/** Owner half of a `publisher/name` model id, when there is one. */
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
