import { useEffect, useState } from 'react'

interface SliderFieldProps {
  label: string
  autoValue: number | undefined
  hint: string
  value: unknown
  min: number
  max: number
  step: number
  unit?: string
  onSave: (value: number | null) => void
  zeroLabel?: string
}

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

  const effective = stored ?? autoValue ?? min
  const [draft, setDraft] = useState(effective)
  const [typed, setTyped] = useState('')
  const [editing, setEditing] = useState(false)

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
