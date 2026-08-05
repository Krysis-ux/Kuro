import { useEffect, useState, type ReactNode } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { isOptimistic, type Message } from '../lib/api'
import type { StreamingState } from '../lib/turns'
import { CodeBlock, MarkdownPre } from './CodeBlock'
import {
  CheckIcon,
  ChevronIcon,
  ExternalIcon,
  FileIcon,
  GlobeIcon,
  InfoIcon,
  ListIcon,
  PencilIcon,
  SearchIcon,
  TerminalIcon,
  ToolIcon,
} from './icons'
import { Logo } from './Logo'
import { MessageActions } from './MessageActions'
import { RichText } from './RichText'

/**
 * How a reply's markdown is rendered.
 *
 * Defined once, at module scope, because react-markdown compares this object by
 * identity — building it inside the component would rebuild every code block on
 * every streamed token, which both flickers and loses the "Copied" state of a
 * button somebody has just pressed.
 */
const MARKDOWN_COMPONENTS = { pre: MarkdownPre, a: MarkdownLink } as const

/**
 * A link in a reply.
 *
 * Given its own colour and always opened in a new tab. Both are the same point:
 * a link is the one part of an answer that can be *checked*, and it should
 * neither look like the prose around it nor cost you the conversation to
 * follow.
 */
function MarkdownLink({ href, children }: { href?: string; children?: ReactNode }) {
  return (
    <a className="rich-link" href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  )
}

// Defined with the streaming loop that produces them, and re-exported here
// because this is where callers already import the transcript from.
export type { StreamingState, StreamingTool } from '../lib/turns'

interface MessageListProps {
  messages: Message[]
  streaming: StreamingState | null
  error: string | null
  /** Things that went wrong without ending the turn. */
  notices?: string[]
  /** Branch a new conversation ending at this message. */
  onFork: (messageId: string) => void
  /** Replace a user message and answer again from there. */
  onEdit: (messageId: string, content: string) => void
  /**
   * Open a file the turn touched.
   *
   * Absent on surfaces that have nowhere to show one — a chat can read a
   * project but has no file panel — and the paths render as plain text there
   * rather than as buttons that would do nothing.
   */
  onOpenPath?: (path: string) => void
}

export function MessageList({
  messages,
  streaming,
  error,
  notices = [],
  onFork,
  onEdit,
  onOpenPath,
}: MessageListProps) {
  return (
    <div className="messages">
      {messages.map((message) => (
        <MessageRow
          key={message.id}
          message={message}
          // A turn still being generated has nothing to fork or edit yet, and
          // an optimistic row has no id the server would recognise.
          actionable={!isOptimistic(message.id) && streaming === null}
          onFork={() => onFork(message.id)}
          onEdit={(content) => onEdit(message.id, content)}
          onOpenPath={onOpenPath}
        />
      ))}

      {streaming && <StreamingRow state={streaming} onOpenPath={onOpenPath} />}

      {notices.map((notice, index) => (
        <div key={`${notice}-${index}`} className="message message-assistant">
          <div className="message-notice">{notice}</div>
        </div>
      ))}

      {error && (
        <div className="message message-assistant">
          <div className="message-error">{error}</div>
        </div>
      )}
    </div>
  )
}

interface MessageRowProps {
  message: Message
  actionable: boolean
  onFork: () => void
  onEdit: (content: string) => void
  onOpenPath?: (path: string) => void
}

