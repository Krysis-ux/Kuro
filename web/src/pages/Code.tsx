import { useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { MessageList } from '../components/MessageList'
import { CodeBlock } from '../components/CodeBlock'
import { Composer } from '../components/Composer'
import { FolderField } from '../components/FolderPicker'
import { PanelDivider } from '../components/PanelDivider'
import { PreviewPanel } from '../components/PreviewPanel'
import { DiffView } from '../components/DiffView'
import { api, type WorkspaceChange, type WorkspaceMode } from '../lib/api'
import { forkTurn, runTurn, stopTurn, useTurn } from '../lib/turns'
import { useUi } from '../store/ui'
import {
  BoltIcon,
  ChatIcon,
  ChevronIcon,
  CloseIcon,
  EyeIcon,
  GlobeIcon,
  ListIcon,
  ShieldOffIcon,
  FileIcon,
  FolderIcon,
  PanelIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  TerminalIcon,
  TrashIcon,
} from '../components/icons'

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
    if (!root.trim()) return
    create.mutate()
  }

  const held = workspaces.data?.workspaces ?? []

  const chooseRoot = (path: string) => {
    setRoot(path)
    if (!name.trim()) {
      const leaf = path.split(/[\\/]/).filter(Boolean).pop()
      if (leaf) setName(leaf)
    }
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>Code</h1>
        <p className="muted">
          A workspace is a folder on this computer that a model can work in — reading it,
          changing it, and running its build and tests. Chat can read these folders; only
          here can anything be changed.
        </p>
      </header>

      <section className="panel">
        <h2 className="panel-title">New workspace</h2>

        <div className="field field-stacked">
          <div className="field-label">
            <span>Folder</span>
            <span className="faint field-hint">It must already exist.</span>
          </div>
          <FolderField
            value={root}
            onChange={chooseRoot}
            title="Choose a project folder"
            placeholder="No folder chosen"
          />
        </div>

        <div className="field field-stacked">
          <div className="field-label">
            <span>Name</span>
            <span className="faint field-hint">What you will call it here.</span>
          </div>
          <div className="inline-form">
            <input
              className="input"
              placeholder="Named after the folder"
              value={name}
              onChange={(event) => setName(event.target.value)}
              onKeyDown={(event) => event.key === 'Enter' && submit()}
            />
            <button
              className="btn btn-solid"
              onClick={submit}
              disabled={!root.trim() || create.isPending}
            >
              <PlusIcon size={14} />
              {create.isPending ? 'Opening…' : 'Open'}
            </button>
          </div>
        </div>

        {error && <p className="form-error">{error}</p>}
        <p className="faint hint">
          Everything outside the folder is refused, and credentials inside it are refused too.
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
  const {
    codeModel,
    setCodeModel,
    codeEffort,
    setCodeEffort,
    codePanel,
    setCodePanel,
    filesOpen,
    setFilesOpen,
    filesWidth,
    setFilesWidth,
    runningOpen,
    setRunningOpen,
    runningView,
    setRunningView,
    runningWidth,
    setRunningWidth,
  } = useUi()
  const [conversationId, setConversationId] = useState<string | null>(null)
  const [openFile, setOpenFile] = useState<string | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)

  const detail = useQuery({
    queryKey: ['workspace', id],
    queryFn: () => api.workspaces.get(id),
  })
  const models = useQuery({ queryKey: ['models'], queryFn: api.models.list })

  const running = useRunningProcesses(id, runningOpen, setRunningOpen)

  const turn = useTurn(conversationId)

  const messages = useQuery({
    queryKey: ['messages', conversationId],
    queryFn: () => api.conversations.messages(conversationId as string),
    enabled: conversationId !== null,
  })

  const workspace = detail.data?.workspace
  const history = messages.data?.messages ?? []

  useEffect(() => {
    if (conversationId !== null || !detail.data) return
    const existing = detail.data.conversations[0]
    if (existing) setConversationId(existing.id)
  }, [detail.data, conversationId])

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight })
  }, [history.length, turn.stream?.content, turn.stream?.tools.length])

  const setMode = useMutation({
    mutationFn: (mode: WorkspaceMode) => api.workspaces.update(id, { mode }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['workspace', id] })
      void queryClient.invalidateQueries({ queryKey: ['workspaces'] })
    },
  })

  const send = async (content: string, skills: string[] = []) => {
    if (turn.stream) return

    let target = conversationId
    if (target === null) {
      const created = await api.workspaces.newConversation(id)
      target = created.id
      setConversationId(created.id)
      void queryClient.invalidateQueries({ queryKey: ['workspace', id] })
    }

    await runTurn(target, content, {
      model: codeModel ?? undefined,
      effort: codeEffort,
      tools: [],
      skills,
      workspace: id,
    })
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

  const blocked = !workspace.root_exists
    ? 'This folder is no longer on disk, so nothing here can run.'
    : null

  return (
    <div className="code">
      {filesOpen && (
        <>
          <aside className="code-side" style={{ width: filesWidth }}>
            <button className="code-back" onClick={() => navigate('/code')}>
              ← Workspaces
            </button>
            <div className="code-workspace">
              <strong>{workspace.name}</strong>
              <span className="faint mono code-root">{workspace.root_path}</span>
            </div>

            {blocked && <p className="form-error">{blocked}</p>}

            <div className="code-tabs">
              <button
                className={`code-tab ${codePanel === 'files' ? 'is-on' : ''}`}
                onClick={() => setCodePanel('files')}
              >
                Files
              </button>
              <button
                className={`code-tab ${codePanel === 'changes' ? 'is-on' : ''}`}
                onClick={() => setCodePanel('changes')}
              >
                Changes
              </button>
            </div>

            {codePanel === 'files' && <FileTree id={id} onOpen={setOpenFile} />}
            {codePanel === 'changes' && <ChangeList id={id} />}
          </aside>

          <PanelDivider
            width={filesWidth}
            onResize={setFilesWidth}
            side="left"
            label="Resize the file panel"
          />
        </>
      )}

      {openFile && (
        <FileViewer id={id} path={openFile} onClose={() => setOpenFile(null)} />
      )}

      <main className="code-main">
        <div className="code-bar">
          <button
            className={`code-bar-btn ${filesOpen ? 'is-on' : ''}`}
            onClick={() => setFilesOpen(!filesOpen)}
            aria-pressed={filesOpen}
            title={filesOpen ? 'Hide the file panel' : 'Show the file panel'}
          >
            <PanelIcon size={13} />
            <span className="code-bar-label">Files</span>
          </button>

          <span className="code-bar-spacer" />

          {/*
            Small, and on the right, because it is a viewfinder rather than a
            destination — and it counts, because "is anything running" is the
            question and a badge answers it without the panel being open.
          */}
          <button
            className={`code-bar-btn ${runningOpen ? 'is-on' : ''} ${
              running.live > 0 ? 'is-live' : ''
            }`}
            onClick={() => setRunningOpen(!runningOpen)}
            aria-pressed={runningOpen}
            title={
              running.live > 0
                ? `${running.live} running — click to ${runningOpen ? 'hide' : 'show'}`
                : 'Nothing is running'
            }
          >
            <EyeIcon size={13} />
            <span className="code-bar-label">Running</span>
            {running.live > 0 && <span className="code-bar-count">{running.live}</span>}
          </button>

          {/*
            The terminal and the browser, each one click away.

            Both were already in the running panel and neither was reachable
            without first finding the panel, opening it, and scrolling to the
            half you wanted. A button that opens the panel *at* the thing you
            asked for is the same feature with the hunting removed.
          */}
          <button
            className={`code-bar-btn ${
              runningOpen && runningView === 'terminal' ? 'is-on' : ''
            }`}
            onClick={() => {
              setRunningView('terminal')
              setRunningOpen(true)
            }}
            aria-pressed={runningOpen && runningView === 'terminal'}
            title="Run a command and read its output"
          >
            <TerminalIcon size={13} />
            <span className="code-bar-label">Terminal</span>
          </button>

          <button
            className={`code-bar-btn ${
              runningOpen && runningView === 'browser' ? 'is-on' : ''
            }`}
            onClick={() => {
              setRunningView('browser')
              setRunningOpen(true)
            }}
            aria-pressed={runningOpen && runningView === 'browser'}
            title="Look at whatever this workspace is serving"
          >
            <GlobeIcon size={13} />
            <span className="code-bar-label">Browser</span>
          </button>
        </div>

        <div className="chat-scroll" ref={scrollRef}>
          {history.length === 0 && !turn.stream && !turn.error && turn.notices.length === 0 ? (
            <div className="code-empty">
              <FolderIcon size={26} />
              <p className="muted">{emptyLine(workspace.mode)}</p>
            </div>
          ) : (
            <MessageList
              messages={history}
              streaming={turn.stream}
              error={turn.error}
              notices={turn.notices}
              onFork={(messageId) => {
                void forkTurn(conversationId as string, messageId).then((branch) => {
                  if (branch) setConversationId(branch.id)
                })
              }}
              onOpenPath={(path) => setOpenFile(path)}
              onEdit={(messageId, content) => {
                void runTurn(
                  conversationId as string,
                  content,
                  {
                    model: codeModel ?? undefined,
                    effort: codeEffort,
                    tools: [],
                    workspace: id,
                  },
                  messageId,
                )
              }}
            />
          )}
        </div>

        <Composer
          models={models.data?.models ?? []}
          remote={models.data?.remote ?? []}
          onSend={(content, skills) => void send(content, skills)}
          onStop={() => stopTurn(conversationId)}
          isStreaming={Boolean(turn.stream)}
          selectedModel={codeModel}
          onSelectModel={setCodeModel}
          effort={codeEffort}
          onEffortChange={setCodeEffort}
          coding
          effortNote="More effort means more rounds of reading, editing and running, and pulls in the skills that match this project. Ultracode brings all of them and lets a turn run as long as it needs."
          placeholder={placeholderFor(workspace.mode)}
          leading={
            <ModeSwitch
              current={workspace.mode}
              onChange={(mode) => setMode.mutate(mode)}
              busy={setMode.isPending}
            />
          }
          draftKey={`code:${id}`}
          disabledReason={blocked}
          hint={modeNote(workspace.mode)}
        />
      </main>

      {runningOpen && (
        <>
          <PanelDivider
            width={runningWidth}
            onResize={setRunningWidth}
            side="right"
            label="Resize the running panel"
          />

          <aside className="code-running" style={{ width: runningWidth }}>
            <div className="code-running-head">
              {runningView === 'browser' ? <GlobeIcon size={13} /> : <TerminalIcon size={13} />}
              <strong>{runningView === 'browser' ? 'Browser' : 'Terminal'}</strong>
              {running.live > 0 && (
                <span className="tag tag-live">{running.live} running</span>
              )}
              <button
                className="btn btn-ghost btn-icon"
                onClick={() => setRunningOpen(false)}
                aria-label="Hide the running panel"
                title="Hide"
              >
                <CloseIcon size={14} />
              </button>
            </div>
            <PreviewPanel workspaceId={id} mode={workspace.mode} view={runningView} />
          </aside>
        </>
      )}
    </div>
  )
}

