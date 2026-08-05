import { useCallback, useEffect, useRef, useState } from 'react'
import { PANEL_DEFAULTS } from '../store/ui'

/**
 * The seam between two panels, draggable.
 *
 * A one-pixel border with a wider invisible grab area, which is the only way a
 * divider can be both thin enough to look like a seam and thick enough to hit.
 * The visible line thickens on hover so the target announces itself before the
 * pointer has to guess.
 *
 * ## Why the width lives outside this
 *
 * The divider reports deltas and the panel owns its width. That keeps the stored
 * width in one place — the UI store, which persists it — rather than having a
 * component that both draws a line and remembers how wide something else is.
 *
 * ## Keyboard
 *
 * Arrow keys move it, because a control that can only be operated by dragging is
 * a control some people do not have. `separator` with `aria-orientation` is the
 * role screen readers already know this shape by.
 */
interface PanelDividerProps {
  /** Current width of the panel being sized, in pixels. */
  width: number
  /** Called with the new width as the pointer moves. */
  onResize: (width: number) => void
  /**
   * Which side the panel is on.
   *
   * A divider to the right of its panel grows it when dragged right; one to the
   * left grows it when dragged left. Getting this backwards makes a panel that
   * shrinks when you pull it open, which feels broken long before anyone works
   * out why.
   */
  side: 'left' | 'right'
  label: string
}

/** How far one arrow-key press moves the divider. */
const STEP = 16

export function PanelDivider({ width, onResize, side, label }: PanelDividerProps) {
  const [dragging, setDragging] = useState(false)
  // Read inside the pointer handler, which is registered once — a stale closure
  // here would make every drag start from the width the panel had on mount.
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
    // A pointer that leaves the window mid-drag never sends `pointerup`, which
    // would leave the divider stuck to the cursor.
    window.addEventListener('pointercancel', stop)

    // While dragging, the whole page shows the resize cursor and stops selecting
    // text — otherwise a drag across the conversation highlights it.
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
      // Imported rather than repeated: these were hardcoded here and in the
      // store, so a changed default would have silently disagreed with what
      // double-click restored.
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
