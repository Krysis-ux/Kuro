import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime, type RunningProcess, type WorkspaceMode } from '../lib/api'
import { CodeBlock } from './CodeBlock'
import { EyeIcon, ExternalIcon, PlayIcon, RefreshIcon, StopIcon, TerminalIcon } from './icons'

/** How often the running list is re-read while something is alive. */
const POLL_MS = 2000

/**
 * What is running, and what it looks like.
 *
 * The missing half of a coding assistant. A model that can edit files and run
 * commands can tell you the build succeeded; it cannot tell you the page renders,
 * and neither could you without leaving the application to go and look.
 *
 * So a workspace's background processes get a panel. It lists them, shows their
 * output, and — when one of them has announced an address — puts that address in
 * a frame. The address is read out of the process's own output rather than
 * guessed at, which is why a server that has not printed anything yet shows a
 * log instead of a blank frame: there is genuinely nothing to point at, and an
 * empty frame would look like the page is broken rather than not started.
 *
 * ## What the frame can and cannot do
 *
 * It is an ordinary iframe pointed at localhost. That renders the page and it is
 * enough to see whether a layout is right. It is not a browser Kuro drives: a
 * site that refuses framing shows nothing, and the panel says so and offers to
 * open the address properly rather than pretending. Driving a real browser is
 * what the Playwright server on the Tools page is for.
 */
export function PreviewPanel({
  workspaceId,
  mode,
}: {
  workspaceId: string
  mode: WorkspaceMode
}) {
  const queryClient = useQueryClient()
  const [selected, setSelected] = useState<string | null>(null)
  const [command, setCommand] = useState('')
  const [startError, setStartError] = useState<string | null>(null)
  const [frameKey, setFrameKey] = useState(0)

  const processes = useQuery({
    queryKey: ['workspace-processes', workspaceId],
    queryFn: () => api.workspaces.processes(workspaceId),
    // Only poll while something is alive. A workspace with nothing running does
    // not need a request every two seconds for the rest of the afternoon.
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

  const stop = useMutation({
    mutationFn: (processId: string) => api.workspaces.stopProcess(workspaceId, processId),
    onSuccess: () =>
      void queryClient.invalidateQueries({ queryKey: ['workspace-processes', workspaceId] }),
  })

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

      {startError && <p className="form-error">{startError}</p>}

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
              {process.running && (
                <button
                  className="btn btn-ghost btn-icon"
                  aria-label={`Stop ${process.command}`}
                  title="Stop"
                  onClick={() => stop.mutate(process.id)}
                >
                  <StopIcon size={13} />
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      {active?.url && active.running ? (
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
            // The page being framed is the user's own dev server on loopback.
            // The sandbox still applies: it stops a page from navigating this
            // one away or opening things, which a half-written app can do by
            // accident.
            sandbox="allow-scripts allow-forms allow-same-origin"
          />
          <p className="faint preview-frame-note">
            This is a frame around your dev server. If it stays blank, the page may refuse to
            be framed — open it in a browser tab instead.
          </p>
        </div>
      ) : (
        active && <ProcessLog workspaceId={workspaceId} process={active} />
      )}
    </div>
  )
}

/**
 * A process's recent output.
 *
 * Shown instead of the frame when there is no address yet, because that is
 * exactly when somebody needs to know why: a missing script, a port already
 * taken, or a compile error all look identical from outside and are all in the
 * first ten lines of this.
 */
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
        // The output of a failed build is the single most-pasted thing in this
        // application, and selecting it by hand means dragging inside a pane
        // that is scrolling itself as new lines arrive.
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
