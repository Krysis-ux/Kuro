import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime, type RunningProcess, type WorkspaceMode } from '../lib/api'
import { CodeBlock } from './CodeBlock'
import {
  CloseIcon,
  EyeIcon,
  ExternalIcon,
  PlayIcon,
  RefreshIcon,
  StopIcon,
  TerminalIcon,
  TrashIcon,
} from './icons'

const POLL_MS = 2000

export function PreviewPanel({
  workspaceId,
  mode,
  view = 'terminal',
}: {
  workspaceId: string
  mode: WorkspaceMode
  view?: 'terminal' | 'browser'
}) {
  const queryClient = useQueryClient()
  const [selected, setSelected] = useState<string | null>(null)
  const [command, setCommand] = useState('')
  const [startError, setStartError] = useState<string | null>(null)
  const [frameKey, setFrameKey] = useState(0)

  const processes = useQuery({
    queryKey: ['workspace-processes', workspaceId],
    queryFn: () => api.workspaces.processes(workspaceId),
    refetchInterval: (query) =>
      query.state.data?.processes.some((held) => held.running) ? POLL_MS : false,
  })

  const held = processes.data?.processes ?? []
  const active = held.find((process) => process.id === selected) ?? held.find((p) => p.running) ?? held[0]

  const start = useMutation({
    mutationFn: (line: string) => api.workspaces.startProcess(workspaceId, line),
    onSuccess: (result) => {
      setCommand('')
      setStartError(null)
      setSelected(result.process.id)
      void queryClient.invalidateQueries({ queryKey: ['workspace-processes', workspaceId] })
    },
    onError: (caught) =>
      setStartError(caught instanceof Error ? caught.message : 'That command could not start.'),
  })

  const refresh = () =>
    void queryClient.invalidateQueries({ queryKey: ['workspace-processes', workspaceId] })

  const stop = useMutation({
    mutationFn: (processId: string) => api.workspaces.stopProcess(workspaceId, processId),
    onSuccess: refresh,
  })

  const forget = useMutation({
    mutationFn: (processId: string) => api.workspaces.forgetProcess(workspaceId, processId),
    onSuccess: refresh,
  })

  const clearFinished = useMutation({
    mutationFn: () => api.workspaces.clearProcesses(workspaceId),
    onSuccess: refresh,
  })

  const finished = held.filter((process) => !process.running).length

  const canRun = mode === 'agent' || mode === 'bypass'

  if (!canRun) {
    return (
      <p className="faint code-panel-note">
        {mode === 'ask' ? 'Ask' : 'Plan'} mode cannot run commands, so nothing can be started
        here. Switch to Agent to build the project, run its tests, or start a dev server.
      </p>
    )
  }

  return (
    <div className="preview-panel">
      {view === 'terminal' && (
      <form
        className="inline-form preview-start"
        onSubmit={(event) => {
          event.preventDefault()
          if (command.trim()) start.mutate(command.trim())
        }}
      >
        <input
          className="input mono"
          placeholder="npm run dev"
          value={command}
          onChange={(event) => setCommand(event.target.value)}
        />
        <button
          className="btn btn-solid btn-sm"
          type="submit"
          disabled={!command.trim() || start.isPending}
          title="Start this in the background"
        >
          <PlayIcon size={13} />
          Start
        </button>
      </form>
      )}

      {startError && <p className="form-error">{startError}</p>}

      {finished > 1 && (
        <button
          className="btn btn-ghost btn-sm preview-clear"
          onClick={() => clearFinished.mutate()}
          disabled={clearFinished.isPending}
        >
          <TrashIcon size={12} />
          Clear {finished} finished
        </button>
      )}

      {held.length === 0 && (
        <p className="faint code-panel-note">
          Nothing running. Start a dev server above, or ask the model to — it can do this
          itself, and the page appears here when it does.
        </p>
      )}

      {held.length > 0 && (
        <ul className="process-list">
          {held.map((process) => (
            <li key={process.id}>
              <button
                className={`process-row ${process.id === active?.id ? 'is-on' : ''}`}
                onClick={() => setSelected(process.id)}
              >
                <span className={`status-dot ${process.running ? 'status-connected' : ''}`} />
                <span className="process-row-main">
                  <span className="mono process-row-command">{process.command}</span>
                  <span className="faint process-row-note">{describe(process)}</span>
                </span>
              </button>
              {process.running ? (
                <button
                  className="btn btn-ghost btn-icon"
                  aria-label={`Stop ${process.command}`}
                  title="Stop"
                  onClick={() => stop.mutate(process.id)}
                >
                  <StopIcon size={13} />
                </button>
              ) : (
                <button
                  className="btn btn-ghost btn-icon"
                  aria-label={`Clear ${process.command}`}
                  title="Clear this from the list"
                  onClick={() => forget.mutate(process.id)}
                >
                  <CloseIcon size={13} />
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {view === 'browser' && !(active?.url && active.running) && (
        <p className="faint code-panel-note">
          Nothing is serving a page yet. Start a dev server in the terminal and its address
          appears here as soon as it prints one.
        </p>
      )}

      {view === 'terminal' && active && (
        <ProcessLog workspaceId={workspaceId} process={active} />
      )}

      {view === 'browser' && active?.url && active.running && (
        <div className="preview-frame-wrap">
          <div className="preview-frame-bar">
            <EyeIcon size={12} />
            <span className="mono faint preview-frame-url">{active.url}</span>
            <button
              className="btn btn-ghost btn-icon"
              aria-label="Reload the preview"
              title="Reload"
              onClick={() => setFrameKey((value) => value + 1)}
            >
              <RefreshIcon size={12} />
            </button>
            <a
              className="btn btn-ghost btn-icon"
              href={active.url}
              target="_blank"
              rel="noopener noreferrer"
              aria-label="Open in a real browser tab"
              title="Open in a browser tab"
            >
              <ExternalIcon size={12} />
            </a>
          </div>
          <iframe
            key={frameKey}
            className="preview-frame"
            src={active.url}
            title="Preview of the running app"
            sandbox="allow-scripts allow-forms allow-same-origin"
          />
          <p className="faint preview-frame-note">
            This is a frame around your dev server. If it stays blank, the page may refuse to
            be framed — open it in a browser tab instead.
          </p>
        </div>
      )}
    </div>
  )
}

function ProcessLog({
  workspaceId,
  process,
}: {
  workspaceId: string
  process: RunningProcess
}) {
  const bottomRef = useRef<HTMLDivElement>(null)

  const log = useQuery({
    queryKey: ['workspace-process-log', workspaceId, process.id],
    queryFn: () => api.workspaces.processLog(workspaceId, process.id),
    refetchInterval: process.running ? POLL_MS : false,
  })

  const lines = log.data?.lines ?? []

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: 'end' })
  }, [lines.length])

  return (
    <div className="process-log">
      <div className="process-log-head">
        <TerminalIcon size={12} />
        <span className="faint">Output</span>
      </div>
      {lines.length === 0 ? (
        <p className="faint code-panel-note">
          {process.running ? 'Nothing printed yet.' : 'It printed nothing before exiting.'}
        </p>
      ) : (
        <CodeBlock
          text={lines.join('\n')}
          className="is-filled"
          label={`Copy the output of ${process.command}`}
        >
          <pre className="process-log-body mono">
            {lines.join('\n')}
            <div ref={bottomRef} />
          </pre>
        </CodeBlock>
      )}
    </div>
  )
}

function describe(process: RunningProcess): string {
  if (!process.running) {
    const code = process.exit_code
    return code === 0 ? 'finished' : `exited with ${code ?? 'an unknown code'}`
  }
  if (process.url) return `serving ${process.url} · started ${relativeTime(process.started_at)}`
  return `running · started ${relativeTime(process.started_at)}`
}
