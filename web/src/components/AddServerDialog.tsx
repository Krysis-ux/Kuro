import { useEffect, useMemo, useState } from 'react'
import { useMutation } from '@tanstack/react-query'
import { api, type McpRegistryEntry, type McpTransport } from '../lib/api'
import { CheckIcon, ExternalIcon, PlusIcon, TrashIcon } from './icons'

interface AddServerDialogProps {
  prefill: McpRegistryEntry | null
  registry: McpRegistryEntry[]
  onClose: () => void
  onAdded: () => void
}

export function AddServerDialog({ prefill, registry, onClose, onAdded }: AddServerDialogProps) {
  const [slug, setSlug] = useState<string | null>(prefill?.slug ?? null)
  const [transport, setTransport] = useState<McpTransport>(prefill?.transport ?? 'http')
  const [name, setName] = useState(prefill?.name ?? '')
  const [url, setUrl] = useState(prefill?.url ?? '')
  const [command, setCommand] = useState(prefill?.command ?? '')
  const [args, setArgs] = useState((prefill?.args ?? []).join(' '))
  const [useAuth, setUseAuth] = useState(prefill?.requirement === 'api_key')
  const [token, setToken] = useState('')
  const [headers, setHeaders] = useState<{ key: string; value: string }[]>([])
  const [failure, setFailure] = useState<string | null>(null)

  const selected = useMemo(
    () => (slug ? registry.find((entry) => entry.slug === slug) ?? null : null),
    [registry, slug],
  )

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [onClose])

  const choose = (entry: McpRegistryEntry) => {
    setSlug(entry.slug)
    setTransport(entry.transport)
    setName(entry.name)
    setUrl(entry.url ?? '')
    setCommand(entry.command ?? '')
    setArgs(entry.args.join(' '))
    setUseAuth(entry.requirement === 'api_key')
    setFailure(null)
  }

  const clearChoice = () => {
    setSlug(null)
    setFailure(null)
  }

  const add = useMutation({
    mutationFn: () =>
      api.mcp.add({
        ...(slug ? { slug } : { name: name.trim(), transport }),
        ...(slug ? {} : transport === 'http' ? { url: url.trim() } : { command: command.trim() }),
        args: splitArgs(args),
        headers: Object.fromEntries(
          headers
            .filter((header) => header.key.trim() && header.value.trim())
            .map((header) => [header.key.trim(), header.value.trim()]),
        ),
        ...(useAuth && token.trim() ? { authToken: token.trim() } : {}),
      }),
    onSuccess: (result) => {
      if (result.connection.ok) {
        onAdded()
        return
      }
      setFailure(result.connection.error ?? 'Connected, but the server returned no tools.')
    },
    onError: (error: Error) => setFailure(error.message),
  })

  const canSubmit = slug
    ? true
    : name.trim().length > 0 &&
      (transport === 'http' ? url.trim().length > 0 : command.trim().length > 0)

  const recommended = registry.filter((entry) => !entry.installed)

  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Add an MCP server"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="dialog-head">
          <h2>Add an MCP server</h2>
          <button className="dialog-close" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>

        <div className="dialog-body">
          {recommended.length > 0 && (
            <>
              <div className="dialog-section-head">
                <span>Recommended</span>
                {slug && (
                  <button className="link-button faint" onClick={clearChoice}>
                    Configure manually instead
                  </button>
                )}
              </div>

              <div className="recommend-grid">
                {recommended.map((entry) => (
                  <button
                    key={entry.slug}
                    className={`recommend-card ${entry.slug === slug ? 'is-on' : ''}`}
                    onClick={() => choose(entry)}
                  >
                    <div className="recommend-card-head">
                      <span>{entry.name}</span>
                      {entry.slug === slug && <CheckIcon size={13} />}
                    </div>
                    <p className="muted">{entry.blurb}</p>
                  </button>
                ))}
              </div>
            </>
          )}

          {selected ? (
            <div className="dialog-selected">
              <p className="muted">{selected.detail}</p>
              <div className="row">
                <span className="faint">Endpoint</span>
                <span className="mono">
                  {selected.url ?? [selected.command, ...selected.args].join(' ')}
                </span>
              </div>
              {selected.requirement === 'local_runtime' && (
                <p className="faint">
                  Runs as a process on this machine. It can only reach what you pass in the
                  arguments below.
                </p>
              )}
            </div>
          ) : (
            <>
              <div className="dialog-section-head">
                <span>Manual</span>
              </div>

              <div className="segmented dialog-segmented">
                {(['http', 'stdio'] as const).map((option) => (
                  <button
                    key={option}
                    className={`segment ${transport === option ? 'is-on' : ''}`}
                    onClick={() => setTransport(option)}
                  >
                    {option === 'http' ? 'Remote (HTTP)' : 'Local (stdio)'}
                  </button>
                ))}
              </div>

              <label className="dialog-field">
                <span>
                  Name <span className="required">*</span>
                </span>
                <input
                  className="input"
                  placeholder="My server"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>

              {transport === 'http' ? (
                <label className="dialog-field">
                  <span>
                    Server URL <span className="required">*</span>
                  </span>
                  <input
                    className="input mono"
                    placeholder="https://mcp.example.com/mcp"
                    value={url}
                    onChange={(event) => setUrl(event.target.value)}
                  />
                  <span className="faint dialog-hint">
                    Usually ends in <code className="mono">/mcp</code> or{' '}
                    <code className="mono">/sse</code>.
                  </span>
                </label>
              ) : (
                <label className="dialog-field">
                  <span>
                    Command <span className="required">*</span>
                  </span>
                  <input
                    className="input mono"
                    placeholder="npx"
                    value={command}
                    onChange={(event) => setCommand(event.target.value)}
                  />
                  <span className="faint dialog-hint">
                    Must be on Kuro's PATH. Started fresh for each call and stopped afterwards.
                  </span>
                </label>
              )}
            </>
          )}

          {(transport === 'stdio' || selected?.transport === 'stdio') && (
            <label className="dialog-field">
              <span>Arguments</span>
              <input
                className="input mono"
                placeholder="-y @modelcontextprotocol/server-filesystem /Users/you/project"
                value={args}
                onChange={(event) => setArgs(event.target.value)}
              />
              <span className="faint dialog-hint">
                Space separated. For the filesystem server, the folders it is allowed to read.
              </span>
            </label>
          )}

          <div className="dialog-field">
            <div className="dialog-toggle-row">
              <button
                className={`switch ${useAuth ? 'is-on' : ''}`}
                role="switch"
                aria-checked={useAuth}
                aria-label="Send an authorization header"
                onClick={() => setUseAuth((value) => !value)}
              >
                <span className="switch-knob" />
              </button>
              <span>Authorization</span>
              {selected?.credentials_url && (
                <a
                  className="external-link faint"
                  href={selected.credentials_url}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  Get a token
                  <ExternalIcon size={11} />
                </a>
              )}
            </div>

            {useAuth && (
              <>
                <input
                  className="input"
                  type="password"
                  placeholder="Bearer token"
                  value={token}
                  onChange={(event) => setToken(event.target.value)}
                />
                <span className="faint dialog-hint">
                  Kept in a separate owner-only file, never in Kuro's database.
                </span>
              </>
            )}
          </div>

          <div className="dialog-field">
            <div className="dialog-section-head">
              <span>Custom headers</span>
              <button
                className="link-button"
                onClick={() => setHeaders((held) => [...held, { key: '', value: '' }])}
              >
                <PlusIcon size={12} /> Add
              </button>
            </div>

            {headers.length === 0 ? (
              <p className="faint dialog-hint">None configured.</p>
            ) : (
              headers.map((header, index) => (
                <div key={index} className="header-row">
                  <input
                    className="input mono"
                    placeholder="X-Header"
                    value={header.key}
                    onChange={(event) =>
                      setHeaders((held) =>
                        held.map((entry, position) =>
                          position === index ? { ...entry, key: event.target.value } : entry,
                        ),
                      )
                    }
                  />
                  <input
                    className="input mono"
                    placeholder="value"
                    value={header.value}
                    onChange={(event) =>
                      setHeaders((held) =>
                        held.map((entry, position) =>
                          position === index ? { ...entry, value: event.target.value } : entry,
                        ),
                      )
                    }
                  />
                  <button
                    className="btn btn-ghost btn-icon"
                    aria-label="Remove header"
                    onClick={() =>
                      setHeaders((held) => held.filter((_, position) => position !== index))
                    }
                  >
                    <TrashIcon size={14} />
                  </button>
                </div>
              ))
            )}
          </div>

          {failure && (
            <div className="dialog-failure">
              <strong>Could not connect.</strong> {failure}
              <p className="faint">
                The server was saved. Fix the details above and press Add again, or close and
                reconnect from the list.
              </p>
            </div>
          )}
        </div>

        <div className="dialog-foot">
          <button className="btn btn-ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn-solid"
            disabled={!canSubmit || add.isPending}
            onClick={() => {
              setFailure(null)
              add.mutate()
            }}
          >
            {add.isPending ? <span className="spinner" /> : null}
            {add.isPending ? 'Connecting…' : 'Add'}
          </button>
        </div>
      </div>
    </div>
  )
}

function splitArgs(raw: string): string[] {
  const out: string[] = []
  let current = ''
  let quote: '"' | "'" | null = null

  for (const character of raw.trim()) {
    if (quote) {
      if (character === quote) quote = null
      else current += character
      continue
    }
    if (character === '"' || character === "'") {
      quote = character
      continue
    }
    if (character === ' ') {
      if (current) out.push(current)
      current = ''
      continue
    }
    current += character
  }

  if (current) out.push(current)
  return out
}