function MessageRow({ message, actionable, onFork, onEdit, onOpenPath }: MessageRowProps) {
  const [showDetails, setShowDetails] = useState(false)
  const [editing, setEditing] = useState(false)

  if (message.role === 'user') {
    return (
      <div className="message message-user">
        <div className="message-user-col">
          {editing ? (
            <MessageEditor
              initial={message.content}
              onCancel={() => setEditing(false)}
              onSubmit={(content) => {
                setEditing(false)
                onEdit(content)
              }}
            />
          ) : (
            <>
              <div className="bubble">
                <RichText text={message.content} />
              </div>
              {actionable && (
                <MessageActions
                  content={message.content}
                  onFork={onFork}
                  onEdit={() => setEditing(true)}
                  align="end"
                />
              )}
            </>
          )}
        </div>
      </div>
    )
  }

  const tools = message.tool_calls ?? []
  const sources = message.web_sources ?? []

  // An assistant row with no text is a turn that failed before producing
  // anything; showing an empty bubble would be more confusing than hiding it.
  if (!message.content.trim() && !message.reasoning_content && tools.length === 0) return null

  const hasStats =
    message.usage_completion_tokens !== null || message.timing_tokens_per_sec !== null

  return (
    <div className="message message-assistant">
      <div className="message-avatar">
        <Logo size={15} />
      </div>

      <div className="message-body">
        {tools.length > 0 && (
          <ToolActivity
            steps={tools.map((tool) => ({
              name: tool.name,
              state: tool.ok ? 'done' : 'failed',
              args: tool.arguments,
              detail: tool.preview,
            }))}
            onOpenPath={onOpenPath}
          />
        )}

        {message.reasoning_content && <Reasoning text={message.reasoning_content} />}

        <div className="prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN_COMPONENTS}>
            {message.content}
          </ReactMarkdown>
        </div>

        {sources.length > 0 && <Sources sources={sources} />}

        {hasStats && (
          <div className="message-meta">
            <button className="meta-toggle faint" onClick={() => setShowDetails((open) => !open)}>
              <InfoIcon size={12} />
              {showDetails ? 'Hide details' : 'Details'}
            </button>

            {showDetails && (
              <dl className="inspector">
                <Stat label="Model" value={message.model_id ?? '—'} />
                <Stat label="Prompt tokens" value={message.usage_prompt_tokens} />
                <Stat label="Output tokens" value={message.usage_completion_tokens} />
                <Stat
                  label="Speed"
                  value={
                    message.timing_tokens_per_sec
                      ? `${message.timing_tokens_per_sec.toFixed(1)} tok/s`
                      : null
                  }
                />
                <Stat
                  label="First token"
                  value={message.timing_ttft_ms ? `${message.timing_ttft_ms} ms` : null}
                />
                <Stat
                  label="Total"
                  value={
                    message.timing_total_ms
                      ? `${(message.timing_total_ms / 1000).toFixed(2)} s`
                      : null
                  }
                />
                <Stat label="Tool calls" value={tools.length > 0 ? tools.length : null} />
                <Stat label="Web search" value={message.used_web_search ? 'yes' : null} />
                <Stat label="Stop reason" value={message.finish_reason} />
              </dl>
            )}
          </div>
        )}

        {actionable && (
          <MessageActions content={message.content} onFork={onFork} />
        )}
      </div>
    </div>
  )
}

/**
 * A user message being rewritten.
 *
 * Sends on Enter and cancels on Escape, matching the composer, since this is
 * the same act of writing a message. Submitting is what truncates the
 * conversation, so an accidental Enter is worth guarding: an unchanged or empty
 * message just closes the editor rather than discarding the replies below it.
 */
function MessageEditor({
  initial,
  onSubmit,
  onCancel,
}: {
  initial: string
  onSubmit: (content: string) => void
  onCancel: () => void
}) {
  const [draft, setDraft] = useState(initial)

  const submit = () => {
    const content = draft.trim()
    if (content === '' || content === initial.trim()) {
      onCancel()
      return
    }
    onSubmit(content)
  }

  return (
    <div className="message-editor">
      <textarea
        className="message-editor-input"
        value={draft}
        autoFocus
        rows={Math.min(10, draft.split('\n').length + 1)}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault()
            submit()
          } else if (event.key === 'Escape') {
            onCancel()
          }
        }}
      />
      <div className="message-editor-actions">
        <button type="button" className="btn btn-ghost" onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="btn btn-solid" onClick={submit}>
          Send
        </button>
      </div>
      <p className="faint message-editor-note">
        Sending replaces this message. The replies after it are removed.
      </p>
    </div>
  )
}