function useRunningProcesses(
  id: string,
  isOpen: boolean,
  open: (value: boolean) => void,
): { live: number } {
  const processes = useQuery({
    queryKey: ['workspace-processes', id],
    queryFn: () => api.workspaces.processes(id),
    refetchInterval: 2000,
    refetchIntervalInBackground: true,
  })

  const live = processes.data?.processes.filter((held) => held.running).length ?? 0
  const previous = useRef(live)
  const openRef = useRef(open)
  openRef.current = open
  const isOpenRef = useRef(isOpen)
  isOpenRef.current = isOpen

  useEffect(() => {
    if (live > previous.current && !isOpenRef.current) openRef.current(true)
    previous.current = live
  }, [live])

  return { live }
}

/* ---------- Mode ---------- */

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
          title={`${mode.label} — ${mode.blurb}`}
          aria-pressed={mode.id === current}
          aria-label={mode.label}
        >
          {MODE_ICON[mode.id]}
          <span className="mode-step-label">{mode.label}</span>
        </button>
      ))}
    </div>
  )
}

const MODE_ICON: Record<WorkspaceMode, React.ReactNode> = {
  ask: <ChatIcon size={13} />,
  plan: <ListIcon size={13} />,
  agent: <BoltIcon size={13} />,
  bypass: <ShieldOffIcon size={13} />,
}

