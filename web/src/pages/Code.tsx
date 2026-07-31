import { useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { MessageList } from '../components/MessageList'
import { ModelPicker } from '../components/ModelPicker'
import { api, type WorkspaceChange, type WorkspaceMode } from '../lib/api'
import { useTurn } from '../lib/useTurn'
import { useUi } from '../store/ui'
import {
  FileIcon,
  FolderIcon,
  PlusIcon,
  RefreshIcon,
  SendIcon,
  StopIcon,
  TrashIcon,
} from '../components/icons'

/**
 * The Code page.
 *
 * The only surface in Kuro that can read or change a file. A workspace is a
 * folder the user picked plus a mode saying what may happen inside it, and the
 * mode is the permission: it is chosen before the turn, visible while it runs,
 * and decides which tools the model is even shown.
 *
 * Kept deliberately separable. Everything here talks to `/api/workspaces` and
 * the ordinary chat endpoint, so lifting it into its own application later
 * means moving these files and pointing them at the same daemon.
 */
export function CodePage() {
  const params = useParams<{ id?: string }>()
  return params.id ? <WorkspaceView id={params.id} /> : <WorkspaceList />
}

/* ---------- Choosing a workspace ---------- */

function WorkspaceList() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [name, setName] = useState('')
  const [root, setRoot] = useState('')
  const [error, setError] = useState<string | null>(null)

  const workspaces = useQuery({ queryKey: ['workspaces'], queryFn: api.workspaces.list })

  const create = useMutation({
    mutationFn: () => api.workspaces.create(name.trim(), root.trim()),
    onSuccess: (workspace) => {
      void queryClient.invalidateQueries({ queryKey: ['workspaces'] })
      navigate(`/code/${workspace.id}`)
    },
    onError: (caught) =>
      setError(caught instanceof Error ? caught.message : 'That folder could not be opened.'),
  })

  const submit = () => {
    setError(null)
    if (!name.trim() || !root.trim()) return
    create.mutate()
  }

  const held = workspaces.data?.workspaces ?? []

  return (
    <div className="page">
      <header className="page-head">
        <h1>Code</h1>
        <p className="muted">
          A workspace is a folder on this computer that a model can work in. Chat cannot reach your
          files at all — this is the only place that can, and only in the folder you choose.
        </p>
      </header>

      <section className="panel">
        <h2 className="panel-title">New workspace</h2>
        <div className="workspace-new">
          <input
            className="input"
            placeholder="Name"
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <input
            className="input mono"
            placeholder="~/Projects/my-app"
            value={root}
            onChange={(event) => setRoot(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && submit()}
          />
          <button
            className="btn btn-solid"
            onClick={submit}
            disabled={!name.trim() || !root.trim() || create.isPending}
          >
            <PlusIcon size={14} />
            {create.isPending ? 'Opening…' : 'Open'}
          </button>
        </div>
        {error && <p className="form-error">{error}</p>}
        <p className="faint hint">
          The folder must already exist. Everything outside it is refused, and credentials inside it
          are refused too.
        </p>
      </section>

      {held.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">Workspaces</h2>
          <ul className="workspace-list">
            {held.map((workspace) => (
              <li key={workspace.id}>
                <button className="workspace-row" onClick={() => navigate(`/code/${workspace.id}`)}>
                  <FolderIcon size={15} />
                  <span className="workspace-row-main">
                    <span className="workspace-row-name">{workspace.name}</span>
                    <span className="faint mono workspace-row-path">{workspace.root_path}</span>
                  </span>
                  <span className={`mode-chip is-${workspace.mode}`}>{workspace.mode}</span>
                  {!workspace.root_exists && <span className="tag is-warning">folder missing</span>}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  )
}

/* ---------- Working in one ---------- */

function WorkspaceView({ id }: { id: string }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { selectedModel, setSelectedModel } = useUi()
  const [conversationId, setConversationId] = useState<string | null>(null)
  const [panel, setPanel] = useState<'files' | 'changes'>('files')
  const [text, setText] = useState('')
  const scrollRef = useRef<HTMLDivElement>(null)

  const detail = useQuery({
    queryKey: ['workspace', id],
    queryFn: () => api.workspaces.get(id),
  })
  const models = useQuery({ queryKey: ['models'], queryFn: api.models.list })

  const turn = useTurn(() => {
    // A coding turn may have touched the project, so both panels are stale.
    void queryClient.invalidateQueries({ queryKey: ['workspace-tree', id] })
    void queryClient.invalidateQueries({ queryKey: ['workspace-changes', id] })
  })

  const messages = useQuery({
    queryKey: ['messages', conversationId],
    queryFn: () => api.conversations.messages(conversationId as string),
    enabled: conversationId !== null,
  })

  const workspace = detail.data?.workspace
  const history = messages.data?.messages ?? []

  // Resume the most recent chat in this workspace rather than starting an empty
  // one, so reopening a workspace picks up where it was left.
  useEffect(() => {
    if (conversationId !== null || !detail.data) return
    const existing = detail.data.conversations[0]
    if (existing) setConversationId(existing.id)
  }, [detail.data, conversationId])

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
  }, [history.length, turn.streaming?.content, turn.streaming?.tools.length])

  const setMode = useMutation({
    mutationFn: (mode: WorkspaceMode) => api.workspaces.update(id, { mode }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['workspace', id] })
      void queryClient.invalidateQueries({ queryKey: ['workspaces'] })
    },
  })

  const send = async () => {
    const content = text.trim()
    if (!content || turn.streaming) return

    let target = conversationId
    if (target === null) {
      const created = await api.workspaces.newConversation(id)
      target = created.id
      setConversationId(created.id)
      void queryClient.invalidateQueries({ queryKey: ['workspace', id] })
    }

    setText('')
    // Web and memory are off here by default: a coding turn's tools are the
    // workspace's, and the model already has more than enough to choose between.
    await turn.send(target, content, { model: selectedModel ?? undefined, effort: 'balanced' })
  }

  if (detail.isLoading) return <div className="page muted">Loading…</div>
  if (!workspace) {
    return (
      <div className="page">
        <p className="muted">That workspace is gone.</p>
        <button className="btn btn-ghost" onClick={() => navigate('/code')}>
          Back to workspaces
        </button>
      </div>
    )
  }

  return (
    <div className="code">
      <aside className="code-side">
        <button className="code-back" onClick={() => navigate('/code')}>
          ← Workspaces
        </button>
        <div className="code-workspace">
          <strong>{workspace.name}</strong>
          <span className="faint mono code-root">{workspace.root_path}</span>
        </div>

        {!workspace.root_exists && (
          <p className="form-error">
            This folder is no longer on disk, so nothing here can run.
          </p>
        )}

        <div className="code-tabs">
          <button
            className={`code-tab ${panel === 'files' ? 'is-on' : ''}`}
            onClick={() => setPanel('files')}
          >
            Files
          </button>
          <button
            className={`code-tab ${panel === 'changes' ? 'is-on' : ''}`}
            onClick={() => setPanel('changes')}
          >
            Changes
          </button>
        </div>

        {panel === 'files' ? <FileTree id={id} /> : <ChangeList id={id} />}
      </aside>

      <main className="code-main">
        <header className="code-head">
          <ModeSwitch
            current={workspace.mode}
            onChange={(mode) => setMode.mutate(mode)}
            busy={setMode.isPending}
          />
          <ModelPicker
            installed={models.data?.models ?? []}
            remote={models.data?.remote ?? []}
            selected={selectedModel}
            onSelect={setSelectedModel}
          />
        </header>

        <div className="chat-scroll" ref={scrollRef}>
          {history.length === 0 && !turn.streaming ? (
            <div className="code-empty">
              <FolderIcon size={26} />
              <p className="muted">{emptyLine(workspace.mode)}</p>
            </div>
          ) : (
            <MessageList
              messages={history}
              streaming={turn.streaming}
              error={turn.error}
              notices={turn.notices}
              onFork={(messageId) => {
                void turn.fork(conversationId as string, messageId).then((branch) => {
                  if (branch) setConversationId(branch.id)
                })
              }}
              onEdit={(messageId, content) => {
                void turn.send(
                  conversationId as string,
                  content,
                  { model: selectedModel ?? undefined, effort: 'balanced' },
                  messageId,
                )
              }}
            />
          )}
        </div>

        <div className="composer-shell">
          <div className="composer">
            <textarea
              className="composer-input"
              rows={1}
              placeholder={placeholderFor(workspace.mode)}
              value={text}
              onChange={(event) => setText(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && !event.shiftKey) {
                  event.preventDefault()
                  void send()
                }
              }}
            />
            <div className="composer-actions">
              <span className="faint code-mode-note">{modeNote(workspace.mode)}</span>
              {turn.streaming ? (
                <button className="btn btn-solid btn-icon" onClick={turn.stop} aria-label="Stop">
                  <StopIcon />
                </button>
              ) : (
                <button
                  className="btn btn-solid btn-icon"
                  onClick={() => void send()}
                  disabled={!text.trim() || !workspace.root_exists}
                  aria-label="Send"
                >
                  <SendIcon />
                </button>
              )}
            </div>
          </div>
        </div>
      </main>
    </div>
  )
}