function StreamingRow({
  state,
  onOpenPath,
}: {
  state: StreamingState
  onOpenPath?: (path: string) => void
}) {
  const running = state.tools.some((tool) => tool.state === 'running')

  return (
    <div className="message message-assistant is-live">
      <div className="message-avatar">
        <Logo size={15} />
      </div>
      <div className="message-body">
        {state.tools.length > 0 && (
          <ToolActivity
            steps={state.tools.map((tool) => ({
              name: tool.name,
              state: tool.state,
              args: tool.arguments,
              detail: tool.preview,
            }))}
            defaultOpen
            onOpenPath={onOpenPath}
          />
        )}

        {state.reasoning && <Reasoning text={state.reasoning} defaultOpen />}

        {state.content && (
          <div className="prose">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN_COMPONENTS}>
              {state.content}
            </ReactMarkdown>
          </div>
        )}

        {/*
          Always shown while the turn is open, not only before the first token.

          It used to be an either/or with the reply — spinner until text
          arrived, then nothing — so the longest and most anxious part of a
          coding turn, the minutes between one paragraph of explanation and the
          next tool call, had no sign of life on screen at all. There is a real
          difference between "still working" and "stopped", and the interface
          owes an answer to which one is happening.
        */}
        <Working
          label={running ? 'Working' : state.content ? 'Still going' : 'Thinking'}
          since={state.touchedAt}
        />
      </div>
    </div>
  )
}

/**
 * The heartbeat under a turn in progress.
 *
 * Counts up from the last event rather than from the start of the turn, so it
 * answers the question people actually have — how long since anything happened
 * — and stays silent for the first few seconds, when nothing is wrong yet and a
 * timer would only be noise.
 */
function Working({ label, since }: { label: string; since: number }) {
  const [quiet, setQuiet] = useState(0)

  useEffect(() => {
    setQuiet(0)
    const tick = window.setInterval(
      () => setQuiet(Math.round((Date.now() - since) / 1000)),
      1000,
    )
    return () => window.clearInterval(tick)
  }, [since])

  return (
    <div className="thinking" role="status" aria-live="polite">
      <span className="thinking-dots" aria-hidden="true">
        <i />
        <i />
        <i />
      </span>
      <span className="faint">{label}…</span>
      {quiet >= 5 && <span className="faint thinking-elapsed mono">{quiet}s</span>}
    </div>
  )
}

/** One tool call as the activity panel shows it. */
interface ToolStep {
  name: string
  state: 'running' | 'done' | 'failed'
  /** The call's arguments, which is what says *what* it was done to. */
  args?: Record<string, unknown>
  /** What came back, truncated by the server. */
  detail?: string
}

/** What a tool call was, in words rather than in its wire name. */
interface ToolAction {
  kind: 'read' | 'edit' | 'write' | 'run' | 'search' | 'list' | 'web' | 'other'
  verb: string
  /** The file, command or query it acted on. Rendered in mono. */
  target: string
  /** Set when `target` is a path in the workspace, so it can be opened. */
  path?: string
}

/**
 * Read a tool call as a sentence.
 *
 * The transcript used to print the wire name and then whichever argument
 * happened to be a string first — `edit_file` followed by a hundred characters
 * of the code being replaced. That is the raw call, not what happened, and it
 * is exactly backwards from what somebody watching wants: *which file*, and
 * *what did you do to it*. The arguments are known per tool, so there is no
 * reason to guess at them.
 */
