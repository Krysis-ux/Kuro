import { useEffect, useState } from 'react'

interface SliderFieldProps {
  label: string
  /** What "Auto" resolves to on this machine, shown rather than hidden. */
  autoValue: number | undefined
  hint: string
  value: unknown
  min: number
  max: number
  step: number
  /** Rendered next to the number, e.g. `tokens`. */
  unit?: string
  /** `null` clears the override and returns the field to Auto. */
  onSave: (value: number | null) => void
  /** Label for the lowest position when it means something other than a number. */
  zeroLabel?: string
}

/**
 * A number with a slider and an exact field.
 *
 * Both, deliberately. The slider is for the common case of nudging a value until
 * it feels right, and typing is for the case where you know you want 16384 and do
 * not want to hunt for it — dragging to an exact power of two is miserable.
 *
 * Nothing is written until the drag ends or the field is committed. A slider that
 * saved on every pixel would write dozens of settings rows per gesture, and each
 * one invalidates the hardware query.
 */
export function SliderField({
  label,
  autoValue,
  hint,
  value,
  min,
  max,
  step,
  unit,
  onSave,
  zeroLabel,
}: SliderFieldProps) {
  const stored = typeof value === 'number' ? value : null
  const isAuto = stored === null

  // The slider needs a position even when the value is Auto, so it starts where
  // Auto lands and the thumb never sits at zero pretending to be a choice.
  const effective = stored ?? autoValue ?? min
  const [draft, setDraft] = useState(effective)
  const [typed, setTyped] = useState('')
  const [editing, setEditing] = useState(false)

  // Adopt a newly loaded or externally changed value, without clobbering a drag.
  useEffect(() => {
    if (!editing) setDraft(effective)
  }, [effective, editing])

  const commit = (next: number) => {
    const clamped = Math.min(Math.max(next, min), max)
    setDraft(clamped)
    onSave(clamped)
  }

  const commitTyped = () => {
    setEditing(false)
    const trimmed = typed.trim()
    setTyped('')
    if (trimmed === '') return

    const parsed = Number(trimmed)
    if (!Number.isFinite(parsed)) return
    commit(parsed)
  }

  const display = () => {
    if (isAuto) return `Auto · ${formatValue(autoValue, unit, zeroLabel)}`
    return formatValue(draft, unit, zeroLabel)
  }

  return (
    <div className="slider-field">
      <div className="slider-head">
        <span className="slider-label">{label}</span>

        {editing ? (
          <input
            className="input slider-input"
            type="number"
            min={min}
            max={max}
            autoFocus
            placeholder={String(effective)}
            value={typed}
            onChange={(event) => setTyped(event.target.value)}
            onBlur={commitTyped}
            onKeyDown={(event) => {
              if (event.key === 'Enter') commitTyped()
              if (event.key === 'Escape') {
                setTyped('')
                setEditing(false)
              }
            }}
          />
        ) : (
          <button
            className={`slider-value mono ${isAuto ? 'is-auto' : ''}`}
            onClick={() => {
              setTyped(String(effective))
              setEditing(true)
            }}
            title="Click to type an exact value"
          >
            {display()}
          </button>
        )}
      </div>

      <input
        className="slider"
        type="range"
        min={min}
        max={max}
        step={step}
        value={draft}
        aria-label={label}
        onChange={(event) => {
          setDraft(Number(event.target.value))
          setEditing(false)
        }}
        // Written on release rather than on every frame.
        onPointerUp={() => commit(draft)}
        onKeyUp={() => commit(draft)}
      />

      <div className="slider-foot">
        <span className="faint">{hint}</span>
        {!isAuto && (
          <button className="link-button faint" onClick={() => onSave(null)}>
            Reset to auto
          </button>
        )}
      </div>
    </div>
  )
}

function formatValue(value: number | undefined, unit?: string, zeroLabel?: string): string {
  if (value === undefined) return '—'
  if (value === 0 && zeroLabel) return zeroLabel
  return unit ? `${value.toLocaleString()} ${unit}` : value.toLocaleString()
}
