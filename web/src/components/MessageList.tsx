import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { isOptimistic, type Message } from '../lib/api'
import type { StreamingState } from '../lib/useTurn'
import { CodeBlock, MarkdownPre } from './CodeBlock'
import { CheckIcon, ChevronIcon, ExternalIcon, GlobeIcon, InfoIcon, ToolIcon } from './icons'
import { Logo } from './Logo'
import { MessageActions } from './MessageActions'

/**
 * How a reply's markdown is rendered.
 *
 * Defined once, at module scope, because react-markdown compares this object by
 * identity — building it inside the component would rebuild every code block on
 * every streamed token, which both flickers and loses the "Copied" state of a
 * button somebody has just pressed.
 */
const MARKDOWN_COMPONENTS = { pre: MarkdownPre } as const

// Defined with the streaming loop that produces them, and re-exported here
// because this is where callers already import the transcript from.
export type { StreamingState, StreamingTool } from '../lib/useTurn'

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
}

export function MessageList({
  messages,
  streaming,
  error,
  notices = [],
  onFork,
  onEdit,
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
        />
      ))}

      {streaming && <StreamingRow state={streaming} />}

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
}

function MessageRow({ message, actionable, onFork, onEdit }: MessageRowProps) {
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
              <div className="bubble">{message.content}</div>
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
              detail: tool.preview,
            }))}
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

function StreamingRow({ state }: { state: StreamingState }) {
  const running = state.tools.some((tool) => tool.state === 'running')

  return (
    <div className="message message-assistant">
      <div className="message-avatar">
        <Logo size={15} />
      </div>
      <div className="message-body">
        {state.tools.length > 0 && (
          <ToolActivity
            steps={state.tools.map((tool) => ({
              name: tool.name,
              state: tool.state,
              query: describeArguments(tool.arguments),
              detail: tool.preview,
            }))}
            defaultOpen
          />
        )}

        {state.reasoning && <Reasoning text={state.reasoning} defaultOpen />}

        {state.content ? (
          <div className="prose">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={MARKDOWN_COMPONENTS}>
              {state.content}
            </ReactMarkdown>
          </div>
        ) : (
          <div className="thinking">
            <span className="spinner" />
            <span className="faint">{running ? 'Using tools…' : 'Thinking…'}</span>
          </div>
        )}
      </div>
    </div>
  )
}

/** One tool call as the activity panel shows it. */
interface ToolStep {
  name: string
  state: 'running' | 'done' | 'failed'
  /** The argument the call was made with, when there is a readable one. */
  query?: string
  /** What came back, truncated by the server. */
  detail?: string
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
}: {
  steps: ToolStep[]
  defaultOpen?: boolean
}) {
  const [open, setOpen] = useState(defaultOpen)

  const running = steps.some((step) => step.state === 'running')
  const failed = steps.filter((step) => step.state === 'failed').length
  const summary = running
    ? `Using ${steps[steps.length - 1]?.name ?? 'a tool'}…`
    : `${steps.length} tool ${steps.length === 1 ? 'call' : 'calls'}${
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
            <li key={`${step.name}-${index}`} className={`tool-step is-${step.state}`}>
              <div className="tool-step-head">
                {step.state === 'running' ? (
                  <span className="spinner spinner-xs" />
                ) : step.state === 'done' ? (
                  <CheckIcon size={11} />
                ) : (
                  <span className="tool-step-cross">×</span>
                )}
                <code className="mono">{step.name}</code>
                {step.query && <span className="faint tool-step-query">{step.query}</span>}
              </div>
              {step.detail && (
                <CodeBlock
                  text={step.detail}
                  className="is-inline"
                  label={`Copy what ${step.name} returned`}
                >
                  <pre className="tool-step-detail">{step.detail}</pre>
                </CodeBlock>
              )}
            </li>
          ))}
        </ol>
      )}
    </div>
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

/** The first string argument, which is the one worth showing in a tooltip. */
function describeArguments(args: Record<string, unknown>): string {
  const first = Object.values(args).find((value) => typeof value === 'string')
  return typeof first === 'string' ? first : ''
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '')
  } catch {
    return url
  }
}
