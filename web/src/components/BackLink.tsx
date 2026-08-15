import { useLocation, useNavigate } from 'react-router-dom'

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

function labelFor(path: string, fallback: string): string {
  if (path === '/' || path.startsWith('/chat')) return 'Back to chat'
  if (path.startsWith('/code')) return 'Back to the workspace'
  if (path.startsWith('/projects')) return 'Back to the project'
  return fallback
}