function describeTool(name: string, args: Record<string, unknown> = {}): ToolAction {
  const text = (key: string): string | undefined => {
    const value = args[key]
    return typeof value === 'string' && value !== '' ? value : undefined
  }
  const count = (key: string): number | undefined => {
    const value = args[key]
    return typeof value === 'number' ? value : undefined
  }

  const path = text('path') ?? text('file') ?? text('file_path')

  switch (name) {
    case 'read_file': {
      const from = count('start_line')
      const to = count('end_line')
      // The range matters: "read lines 240-255" and "read the file" are
      // different acts, and only one of them explains a wrong answer.
      const range = from ? `:${from}${to ? `-${to}` : '+'}` : ''
      return { kind: 'read', verb: 'Read', target: `${path ?? 'a file'}${range}`, path }
    }
    case 'write_file':
      return { kind: 'write', verb: 'Wrote', target: path ?? 'a file', path }
    case 'edit_file':
      return { kind: 'edit', verb: 'Edited', target: path ?? 'a file', path }
    case 'run_command':
      return { kind: 'run', verb: 'Ran', target: text('command') ?? '' }
    case 'start_server':
      return { kind: 'run', verb: 'Started', target: text('command') ?? '' }
    case 'stop_server':
      return { kind: 'run', verb: 'Stopped', target: text('command') ?? 'a process' }
    case 'check_server':
      return { kind: 'run', verb: 'Checked', target: text('command') ?? 'what is running' }
    case 'search_files':
      return { kind: 'search', verb: 'Searched for', target: text('query') ?? text('pattern') ?? '' }
    case 'find_files':
      return { kind: 'search', verb: 'Looked for', target: text('pattern') ?? '' }
    case 'project_tree':
      return { kind: 'list', verb: 'Listed', target: path ?? 'the project' }
    case 'web_search':
      return { kind: 'web', verb: 'Searched the web for', target: text('query') ?? '' }
    case 'fetch_url':
    case 'read_url':
      return { kind: 'web', verb: 'Fetched', target: text('url') ?? '' }
    case 'remember':
      return { kind: 'other', verb: 'Remembered', target: text('fact') ?? text('content') ?? '' }
    case 'recall':
      return { kind: 'other', verb: 'Recalled', target: text('query') ?? '' }
    default: {
      // An MCP tool, whose arguments this build has never seen. Its name is the
      // most honest thing available, so it is shown as the name.
      const first = Object.values(args).find((value) => typeof value === 'string')
      return {
        kind: 'other',
        verb: name.replace(/_/g, ' '),
        target: typeof first === 'string' ? first : '',
      }
    }
  }
}

const ACTION_ICON: Record<ToolAction['kind'], ReactNode> = {
  read: <FileIcon size={11} />,
  edit: <PencilIcon size={11} />,
  write: <PencilIcon size={11} />,
  run: <TerminalIcon size={11} />,
  search: <SearchIcon size={11} />,
  list: <ListIcon size={11} />,
  web: <GlobeIcon size={11} />,
  other: <ToolIcon size={11} />,
}

/**
 * What the model did before answering.
 *
 * Collapsed to a single line by default and expandable to show the query sent and
 * what came back. Hosted assistants all do some version of this, and the reason is
 * not decoration: when a model answers using a tool, the answer is only as good as
 * the tool result, and a user who cannot see that result has no way to tell a good
 * answer from a confident one.
 *
 * Open by default while streaming — that is exactly when a person is wondering
 * what the spinner is doing — and closed once the reply has arrived.
 */
function ToolActivity({
  steps,
  defaultOpen = false,
  onOpenPath,
}: {
  steps: ToolStep[]
  defaultOpen?: boolean
  onOpenPath?: (path: string) => void
}) {
  const [open, setOpen] = useState(defaultOpen)

  const running = steps.some((step) => step.state === 'running')
  const failed = steps.filter((step) => step.state === 'failed').length
  const current = steps[steps.length - 1]

  // While it is working, the summary says what it is working *on*. "Using a
  // tool…" is true of every turn and therefore tells you nothing; "Editing
  // src/lib/turns.ts" is the answer to the question actually being asked, which
  // is whether this is going anywhere.
  const now = current ? describeTool(current.name, current.args) : null
  const summary =
    running && now
      ? `${now.verb}${now.target ? ` ${now.target}` : ''}…`
      : `${steps.length} step${steps.length === 1 ? '' : 's'}${
          failed > 0 ? `, ${failed} failed` : ''
        }`

  return (
    <div className={`tool-activity ${open ? 'is-open' : ''}`}>
      <button className="tool-activity-head" onClick={() => setOpen((value) => !value)}>
        {running ? <span className="spinner spinner-xs" /> : <ToolIcon size={12} />}
        <span className="tool-activity-summary">{summary}</span>
        <ChevronIcon size={12} className="tool-activity-caret" />
      </button>

      {open && (
        <ol className="tool-activity-steps">
          {steps.map((step, index) => (
            <ToolStepRow
              key={`${step.name}-${index}`}
              step={step}
              onOpenPath={onOpenPath}
            />
          ))}
        </ol>
      )}
    </div>
  )
}

