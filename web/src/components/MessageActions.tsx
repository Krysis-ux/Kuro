import { useEffect, useState } from 'react'
import { BranchIcon, CheckIcon, CopyIcon, PencilIcon } from './icons'

const COPIED_FOR_MS = 1500

interface MessageActionsProps {
  content: string
  onFork: () => void
  onEdit?: () => void
  align?: 'start' | 'end'
}

export function MessageActions({ content, onFork, onEdit, align = 'start' }: MessageActionsProps) {
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), COPIED_FOR_MS)
    return () => clearTimeout(timer)
  }, [copied])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(content)
      setCopied(true)
    } catch {
      setCopied(false)
    }
  }

  return (
    <div className={`message-actions is-${align}`}>
      <button
        type="button"
        className="message-action"
        onClick={() => void copy()}
        title={copied ? 'Copied' : 'Copy'}
        aria-label={copied ? 'Copied' : 'Copy message'}
      >
        {copied ? <CheckIcon size={13} /> : <CopyIcon size={13} />}
      </button>

      <button
        type="button"
        className="message-action"
        onClick={onFork}
        title="Fork from here"
        aria-label="Fork the conversation from this message"
      >
        <BranchIcon size={13} />
      </button>

      {onEdit && (
        <button
          type="button"
          className="message-action"
          onClick={onEdit}
          title="Edit"
          aria-label="Edit this message"
        >
          <PencilIcon size={13} />
        </button>
      )}
    </div>
  )
}
