import { useEffect, useRef } from 'react'

/**
 * Are you sure — for the things that cannot be undone.
 *
 * Kuro undoes almost everything: a file a model edits keeps its previous
 * contents, a conversation can be forked rather than overwritten. Deleting a
 * model's weights is one of the few actions with nothing behind it, and it was
 * a single click on a small icon next to a list of similar-looking rows.
 *
 * So the confirming action carries the colour and the plain word for what it
 * does — "Delete", not "OK" — and the cancelling one is the default focus, so
 * that Enter and Escape both mean "no". The dialog is deliberately dull; a
 * frightening one would get clicked through just as fast.
 */
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  busy = false,
  onConfirm,
  onCancel,
}: {
  title: string
  body: React.ReactNode
  /** Names the action, never "OK". */
  confirmLabel: string
  busy?: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  const cancelRef = useRef<HTMLButtonElement>(null)

  // Focus starts on the way out, so a stray Enter cancels rather than deletes.
  useEffect(() => cancelRef.current?.focus(), [])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onCancel()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onCancel])

  return (
    <div
      className="confirm-backdrop"
      role="presentation"
      // Clicking away is a cancel. It is the same decision as Escape, and a
      // dialog you cannot dismiss by looking away from it is a trap.
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onCancel()
      }}
    >
      <div className="confirm" role="alertdialog" aria-modal="true" aria-label={title}>
        <h2 className="confirm-title">{title}</h2>
        <div className="confirm-body">{body}</div>
        <div className="confirm-actions">
          <button ref={cancelRef} className="btn btn-ghost" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
          <button className="btn btn-danger" onClick={onConfirm} disabled={busy}>
            {busy ? 'Working…' : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  )
}
