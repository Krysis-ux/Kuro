import { PlugIcon } from '../components/icons'

/**
 * MCP server management.
 *
 * The connection layer lands in the next phase. This page states that plainly
 * rather than offering a form that would save servers Kuro cannot yet reach.
 */
export function McpServersPage() {
  return (
    <div className="page">
      <header className="page-head">
        <h1>MCP servers</h1>
        <p className="muted">
          Give models access to tools — files, GitHub, search, your own scripts — through the Model
          Context Protocol.
        </p>
      </header>

      <section className="panel empty-state">
        <PlugIcon size={22} className="empty-mark" />
        <h2 className="panel-title">Not connected yet</h2>
        <p className="muted empty-copy">
          Tool support is the next thing being built. When it lands you will be able to add stdio
          and HTTP servers here, and any tools they expose become available in chat through the
          composer's <strong>+</strong> menu.
        </p>
        <div className="empty-list">
          <span className="tag">stdio</span>
          <span className="tag">HTTP / SSE</span>
          <span className="tag">GitHub</span>
          <span className="tag">Filesystem</span>
          <span className="tag">Web search</span>
        </div>
      </section>
    </div>
  )
}
