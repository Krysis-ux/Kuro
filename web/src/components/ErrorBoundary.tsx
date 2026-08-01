import { Component, type ErrorInfo, type ReactNode } from 'react'
import { RefreshIcon } from './icons'

/**
 * A page that throws should not take the window with it.
 *
 * Without this, one bad field read anywhere in a route unmounts everything and
 * leaves a black rectangle — no message, no way back, and nothing to tell you
 * which page was at fault. That is exactly what happened when a newer interface
 * ran against an older server: the response was missing a field, a `.length` on
 * it threw, and the whole application went blank.
 *
 * The reload button matters as much as the message. A crashed page is usually a
 * stale build on one side or the other, and reloading is the fix often enough to
 * be worth offering before anything else.
 */
interface Props {
  children: ReactNode
  /** Named so the message can say which page failed. */
  label?: string
}

interface State {
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Kept for the browser console, which is where somebody debugging this will
    // look first. The interface shows the message; the console shows the stack.
    console.error('A page failed to render', error, info.componentStack)
  }

  render() {
    const { error } = this.state
    if (!error) return this.props.children

    return (
      <div className="page">
        <header className="page-head">
          <h1>This page did not load</h1>
          <p className="muted">
            {this.props.label ? `The ${this.props.label} page ` : 'It '}
            stopped with an error rather than rendering. The rest of the application is
            unaffected — the navigation on the left still works.
          </p>
        </header>

        <section className="panel">
          <h2 className="panel-title">What went wrong</h2>
          <pre className="code-block mono">{error.message}</pre>
          <p className="faint panel-note">
            The most common cause is a mismatch between the interface and the server —
            usually one of them having been rebuilt without the other. Reloading picks up a
            newer interface; restarting Kuro picks up a newer server.
          </p>
          <div className="panel-foot">
            <button className="btn btn-solid btn-sm" onClick={() => window.location.reload()}>
              <RefreshIcon size={14} />
              Reload
            </button>
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => this.setState({ error: null })}
            >
              Try rendering it again
            </button>
          </div>
        </section>
      </div>
    )
  }
}
