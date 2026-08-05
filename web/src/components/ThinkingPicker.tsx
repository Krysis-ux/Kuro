import { useEffect, useRef, useState } from 'react'
import type { Effort } from '../lib/api'
import { BoltIcon, BrainIcon, CheckIcon, ChevronIcon } from './icons'

/**
 * How hard to think, as one small control.
 *
 * This used to be four words in a row — low, balanced, high, max — sitting
 * permanently under every message box. Four visible options is a decision the
 * interface asks for on every single turn, and "balanced" versus "high" is not a
 * decision most people have an opinion about; it read as configuration left on
 * screen by accident.
 *
 * So it collapses to one word and a chevron. The word is the current setting,
 * which is the only part that was ever worth being able to see at a glance, and
 * the reasoning behind each option moves into the menu where there is room to
 * write it properly.
 *
 * The labels changed too. "Low" and "max" describe a number; "Instant" and
 * "Extended" describe what happens, and what happens is what somebody is
 * actually choosing between.
 */
interface Option {
  effort: Effort
  label: string
  detail: string
  /** Whether this setting makes the model reason before answering. */
  thinking: boolean
  /**
   * Hidden outside a coding workspace.
   *
   * A chat has nothing to spend the top level on — no project to read, no build
   * to run — so offering it there would make the whole control look arbitrary.
   */
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

/**
 * What an unrecognised stored effort falls back to.
 *
 * Balanced, because it is the default, and reading a stale setting as "maximum
 * thinking" would silently make every reply slower and more expensive.
 */
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
  /** Unlocks the coding-only level. */
  coding?: boolean
  /**
   * What raising the effort does here, shown at the foot of the menu.
   *
   * Different on each surface: in a chat it buys length, in a workspace it also
   * buys tool rounds and pulls in the skills that match the project. Saying so
   * is what stops the control reading as a mystery dial.
   */
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

  // Also covers `ultra` carried over from the Code page into a chat, which is
  // filtered out of the list below and would otherwise leave the trigger blank.
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
