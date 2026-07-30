import { useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { Message } from '../lib/api'
import { InfoIcon } from './icons'
import { Logo } from './Logo'

export interface StreamingState {
  content: string
  reasoning: string
}

interface MessageListProps {
  messages: Message[]
  streaming: StreamingState | null
  error: string | null
}

export function MessageList({ messages, streaming, error }: MessageListProps) {
  return (
    <div className="messages">
      {messages.map((message) => (
        <MessageRow key={message.id} message={message} />
      ))}

      {streaming && <StreamingRow state={streaming} />}

      {error && (
        <div className="message message-assistant">
          <div className="message-error">{error}</div>
        </div>
      )}
    </div>
  )
}

function MessageRow({ message }: { message: Message }) {
  const [showDetails, setShowDetails] = useState(false)

  if (message.role === 'user') {
    return (
      <div className="message message-user">
        <div className="bubble">{message.content}</div>
      </div>
    )
  }

  // An assistant row with no text is a turn that failed before producing
  // anything; showing an empty bubble would be more confusing than hiding it.
  if (!message.content.trim() && !message.reasoning_content) return null

  const hasStats =
    message.usage_completion_tokens !== null || message.timing_tokens_per_sec !== null

  return (
    <div className="message message-assistant">
      <div className="message-avatar">
        <Logo size={15} />
      </div>

      <div className="message-body">
        {message.reasoning_content && <Reasoning text={message.reasoning_content} />}

        <div className="prose">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{message.content}</ReactMarkdown>
        </div>

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
                <Stat label="Stop reason" value={message.finish_reason} />
              </dl>
            )}
          </div>
        )}
      </div>
    </div>
  )
}

function StreamingRow({ state }: { state: StreamingState }) {
  return (
    <div className="message message-assistant">
      <div className="message-avatar">
        <Logo size={15} />
      </div>
      <div className="message-body">
        {state.reasoning && <Reasoning text={state.reasoning} defaultOpen />}
        {state.content ? (
          <div className="prose">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{state.content}</ReactMarkdown>
          </div>
        ) : (
          <div className="thinking">
            <span className="spinner" />
            <span className="faint">Thinking…</span>
          </div>
        )}
      </div>
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
