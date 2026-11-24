import { useEffect, useRef, useState } from 'react'
import type { Effort } from '../lib/api'
import { BoltIcon, BrainIcon, CheckIcon, ChevronIcon } from './icons'

interface Option {
  effort: Effort
  label: string
  detail: string
  thinking: boolean
  codingOnly?: boolean
}

const OPTIONS: readonly Option[] = [
  {
    effort: 'low',
    label: 'Instant',
    detail: 'Answers straight away, briefly. Best for quick questions.',
    thinking: false,
  },
  {
    effort: 'balanced',
    label: 'Balanced',
    detail: 'The default. Enough room to use a tool or two.',
    thinking: false,
  },
  {
    effort: 'high',
    label: 'Thinking',
    detail: 'Works through the problem, reads more, and checks itself.',
    thinking: true,
  },
  {
    effort: 'max',
    label: 'Extended',
    detail: 'The most thinking, the most tool use, the longest answers.',
    thinking: true,
  },
  {
    effort: 'ultra',
    label: 'Ultracode',
    detail:
      'Every coding skill, every tool, and as long as it takes. For a change that spans several files and has to be built and tested.',
    thinking: true,
    codingOnly: true,
  },
]

const FALLBACK: Option = {
  effort: 'balanced',
  label: 'Balanced',
  detail: 'The default. Enough room to use a tool or two.',
  thinking: false,
}

export function labelForEffort(effort: Effort): string {
  return OPTIONS.find((option) => option.effort === effort)?.label ?? 'Balanced'
}

interface ThinkingPickerProps {
  value: Effort
  onChange: (effort: Effort) => void
  coding?: boolean
  note?: string
}

export function ThinkingPicker({ value, onChange, note, coding = false }: ThinkingPickerProps) {
  const [open, setOpen] = useState(false)
  const anchorRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return

    const close = (event: MouseEvent) => {
      if (!anchorRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }

    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', escape)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', escape)
    }
  }, [open])

  const current: Option = OPTIONS.find((option) => option.effort === value) ?? FALLBACK

  return (
    <div className="menu-anchor" ref={anchorRef}>
      <button
        className={`btn btn-ghost composer-toggle thinking-trigger ${current.thinking ? 'is-on' : ''}`}
        onClick={() => setOpen((value) => !value)}
        aria-haspopup="menu"
        aria-expanded={open}
        title={current.detail}
      >
        {current.thinking ? <BrainIcon size={14} /> : <BoltIcon size={14} />}
        {/* Wrapped so a narrow composer can collapse it to the icon. A bare
            text node inherits `white-space: nowrap` from `.btn` and has no
            box to shrink, which is how it used to be painted over the model
            picker instead. */}
        <span className="thinking-trigger-label">{current.label}</span>
        <ChevronIcon size={11} />
      </button>

      {open && (
        <div className="menu thinking-menu fade-in" role="menu">
          <div className="menu-label">How hard to think</div>

          {OPTIONS.filter((option) => coding || !option.codingOnly).map((option) => (
            <button
              key={option.effort}
              className={`thinking-option ${option.effort === value ? 'is-on' : ''}`}
              role="menuitemradio"
              aria-checked={option.effort === value}
              onClick={() => {
                onChange(option.effort)
                setOpen(false)
              }}
            >
              <span className="thinking-option-check">
                {option.effort === value && <CheckIcon size={13} />}
              </span>
              <span className="thinking-option-main">
                <span className="thinking-option-label">{option.label}</span>
                <span className="faint thinking-option-detail">{option.detail}</span>
              </span>
            </button>
          ))}

          {note && <p className="faint thinking-note">{note}</p>}
        </div>
      )}
    </div>
  )
}