/* ---------- Mode ---------- */

/**
 * The three modes, as one control.
 *
 * Shown as a row rather than a dropdown because which one is active is the most
 * important fact on this page — it is the difference between a model that can
 * read the project and one that can rewrite it.
 */
function ModeSwitch({
  current,
  onChange,
  busy,
}: {
  current: WorkspaceMode
  onChange: (mode: WorkspaceMode) => void
  busy: boolean
}) {
  const modes = useQuery({ queryKey: ['workspaces'], queryFn: api.workspaces.list })
  const options = modes.data?.modes ?? []

  return (
    <div className="mode-switch" role="group" aria-label="Mode">
      {options.map((mode) => (
        <button
          key={mode.id}
          className={`mode-step is-${mode.id} ${mode.id === current ? 'is-on' : ''}`}
          onClick={() => onChange(mode.id)}
          disabled={busy}
          title={mode.blurb}
          aria-pressed={mode.id === current}
        >
          {mode.label}
        </button>
      ))}
    </div>
  )
}

/* ---------- Panels ---------- */

function FileTree({ id }: { id: string }) {
  const tree = useQuery({
    queryKey: ['workspace-tree', id],
    queryFn: () => api.workspaces.tree(id),
  })

  if (tree.isLoading) return <p className="faint code-panel-note">Reading the folder…</p>
  if (tree.isError) return <p className="form-error">That folder could not be read.</p>

  const entries = tree.data?.entries ?? []
  if (entries.length === 0) return <p className="faint code-panel-note">This folder is empty.</p>

  return (
    <ul className="code-tree">
      {entries.map((entry) => (
        <li key={entry} className={entry.endsWith('/') ? 'is-dir' : ''}>
          {entry.endsWith('/') ? <FolderIcon size={12} /> : <FileIcon size={12} />}
          <span className="mono">{entry}</span>
        </li>
      ))}
    </ul>
  )
}

