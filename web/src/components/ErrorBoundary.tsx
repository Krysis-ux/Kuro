import { Component, type ErrorInfo, type ReactNode } from 'react'
import { RefreshIcon } from './icons'

interface Props {
  children: ReactNode
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