/**
 * One step, as a line rather than as a dump.
 *
 * The result is folded away unless the step failed. A successful `read_file`
 * returning four hundred lines of the file it read is noise — the interesting
 * thing about it is that it happened, and which file — whereas a failure is
 * only useful *with* its message, which is why that one opens itself.
 */
function ToolStepRow({
  step,
  onOpenPath,
}: {
  step: ToolStep
  onOpenPath?: (path: string) => void
}) {
  const [open, setOpen] = useState(false)
  const action = describeTool(step.name, step.args)
  const showDetail = open || step.state === 'failed'

  return (
    <li className={`tool-step is-${step.state} is-${action.kind}`}>
      <div className="tool-step-head">
        <span className="tool-step-mark">
          {step.state === 'running' ? (
            <span className="spinner spinner-xs" />
          ) : step.state === 'done' ? (
            <CheckIcon size={11} />
          ) : (
            <span className="tool-step-cross">×</span>
          )}
        </span>

        <span className="tool-step-icon faint">{ACTION_ICON[action.kind]}</span>
        <span className="tool-step-verb">{action.verb}</span>

        {action.target &&
          (action.path && onOpenPath ? (
            // Underlined and clickable, because a file the model touched is the
            // thing you most want to look at next and the transcript is where
            // you are already looking.
            <button
              className="tool-step-path mono"
              onClick={() => onOpenPath(action.path as string)}
              title={`Open ${action.path}`}
            >
              {action.target}
            </button>
          ) : (
            <code className="tool-step-target mono">{action.target}</code>
          ))}

        {step.detail && step.state !== 'failed' && (
          <button
            className="tool-step-more faint"
            onClick={() => setOpen((value) => !value)}
            aria-expanded={open}
          >
            {open ? 'hide' : 'output'}
          </button>
        )}
      </div>

      {step.detail && showDetail && (
        <CodeBlock
          text={step.detail}
          className="is-inline"
          label={`Copy what ${step.name} returned`}
        >
          <pre className="tool-step-detail">{step.detail}</pre>
        </CodeBlock>
      )}
    </li>
  )
}

/**
 * Pages the turn drew on.
 *
 * Listed by the interface rather than left to the model, because a small model
 * asked to cite its sources will frequently invent a plausible URL instead.
 */
function Sources({ sources }: { sources: { title: string; url: string }[] }) {
  return (
    <div className="sources">
      <div className="sources-head faint">
        <GlobeIcon size={12} />
        Sources
      </div>
      <ol className="sources-list">
        {sources.map((source) => (
          <li key={source.url}>
            <a href={source.url} target="_blank" rel="noopener noreferrer">
              <span className="sources-title">{source.title || source.url}</span>
              <span className="faint sources-host">{hostOf(source.url)}</span>
              <ExternalIcon size={11} />
            </a>
          </li>
        ))}
      </ol>
    </div>
  )
}

function Reasoning({ text, defaultOpen = false }: { text: string; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen)

  return (
    <div className="reasoning">
      <button className="reasoning-toggle faint" onClick={() => setOpen((value) => !value)}>
        {open ? 'Hide thinking' : 'Show thinking'}
      </button>
      {open && <div className="reasoning-body">{text}</div>}
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string | number | null }) {
  if (value === null || value === undefined) return null
  return (
    <div className="stat">
      <dt className="faint">{label}</dt>
      <dd className="mono">{value}</dd>
    </div>
  )
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return url
  }
}