/* ---------- Panels ---------- */

function FileTree({ id, onOpen }: { id: string; onOpen: (path: string) => void }) {
  const [filter, setFilter] = useState('')

  const tree = useQuery({
    queryKey: ['workspace-tree', id],
    queryFn: () => api.workspaces.tree(id),
  })

  if (tree.isLoading) return <p className="faint code-panel-note">Reading the folder…</p>
  if (tree.isError) return <p className="form-error">That folder could not be read.</p>

  const entries = tree.data?.entries ?? []
  if (entries.length === 0) return <p className="faint code-panel-note">This folder is empty.</p>

  const needle = filter.trim().toLowerCase()
  const shown = needle
    ? entries.filter((entry) => entry.toLowerCase().includes(needle))
    : entries

  return (
    <>
      <div className="code-tree-filter">
        <SearchIcon size={12} className="search-icon" />
        <input
          className="input"
          placeholder="Filter files…"
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
        />
      </div>

      {shown.length === 0 && (
        <p className="faint code-panel-note">Nothing matched “{filter.trim()}”.</p>
      )}

      <ul className="code-tree">
        <TreeLevel
          nodes={buildTree(shown)}
          depth={0}
          forceOpen={needle.length > 0}
          onOpen={onOpen}
        />
      </ul>
    </>
  )
}

