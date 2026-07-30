import { useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Composer } from '../components/Composer'
import { MessageList, type StreamingState } from '../components/MessageList'
import { Logo } from '../components/Logo'
import { api, streamMessage, type Message } from '../lib/api'
import { activeToolGroups, useUi } from '../store/ui'

export function ChatPage() {
  const params = useParams<{ id?: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { selectedModel, effort, webSearch, memory } = useUi()

  const [streaming, setStreaming] = useState<StreamingState | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [notices, setNotices] = useState<string[]>([])
  const abortRef = useRef<AbortController | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)

  const conversationId = params.id ?? null

  const models = useQuery({ queryKey: ['models'], queryFn: api.models.list })

  const messages = useQuery({
    queryKey: ['messages', conversationId],
    queryFn: () => api.conversations.messages(conversationId as string),
    enabled: conversationId !== null,
  })

  const history = messages.data?.messages ?? []
  const isEmpty = conversationId === null || (history.length === 0 && !streaming)

  // Follow the output as it streams.
  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
  }, [history.length, streaming?.content, streaming?.reasoning, streaming?.tools.length])

  // Abandoning a stream by navigating away should stop it, not leave it running.
  useEffect(() => () => abortRef.current?.abort(), [])

  const send = async (content: string) => {
    setError(null)
    setNotices([])

    let targetId = conversationId
    if (targetId === null) {
      const created = await api.conversations.create(selectedModel ?? undefined)
      targetId = created.id
      navigate(`/chat/${created.id}`, { replace: true })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    }

    // Show the user's own message immediately rather than waiting for a refetch.
    queryClient.setQueryData<{ messages: Message[] }>(['messages', targetId], (existing) => ({
      messages: [...(existing?.messages ?? []), optimisticUserMessage(targetId, content)],
    }))

    setStreaming(emptyStream())

    const controller = new AbortController()
    abortRef.current = controller

    try {
      const events = streamMessage(
        targetId,
        {
          content,
          model: selectedModel ?? undefined,
          effort,
          tools: activeToolGroups({ webSearch, memory }),
          web_search: webSearch,
        },
        controller.signal,
      )

      for await (const event of events) {
        if (event.type === 'token') {
          setStreaming((state) => ({
            ...(state ?? emptyStream()),
            content: (state?.content ?? '') + event.content,
          }))
        } else if (event.type === 'reasoning') {
          setStreaming((state) => ({
            ...(state ?? emptyStream()),
            reasoning: (state?.reasoning ?? '') + event.content,
          }))
        } else if (event.type === 'tool_call') {
          // Shown while it runs, so a long search does not look like a hang.
          setStreaming((state) => ({
            ...(state ?? emptyStream()),
            tools: [
              ...(state?.tools ?? []),
              { name: event.name, arguments: event.arguments, state: 'running' },
            ],
          }))
        } else if (event.type === 'tool_result') {
          setStreaming((state) => ({
            ...(state ?? emptyStream()),
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
      // Replace the optimistic view with what was actually stored, which also
      // brings in the usage, timing and tool numbers.
      void queryClient.invalidateQueries({ queryKey: ['messages', targetId] })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    }
  }

  const stop = () => abortRef.current?.abort()

  const composer = (centred: boolean) => (
    <Composer
      models={models.data?.models ?? []}
      remote={models.data?.remote ?? []}
      onSend={(content) => void send(content)}
      onStop={stop}
      isStreaming={streaming !== null}
      centred={centred}
    />
  )

  return (
    <div className={`chat ${isEmpty ? 'is-empty' : ''}`}>
      {isEmpty ? (
        <div className="chat-welcome">
          <Logo size={44} className="welcome-mark" />
          <h1>Kuro</h1>
          <p className="muted">{welcomeLine(models.data)}</p>
          {composer(true)}
        </div>
      ) : (
        <>
          <div className="chat-scroll" ref={scrollRef}>
            <MessageList
              messages={history}
              streaming={streaming}
              error={error}
              notices={notices}
            />
          </div>
          {composer(false)}
        </>
      )}
    </div>
  )
}

function emptyStream(): StreamingState {
  return { content: '', reasoning: '', tools: [] }
}

/**
 * Mark the most recent matching call as finished.
 *
 * Matched from the end because parallel calls to the same tool are legal, and the
 * result that just arrived belongs to the newest one still running.
 */
function resolveLast(
  tools: StreamingState['tools'],
  name: string,
  ok: boolean,
  preview: string,
): StreamingState['tools'] {
  const index = [...tools]
    .reverse()
    .findIndex((tool) => tool.name === name && tool.state === 'running')

  if (index === -1) return tools

  const target = tools.length - 1 - index
  return tools.map((tool, position) =>
    position === target ? { ...tool, state: ok ? 'done' : 'failed', preview } : tool,
  )
}

function welcomeLine(data: { models: unknown[]; remote: unknown[] } | undefined): string {
  if (!data) return 'Loading…'
  if (data.models.length > 0) return 'Ask anything. Everything runs on this machine.'
  if (data.remote.length > 0) return 'No local models yet — a connected provider is standing in.'
  return 'Install a model to get started, or connect a provider.'
}

/**
 * Stand-in for the message the server is about to store.
 *
 * Only the fields the message list reads are filled; the real row replaces this as
 * soon as the turn finishes.
 */
function optimisticUserMessage(conversationId: string, content: string): Message {
  return {
    id: `optimistic-${Date.now()}`,
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
