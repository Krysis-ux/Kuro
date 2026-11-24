import { useEffect, useRef } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Composer, chatToggles } from '../components/Composer'
import { MessageList } from '../components/MessageList'
import { Logo } from '../components/Logo'
import { api } from '../lib/api'
import { forkTurn, runTurn, stopTurn, useTurn } from '../lib/turns'
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

  const scrollRef = useRef<HTMLDivElement>(null)

  const conversationId = params.id ?? null

  const { stream: streaming, error, notices } = useTurn(conversationId)

  const models = useQuery({ queryKey: ['models'], queryFn: api.models.list })

  const messages = useQuery({
    queryKey: ['messages', conversationId],
    queryFn: () => api.conversations.messages(conversationId as string),
    enabled: conversationId !== null,
  })

  const history = messages.data?.messages ?? []
  const isEmpty =
    conversationId === null ||
    (history.length === 0 && !streaming && !error && notices.length === 0)

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
  }, [history.length, streaming?.content, streaming?.reasoning, streaming?.tools.length])

  const send = async (content: string, skills: string[] = [], editing?: string) => {
    let targetId = conversationId
    if (targetId === null) {
      const created = await api.conversations.create(selectedModel ?? undefined)
      targetId = created.id
      navigate(`/chat/${created.id}`, { replace: true })
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    }

    await runTurn(
      targetId,
      content,
      {
        model: selectedModel ?? undefined,
        effort,
        tools: activeToolGroups({ webSearch, memory, projects }),
        web_search: webSearch,
        skills,
      },
      editing,
    )
  }

  const stop = () => stopTurn(conversationId)

  const fork = async (messageId: string) => {
    const branch = await forkTurn(conversationId as string, messageId)
    if (branch) navigate(`/chat/${branch.id}`)
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

function welcomeLine(data: { models: unknown[]; remote: unknown[] } | undefined): string {
  if (!data) return 'Loading…'
  if (data.models.length > 0) return 'Ask anything. Everything runs on this machine.'
  if (data.remote.length > 0) return 'No local models yet — a connected provider is standing in.'
  return 'Install a model to get started, or connect a provider.'
}
