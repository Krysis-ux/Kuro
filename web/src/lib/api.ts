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
}

export interface Message {
  id: string
  conversation_id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  reasoning_content: string | null
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
  hardware: () =>
    request<{ hardware: HardwareInfo; effectiveEngineSettings: Record<string, number> }>(
      '/api/hardware',
    ),

  models: {
    list: () => request<{ models: InstalledModel[] }>('/api/models'),
    recommended: () => request<{ models: RecommendedModel[] }>('/api/models/recommended'),
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
  },

  settings: {
    get: () => request<Record<string, unknown>>('/api/settings'),
    patch: (patch: Record<string, unknown>) =>
      request<Record<string, unknown>>('/api/settings', {
        method: 'PATCH',
        body: JSON.stringify(patch),
      }),
  },
}

/* ---------- Streaming ---------- */

export type ChatEvent =
  | { type: 'token'; content: string }
  | { type: 'reasoning'; content: string }
  | { type: 'error'; message: string }
  | {
      type: 'done'
      messageId: string
      modelId: string
      finishReason: string | null
      usage: { promptTokens: number | null; completionTokens: number | null }
      timings: { ttftMs: number | null; totalMs: number | null; tokensPerSecond: number | null }
    }

/**
 * Send a message and yield events as they arrive.
 *
 * `EventSource` cannot issue a POST, so the stream is read from the response
 * body directly. Bytes are decoded incrementally and only complete events are
 * emitted, which keeps multi-byte characters intact across chunk boundaries.
 */
export async function* streamMessage(
  conversationId: string,
  body: { content: string; model?: string; effort?: Effort },
  signal?: AbortSignal,
): AsyncGenerator<ChatEvent> {
  const response = await fetch(`/api/conversations/${conversationId}/messages`, {
    method: 'POST',
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

/** Strip the quantization suffix so lists stay readable. */
export function friendlyModelName(id: string): string {
  return id.split(':')[0] ?? id
}

export function quantOf(id: string): string | null {
  const parts = id.split(':')
  return parts.length > 1 ? (parts[1]?.toUpperCase() ?? null) : null
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