interface TreeNode {
  name: string
  path: string
  dir: boolean
  children: TreeNode[]
}

function buildTree(paths: string[]): TreeNode[] {
  const roots: TreeNode[] = []

  for (const path of paths) {
    const dir = path.endsWith('/')
    const parts = path.replace(/\/$/, '').split('/').filter(Boolean)
    let level = roots

    parts.forEach((name, index) => {
      const last = index === parts.length - 1
      const here = parts.slice(0, index + 1).join('/')

      let node = level.find((candidate) => candidate.name === name)
      if (!node) {
        node = { name, path: last && !dir ? path : `${here}/`, dir: !last || dir, children: [] }
        level.push(node)
      }
      if (!last) node.dir = true
      level = node.children
    })
  }

  return sortLevel(roots)
}

function sortLevel(nodes: TreeNode[]): TreeNode[] {
  nodes.sort((left, right) => {
    if (left.dir !== right.dir) return left.dir ? -1 : 1
    return left.name.localeCompare(right.name)
  })
  for (const node of nodes) sortLevel(node.children)
  return nodes
}

function TreeLevel({
  nodes,
  depth,
  forceOpen,
  onOpen,
}: {
  nodes: TreeNode[]
  depth: number
  forceOpen: boolean
  onOpen: (path: string) => void
}) {
  const [open, setOpen] = useState<string[]>([])

  return (
    <>
      {nodes.map((node) => {
        const isOpen = forceOpen || open.includes(node.path)

        if (!node.dir) {
          return (
            <li key={node.path} style={{ paddingLeft: 4 + depth * 12 }}>
              <button
                className="code-tree-file"
                onClick={() => onOpen(node.path)}
                title={node.path}
              >
                <FileIcon size={12} />
                <span className="mono">{node.name}</span>
              </button>
            </li>
          )
        }

        return (
          <li key={node.path}>
            <button
              className="code-tree-dir"
              style={{ paddingLeft: 4 + depth * 12 }}
              aria-expanded={isOpen}
              onClick={() =>
                setOpen((held) =>
                  held.includes(node.path)
                    ? held.filter((path) => path !== node.path)
                    : [...held, node.path],
                )
              }
              title={node.path}
            >
              <ChevronIcon size={11} className={isOpen ? 'is-open' : ''} />
              <FolderIcon size={12} />
              <span className="mono">{node.name}</span>
            </button>

            {isOpen && node.children.length > 0 && (
              <ul className="code-tree">
                <TreeLevel
                  nodes={node.children}
                  depth={depth + 1}
                  forceOpen={forceOpen}
                  onOpen={onOpen}
                />
              </ul>
            )}
          </li>
        )
      })}
    </>
  )
}

