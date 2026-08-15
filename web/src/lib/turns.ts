import { useSyncExternalStore } from 'react'
import {
  api,
  streamEditMessage,
  streamMessage,
  OPTIMISTIC_ID_PREFIX,
  type Effort,
  type Message,
  type ToolGroup,
} from './api'
import { queryClient } from './queryClient'

export interface StreamingTool {
  name: string
  arguments: Record<string, unknown>
  state: 'running' | 'done' | 'failed'
  preview?: string
  startedAt: number
}

export interface StreamingState {
  content: string
  reasoning: string
  tools: StreamingTool[]
  touchedAt: number
}

export interface TurnState {
  stream: StreamingState | null
  error: string | null
  notices: string[]
}

const EMPTY: TurnState = { stream: null, error: null, notices: [] }

export interface TurnRequest {
  model?: string
  effort?: Effort
  tools?: ToolGroup[]
  web_search?: boolean
  skills?: string[]
  workspace?: string
}

const turns = new Map<string, TurnState>()

const controllers = new Map<string, AbortController>()

const listeners = new Set<() => void>()

function emit() {
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

function read(key: string | null): TurnState {
  if (key === null) return EMPTY
  return turns.get(key) ?? EMPTY
}

function update(key: string, change: (state: TurnState) => TurnState) {
  turns.set(key, change(turns.get(key) ?? EMPTY))
  emit()
}

function patchStream(key: string, change: (stream: StreamingState) => StreamingState) {
  update(key, (state) => ({
    ...state,
    stream: change(state.stream ?? emptyStream()),
  }))
}

function emptyStream(): StreamingState {
  return { content: '', reasoning: '', tools: [], touchedAt: Date.now() }
}

export function isRunning(key: string | null): boolean {
  return key !== null && controllers.has(key)
}

export function runningCount(): number {
  return controllers.size
}

export function stopTurn(key: string | null) {
  if (key === null) return
  controllers.get(key)?.abort()
}

export function clearTurnError(key: string | null) {
  if (key === null) return
  update(key, (state) => ({ ...state, error: null, notices: [] }))
}

export async function runTurn(
  key: string,
  content: string,
  request: TurnRequest,
  editing?: string,
): Promise<void> {
  if (controllers.has(key)) return

  update(key, () => ({ stream: emptyStream(), error: null, notices: [] }))

  queryClient.setQueryData<{ messages: Message[] }>(['messages', key], (existing) => {
    const held = existing?.messages ?? []
    const cut = editing ? held.findIndex((message) => message.id === editing) : -1
    const kept = cut === -1 ? held : held.slice(0, cut)
    return { messages: [...kept, optimisticUserMessage(key, content)] }
  })

  const controller = new AbortController()
  controllers.set(key, controller)
  emit()

  const { workspace, ...body } = request

  try {
    const events = editing
      ? streamEditMessage(key, editing, { content, ...body }, controller.signal)
      : streamMessage(key, { content, ...body }, controller.signal)

    for await (const event of events) {
      const now = Date.now()

      if (event.type === 'token') {
        patchStream(key, (stream) => ({
          ...stream,
          content: stream.content + event.content,
          touchedAt: now,
        }))
      } else if (event.type === 'reasoning') {
        patchStream(key, (stream) => ({
          ...stream,
          reasoning: stream.reasoning + event.content,
          touchedAt: now,
        }))
      } else if (event.type === 'tool_call') {
        patchStream(key, (stream) => ({
          ...stream,
          tools: [
            ...stream.tools,
            {
              name: event.name,
              arguments: event.arguments,
              state: 'running',
              startedAt: now,
            },
          ],
          touchedAt: now,
        }))
      } else if (event.type === 'tool_result') {
        patchStream(key, (stream) => ({
          ...stream,
          tools: resolveLast(stream.tools, event.name, event.ok, event.preview),
          touchedAt: now,
        }))
      } else if (event.type === 'notice') {
        update(key, (state) => ({ ...state, notices: [...state.notices, event.message] }))
      } else if (event.type === 'error') {
        update(key, (state) => ({ ...state, error: event.message }))
      }
    }
  } catch (caught) {
    if (!controller.signal.aborted) {
      const message = caught instanceof Error ? caught.message : 'Something went wrong.'
      update(key, (state) => ({ ...state, error: message }))
    }
  } finally {
    controllers.delete(key)
    update(key, (state) => ({ ...state, stream: null }))

    void queryClient.invalidateQueries({ queryKey: ['messages', key] })
    void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    void queryClient.invalidateQueries({ queryKey: ['models'] })

    if (workspace) {
      void queryClient.invalidateQueries({ queryKey: ['workspace', workspace] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-tree', workspace] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-changes', workspace] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-processes', workspace] })
    }
  }
}

export async function forkTurn(key: string, messageId: string) {
  try {
    const branch = await api.conversations.fork(key, messageId)
    void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    return branch
  } catch (caught) {
    const message =
      caught instanceof Error ? caught.message : 'Could not fork this conversation.'
    update(key, (state) => ({ ...state, error: message }))
    return null
  }
}

export function useTurn(key: string | null): TurnState {
  return useSyncExternalStore(
    subscribe,
    () => read(key),
    () => EMPTY,
  )
}

export function useAnyTurnRunning(): number {
  return useSyncExternalStore(
    subscribe,
    () => controllers.size,
    () => 0,
  )
}

function resolveLast(
  tools: StreamingTool[],
  name: string,
  ok: boolean,
  preview: string,
): StreamingTool[] {
  const index = [...tools]
    .reverse()
    .findIndex((tool) => tool.name === name && tool.state === 'running')

  if (index === -1) return tools

  const target = tools.length - 1 - index
  return tools.map((tool, position) =>
    position === target ? { ...tool, state: ok ? 'done' : 'failed', preview } : tool,
  )
}

function optimisticUserMessage(conversationId: string, content: string): Message {
  return {
    id: `${OPTIMISTIC_ID_PREFIX}${Date.now()}`,
    conversation_id: conversationId,
    role: 'user',
    content,
    reasoning_content: null,
    tool_calls: null,
    used_web_search: false,
    web_sources: null,
    model_id: null,
    usage_prompt_tokens: null,
    usage_completion_tokens: null,
    timing_ttft_ms: null,
    timing_total_ms: null,
    timing_tokens_per_sec: null,
    finish_reason: null,
    created_at: new Date().toISOString(),
  }
}
