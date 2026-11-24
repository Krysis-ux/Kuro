import { useCallback, useEffect, useRef, useState } from 'react'
import { PANEL_DEFAULTS } from '../store/ui'

interface PanelDividerProps {
  width: number
  onResize: (width: number) => void
  side: 'left' | 'right'
  label: string
}

const STEP = 16

export function PanelDivider({ width, onResize, side, label }: PanelDividerProps) {
  const [dragging, setDragging] = useState(false)
  const startRef = useRef({ pointer: 0, width })

  const direction = side === 'left' ? 1 : -1

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    event.preventDefault()
    startRef.current = { pointer: event.clientX, width }
    setDragging(true)
  }

  const move = useCallback(
    (event: PointerEvent) => {
      const travelled = event.clientX - startRef.current.pointer
      onResize(startRef.current.width + travelled * direction)
    },
    [direction, onResize],
  )

  useEffect(() => {
    if (!dragging) return

    const stop = () => setDragging(false)

    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', stop)
    window.addEventListener('pointercancel', stop)

    const previous = document.body.style.cursor
    document.body.style.cursor = 'col-resize'
    document.body.classList.add('is-resizing')

    return () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', stop)
      window.removeEventListener('pointercancel', stop)
      document.body.style.cursor = previous
      document.body.classList.remove('is-resizing')
    }
  }, [dragging, move])

  return (
    <div
      className={`panel-divider ${dragging ? 'is-dragging' : ''}`}
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={Math.round(width)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onDoubleClick={() => onResize(side === 'left' ? PANEL_DEFAULTS.files : PANEL_DEFAULTS.running)}
      onKeyDown={(event) => {
        if (event.key === 'ArrowLeft') {
          event.preventDefault()
          onResize(width - STEP * direction)
        } else if (event.key === 'ArrowRight') {
          event.preventDefault()
          onResize(width + STEP * direction)
        }
      }}
    >
      <span className="panel-divider-grip" />
    </div>
  )
}