function FileViewer({ id, path, onClose }: { id: string; path: string; onClose: () => void }) {
  const file = useQuery({
    queryKey: ['workspace-file', id, path],
    queryFn: () => api.workspaces.file(id, path),
    retry: false,
  })

  return (
    <div className="file-viewer">
      <div className="file-viewer-head">
        <FileIcon size={13} />
        <span className="mono file-viewer-path" title={path}>
          {path}
        </span>
        <button
          className="btn btn-ghost btn-icon"
          onClick={onClose}
          aria-label="Close this file"
          title="Close"
        >
          <CloseIcon size={14} />
        </button>
      </div>

      {file.isLoading && <p className="faint code-panel-note">Reading…</p>}
      {file.isError && (
        <p className="form-error">
          {file.error instanceof Error ? file.error.message : 'That file could not be read.'}
        </p>
      )}
      {file.data && (
        <CodeBlock
          text={file.data.content}
          className="is-filled"
          label={`Copy the contents of ${path}`}
        >
          <pre className="file-viewer-body mono">
            {file.data.content || '(this file is empty)'}
          </pre>
        </CodeBlock>
      )}
    </div>
  )
}

function ChangeList({ id }: { id: string }) {
  const queryClient = useQueryClient()
  const [failed, setFailed] = useState<string | null>(null)
  const [opened, setOpened] = useState<string | null>(null)

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
    return (
      <>
        <p className="faint code-panel-note">Nothing has been changed yet.</p>
        <p className="faint code-panel-note">
          Commands are not listed here. A file edit can be put back; a command that has
          already run cannot.
        </p>
      </>
    )
  }

  return (
    <>
      {failed && <p className="form-error">{failed}</p>}
      <ul className="code-changes">
        {held.map((change) => (
          <li key={change.id} className={change.undone ? 'is-undone' : ''}>
            <div className="code-change-head">
              <button
                className="mono code-change-path"
                onClick={() => setOpened(opened === change.id ? null : change.id)}
                aria-expanded={opened === change.id}
                title={opened === change.id ? 'Hide the changes' : 'Show what changed'}
              >
                {change.path}
              </button>
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
            {opened === change.id && (
              <DiffView
                workspaceId={id}
                changeId={change.id}
                path={change.path}
                onClose={() => setOpened(null)}
              />
            )}
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
      return 'Agent mode. Describe a change and the model will make it, then build it and run the tests. Every edit can be undone.'
    case 'bypass':
      return 'Bypass mode. Same as Agent, with no limit on what commands it may run.'
  }
}

function placeholderFor(mode: WorkspaceMode): string {
  switch (mode) {
    case 'ask':
      return 'Ask about code…'
    case 'plan':
      return 'Ask about this project…'
    case 'agent':
    case 'bypass':
      return 'Describe a change…'
  }
}

function modeNote(mode: WorkspaceMode): string {
  switch (mode) {
    case 'ask':
      return 'Ask mode: no access to this folder.'
    case 'plan':
      return 'Plan mode: can read this folder, and cannot change it or run anything.'
    case 'agent':
      return 'Agent mode: can change files here and run build and test commands. Every edit can be undone.'
    case 'bypass':
      return 'Bypass mode: can change files here and run any command, with no allowlist.'
  }
}
