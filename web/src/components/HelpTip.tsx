import { useEffect, useRef, useState } from 'react'
import { QuestionIcon } from './icons'

/**
 * A question mark that explains a feature in a small popover, so the page itself needs only a title
 * and a few words.
 */
export function HelpTip({ text }: { text: string }) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLSpanElement>(null)

  useEffect(() => {
    if (!open) return
    const close = (event: Event) => {
      if (event instanceof KeyboardEvent ? event.key === 'Escape' : !ref.current?.contains(event.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', close)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', close)
    }
  }, [open])

  return (
    <span className="help-tip" ref={ref}>
      <button
        type="button"
        className="help-tip-button"
        aria-label="About this"
        aria-expanded={open}
        title={open ? undefined : text}
        onClick={(event) => {
          event.stopPropagation()
          setOpen((shown) => !shown)
        }}
      >
        <QuestionIcon size={13} />
      </button>
      {open && (
        <span className="help-tip-pop" role="tooltip">
          {text}
        </span>
      )}
    </span>
  )
}

/** A title with an optional help tip beside it. */
export function TitleWithHelp({ title, help }: { title: string; help?: string }) {
  return (
    <span className="title-help">
      {title}
      {help && <HelpTip text={help} />}
    </span>
  )
}
