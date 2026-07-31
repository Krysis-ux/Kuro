import { useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import {
  api,
  streamEditMessage,
  streamMessage,
  OPTIMISTIC_ID_PREFIX,
  type Effort,
  type Message,
  type ToolGroup,
} from './api'

/** One tool call, as the transcript watches it happen. */
export interface StreamingTool {
  name: string
  arguments: Record<string, unknown>
  state: 'running' | 'done' | 'failed'
  preview?: string
}

export interface StreamingState {
  content: string
  reasoning: string
  tools: StreamingTool[]
}

export interface TurnRequest {
  model?: string
  effort?: Effort
  tools?: ToolGroup[]
  web_search?: boolean
}

/**
 * Running one turn of a conversation, wherever it is being shown.
 *
 * Chat and the Code page are two surfaces over the same endpoint, and the
 * streaming loop is the part that is easy to get subtly different between them —
 * a missed event type, a cache that is invalidated in one and not the other. It
 * lives here once so both behave the same way.
 */
export function useTurn(onChanged?: () => void) {
  const queryClient = useQueryClient()
  const [streaming, setStreaming] = useState<StreamingState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notices, setNotices] = useState<string[]>([])
  const abortRef = useRef<AbortController | null>(null)

  // Navigating away should stop the stream, not leave it running.
  useEffect(() => () => abortRef.current?.abort(), [])

  const stop = () => abortRef.current?.abort()

  /**
   * Send a message, or rewrite one.
   *
   * `editing` carries the id of the message being replaced. The server drops it
   * and everything after it, so the cache is trimmed to match before the
   * optimistic row goes in — otherwise the old replies stay on screen under the
   * new question until the refetch lands.
   */
  const send = async (
    conversationId: string,
    content: string,
    request: TurnRequest,
    editing?: string,
  ) => {
    setError(null)
    setNotices([])

    queryClient.setQueryData<{ messages: Message[] }>(['messages', conversationId], (existing) => {
      const held = existing?.messages ?? []
      const cut = editing ? held.findIndex((message) => message.id === editing) : -1
      // A cut of -1 means the row is not in the cache. Keeping everything is the
      // safe reading: the refetch that follows corrects it either way, and
      // `slice(0, -1)` would quietly drop the wrong message.
      const kept = cut === -1 ? held : held.slice(0, cut)
      return { messages: [...kept, optimisticUserMessage(conversationId, content)] }
    })

    setStreaming({ content: '', reasoning: '', tools: [] })

    const controller = new AbortController()
    abortRef.current = controller

    try {
      const body = { content, ...request }
      const events = editing
        ? streamEditMessage(conversationId, editing, body, controller.signal)
        : streamMessage(conversationId, body, controller.signal)

      for await (const event of events) {
        if (event.type === 'token') {
          setStreaming((state) => ({
            ...(state ?? empty()),
            content: (state?.content ?? '') + event.content,
          }))
        } else if (event.type === 'reasoning') {
          setStreaming((state) => ({
            ...(state ?? empty()),
            reasoning: (state?.reasoning ?? '') + event.content,
          }))
        } else if (event.type === 'tool_call') {
          // Shown while it runs, so a long search or a large file does not look
          // like a hang.
          setStreaming((state) => ({
            ...(state ?? empty()),
            tools: [
              ...(state?.tools ?? []),
              { name: event.name, arguments: event.arguments, state: 'running' },
            ],
          }))
        } else if (event.type === 'tool_result') {
          setStreaming((state) => ({
            ...(state ?? empty()),
            tools: resolveLast(state?.tools ?? [], event.name, event.ok, event.preview),
          }))
        } else if (event.type === 'notice') {
          setNotices((held) => [...held, event.message])
        } else if (event.type === 'error') {
          setError(event.message)
        }
      }
    } catch (caught) {
      // An abort is the user pressing stop, not a failure worth reporting.
      if (!controller.signal.aborted) {
        setError(caught instanceof Error ? caught.message : 'Something went wrong.')
      }
    } finally {
      abortRef.current = null
      setStreaming(null)
      // Replace the optimistic view with what was stored, which also brings in
      // the usage, timing and tool numbers.
      void queryClient.invalidateQueries({ queryKey: ['messages', conversationId] })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      // A coding turn may have changed files, so the panels that show them
      // refetch too. The caller decides what that means.
      onChanged?.()
    }
  }

  /** Branch a conversation at a message. The original is left untouched. */
  const fork = async (conversationId: string, messageId: string) => {
    setError(null)
    try {
      const branch = await api.conversations.fork(conversationId, messageId)
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      return branch
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Could not fork this conversation.')
      return null
    }
  }

  return { streaming, error, notices, send, stop, fork, setError }
}

function empty(): StreamingState {
  return { content: '', reasoning: '', tools: [] }
}

/**
 * Mark the most recent matching call as finished.
 *
 * Matched from the end because parallel calls to the same tool are legal, and
 * the result that just arrived belongs to the newest one still running.
 */
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

/**
 * Stand-in for the message the server is about to store.
 *
 * Only the fields the transcript reads are filled; the real row replaces this
 * as soon as the turn finishes.
 */
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
