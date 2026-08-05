import { useLocation, useNavigate } from 'react-router-dom'

/**
 * The way back to wherever you came from.
 *
 * Shown only when something navigated here deliberately and said so, by passing
 * `state: { from }`. That is the difference between a back link and a browser
 * back button: this one appears when there is a known place to return to, and
 * stays absent when you arrived by clicking the sidebar — where the sidebar
 * itself is the way back and a second control would be noise.
 *
 * It exists because following "Tools" out of a half-written message was a
 * one-way trip: the only route back was the sidebar, which lands on a different
 * conversation than the one being written.
 */
export function BackLink({ fallbackLabel = 'Back' }: { fallbackLabel?: string }) {
  const navigate = useNavigate()
  const location = useLocation()

  const from = (location.state as { from?: string } | null)?.from
  if (!from) return null

  return (
    <button className="back-link" onClick={() => navigate(from)}>
      ← {labelFor(from, fallbackLabel)}
    </button>
  )
}

/** What to call the place being returned to, from its path. */
function labelFor(path: string, fallback: string): string {
  if (path === '/' || path.startsWith('/chat')) return 'Back to chat'
  if (path.startsWith('/code')) return 'Back to the workspace'
  if (path.startsWith('/projects')) return 'Back to the project'
  return fallback
}