/**
 * Every file the model changed, newest first, each with a way to put it back.
 *
 * This is what makes Agent mode reasonable to offer without a prompt before
 * every write: the change already happened, and undoing it is one click.
 */
function ChangeList({ id }: { id: string }) {
  const queryClient = useQueryClient()
  const [failed, setFailed] = useState<string | null>(null)

  const changes = useQuery({
    queryKey: ['workspace-changes', id],
    queryFn: () => api.workspaces.changes(id),
  })

  const undo = useMutation({
    mutationFn: (changeId: string) => api.workspaces.undo(id, changeId),
    onSuccess: () => {
      setFailed(null)
      void queryClient.invalidateQueries({ queryKey: ['workspace-changes', id] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-tree', id] })
    },
    onError: (caught) =>
      setFailed(caught instanceof Error ? caught.message : 'That change could not be undone.'),
  })

  const held = changes.data?.changes ?? []
  if (held.length === 0) {
    return <p className="faint code-panel-note">Nothing has been changed yet.</p>
  }

  return (
    <>
      {failed && <p className="form-error">{failed}</p>}
      <ul className="code-changes">
        {held.map((change) => (
          <li key={change.id} className={change.undone ? 'is-undone' : ''}>
            <div className="code-change-head">
              <span className="mono code-change-path">{change.path}</span>
              {change.undone ? (
                <span className="faint">undone</span>
              ) : (
                change.undoable && (
                  <button
                    className="btn btn-ghost code-undo"
                    onClick={() => undo.mutate(change.id)}
                    disabled={undo.isPending}
                    title={
                      change.created
                        ? 'Remove this file, which the model created'
                        : 'Put the previous contents back'
                    }
                  >
                    {change.created ? <TrashIcon size={12} /> : <RefreshIcon size={12} />}
                    Undo
                  </button>
                )
              )}
            </div>
            <span className="faint code-change-note">{describeChange(change)}</span>
          </li>
        ))}
      </ul>
    </>
  )
}

function describeChange(change: WorkspaceChange): string {
  if (change.created) return `created · ${change.afterLines ?? 0} lines`
  const before = change.beforeLines ?? 0
  const after = change.afterLines ?? 0
  const delta = after - before
  const sign = delta > 0 ? '+' : ''
  return `${change.kind === 'edit' ? 'edited' : 'replaced'} · ${after} lines (${sign}${delta})`
}

/* ---------- Words ---------- */

function emptyLine(mode: WorkspaceMode): string {
  switch (mode) {
    case 'ask':
      return 'Ask mode. The model cannot see this project — paste in what you want to discuss.'
    case 'plan':
      return 'Plan mode. Ask about this project and the model will read it before answering.'
    case 'agent':
      return 'Agent mode. Describe a change and the model will make it. Everything it does can be undone.'
  }
}

function placeholderFor(mode: WorkspaceMode): string {
  switch (mode) {
    case 'ask':
      return 'Ask about code…'
    case 'plan':
      return 'Ask about this project…'
    case 'agent':
      return 'Describe a change…'
  }
}

function modeNote(mode: WorkspaceMode): string {
  switch (mode) {
    case 'ask':
      return 'No file access'
    case 'plan':
      return 'Can read this folder'
    case 'agent':
      return 'Can change this folder'
  }
}
