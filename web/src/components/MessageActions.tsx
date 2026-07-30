import { useEffect, useState } from 'react'
import { BranchIcon, CheckIcon, CopyIcon, PencilIcon } from './icons'

/** How long the copy button stays confirmed before returning to normal. */
const COPIED_FOR_MS = 1500

interface MessageActionsProps {
  /** Text the copy button puts on the clipboard. */
  content: string
  /** Branch a new conversation ending at this message. */
  onFork: () => void
  /** Rewrite this message. Absent on turns that cannot be edited. */
  onEdit?: () => void
  /** Right-aligned under a user bubble, left-aligned under a reply. */
  align?: 'start' | 'end'
}

/**
 * The row of controls under a message.
 *
 * Deliberately in normal document flow rather than floating over the bubble.
 * A message changes height while it streams, while thinking is expanded, and
 * while the tool trail or the inspector is open — anything positioned over that
 * would have to be re-measured on each of those, and would land on top of a
 * neighbouring message whenever it was not. Keeping the row in flow makes that
 * class of bug impossible rather than merely handled.
 *
 * For the same reason the row is revealed with `visibility`, not `display`: the
 * space is reserved whether or not the pointer is over the message, so nothing
 * on the page shifts when it appears.
 */
export function MessageActions({ content, onFork, onEdit, align = 'start' }: MessageActionsProps) {
  const [copied, setCopied] = useState(false)

  // Clears itself, and cancels if the message unmounts first — a fork navigates
  // away mid-timeout, which would otherwise set state on a gone component.
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
      // Denied clipboard permission, or an insecure origin. Saying nothing is
      // wrong, but a failed copy is not worth an error banner over the chat.
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
