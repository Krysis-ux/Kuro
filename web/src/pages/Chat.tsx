import { useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Composer, chatToggles } from '../components/Composer'
import { MessageList, type StreamingState } from '../components/MessageList'
import { Logo } from '../components/Logo'
import {
  api,
  streamEditMessage,
  streamMessage,
  OPTIMISTIC_ID_PREFIX,
  type Message,
} from '../lib/api'
import { activeToolGroups, useUi } from '../store/ui'

export function ChatPage() {
  const params = useParams<{ id?: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const {
    selectedModel,
    setSelectedModel,
    effort,
    setEffort,
    webSearch,
    setWebSearch,
    memory,
    projects,
    setProjects,
  } = useUi()

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

  /**
   * Run a turn: either a new message, or a rewrite of an existing one.
   *
   * `editing` carries the id of the message being replaced. The server drops
   * that message and everything after it, so the local cache is trimmed to
   * match before the optimistic row goes in — otherwise the old replies stay on
   * screen underneath the new question until the refetch lands.
   */
  const send = async (content: string, skills: string[] = [], editing?: string) => {
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
    queryClient.setQueryData<{ messages: Message[] }>(['messages', targetId], (existing) => {
      const held = existing?.messages ?? []
      const cut = editing ? held.findIndex((message) => message.id === editing) : -1
      // A cut of -1 means the row is not in the cache. Keeping everything is the
      // safe reading: the refetch that follows will correct it either way, and
      // `slice(0, -1)` would quietly drop the wrong message.
      const kept = cut === -1 ? held : held.slice(0, cut)
      return { messages: [...kept, optimisticUserMessage(targetId as string, content)] }
    })

    setStreaming(emptyStream())

    const controller = new AbortController()
    abortRef.current = controller

    try {
      const request = {
        content,
        model: selectedModel ?? undefined,
        effort,
        tools: activeToolGroups({ webSearch, memory, projects }),
        web_search: webSearch,
        skills,
      }
      const events = editing
        ? streamEditMessage(targetId, editing, request, controller.signal)
        : streamMessage(targetId, request, controller.signal)

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

  /**
   * Branch this conversation at a message and open the copy.
   *
   * The original is untouched — that is the point of forking rather than
   * editing: two directions from the same history, both kept.
   */
  const fork = async (messageId: string) => {
    setError(null)
    try {
      const branch = await api.conversations.fork(conversationId as string, messageId)
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      navigate(`/chat/${branch.id}`)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Could not fork this conversation.')
    }
  }

  const composer = (centred: boolean) => (
    <Composer
      models={models.data?.models ?? []}
      remote={models.data?.remote ?? []}
      draftKey={`chat:${conversationId ?? 'new'}`}
      onSend={(content, skills) => void send(content, skills)}
      onStop={stop}
      isStreaming={streaming !== null}
      centred={centred}
      selectedModel={selectedModel}
      onSelectModel={setSelectedModel}
      effort={effort}
      onEffortChange={setEffort}
      effortNote="More effort means longer answers and more room to search before replying."
      toggles={chatToggles({ webSearch, setWebSearch, projects, setProjects })}
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
              onFork={(messageId) => void fork(messageId)}
              onEdit={(messageId, content) => void send(content, [], messageId)}
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
