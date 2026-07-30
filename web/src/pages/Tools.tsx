import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, type McpRegistryEntry, type McpServer, type SearchResult } from '../lib/api'
import { AddServerDialog } from '../components/AddServerDialog'
import {
  BrainIcon,
  CheckIcon,
  ExternalIcon,
  GlobeIcon,
  KeyIcon,
  PlugIcon,
  PlusIcon,
  RefreshIcon,
  SparkIcon,
  StoreIcon,
  ToolIcon,
  TrashIcon,
} from '../components/icons'

/**
 * Tools.
 *
 * Kuro's own capabilities and other people's MCP servers on one page, because the
 * distinction is an implementation detail — from the model's side they are the
 * same thing, and a user asking "can it search the web" should not have to know
 * which half answers that.
 */
export function ToolsPage() {
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [prefill, setPrefill] = useState<McpRegistryEntry | null>(null)

  const overview = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })
  const servers = useQuery({
    queryKey: ['mcp', 'servers'],
    queryFn: () => api.mcp.servers(true),
  })
  const registry = useQuery({ queryKey: ['mcp', 'registry'], queryFn: api.mcp.registry })

  const refreshAll = () => {
    void queryClient.invalidateQueries({ queryKey: ['mcp'] })
    void queryClient.invalidateQueries({ queryKey: ['tools'] })
  }

  const openDialog = (entry: McpRegistryEntry | null) => {
    setPrefill(entry)
    setDialogOpen(true)
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>Tools</h1>
        <p className="muted">
          What a model can do besides produce text. Search and memory are built in; anything else
          comes from a Model Context Protocol server.
        </p>
      </header>

      <BuiltinSection overview={overview.data} onChanged={refreshAll} />

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <PlugIcon size={15} />
            MCP servers
          </h2>
          <div className="panel-actions">
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => void servers.refetch()}
              disabled={servers.isFetching}
            >
              {servers.isFetching ? <span className="spinner" /> : <RefreshIcon size={14} />}
              Reconnect
            </button>
            <button className="btn btn-solid btn-sm" onClick={() => openDialog(null)}>
              <PlusIcon size={14} />
              Add server
            </button>
          </div>
        </div>

        {servers.data?.servers.length === 0 ? (
          <p className="faint panel-note">
            None connected. Add one below, or pick from the recommended list.
          </p>
        ) : (
          <div className="server-rows">
            {servers.data?.servers.map((server) => (
              <ServerRow key={server.id} server={server} onChanged={refreshAll} />
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <h2 className="panel-title">
          <StoreIcon size={15} />
          Recommended
        </h2>
        <p className="faint panel-note">
          A short list rather than a directory. Everything here is either keyless or run by the
          people who own the thing it connects to.
        </p>

        <div className="store-grid">
          {registry.data?.entries.map((entry) => (
            <StoreCard key={entry.slug} entry={entry} onInstall={() => openDialog(entry)} />
          ))}
        </div>
      </section>

      {dialogOpen && (
        <AddServerDialog
          prefill={prefill}
          registry={registry.data?.entries ?? []}
          onClose={() => setDialogOpen(false)}
          onAdded={() => {
            setDialogOpen(false)
            refreshAll()
          }}
        />
      )}
    </div>
  )
}

/* ---------- Built-in tools ---------- */

function BuiltinSection({
  overview,
  onChanged,
}: {
  overview: Awaited<ReturnType<typeof api.tools.overview>> | undefined
  onChanged: () => void
}) {
  const queryClient = useQueryClient()
  const [testResult, setTestResult] = useState<{
    ok: boolean
    results?: SearchResult[]
    error?: string
  } | null>(null)

  const configure = useMutation({
    mutationFn: (patch: { provider?: string; baseUrl?: string; apiKey?: string }) =>
      api.tools.configureSearch(patch),
    onSuccess: (data) => {
      queryClient.setQueryData(['tools'], data)
      setTestResult(null)
      onChanged()
    },
  })

  const setDefaults = useMutation({
    mutationFn: (patch: { memoryPreload?: boolean }) => api.tools.setDefaults(patch),
    onSuccess: (data) => queryClient.setQueryData(['tools'], data),
  })

  const test = useMutation({
    mutationFn: () => api.tools.testSearch(),
    onSuccess: (data) => setTestResult(data),
  })

  const [keyDraft, setKeyDraft] = useState('')
  const [urlDraft, setUrlDraft] = useState('')

  if (!overview) {
    return (
      <section className="panel">
        <h2 className="panel-title">Built in</h2>
        <p className="faint">Loading…</p>
      </section>
    )
  }

  const { search } = overview
  const selected = search.providers.find((provider) => provider.id === search.provider)

  return (
    <>
      <section className="panel">
        <h2 className="panel-title">
          <GlobeIcon size={15} />
          Web search
        </h2>
        <p className="faint panel-note">
          Turn it on per message with the <strong>Web</strong> switch in the composer. Kuro searches
          before the model answers, so it works on any model — including ones too small to ask for a
          tool themselves.
        </p>

        <div className="provider-choices">
          {search.providers.map((provider) => (
            <button
              key={provider.id}
              className={`provider-choice ${provider.id === search.provider ? 'is-on' : ''}`}
              onClick={() => configure.mutate({ provider: provider.id })}
              disabled={configure.isPending}
            >
              <div className="provider-choice-head">
                <span>{provider.name}</span>
                {provider.id === search.provider && <CheckIcon size={13} />}
                {!provider.needsApiKey && !provider.needsBaseUrl && (
                  <span className="tag">no key</span>
                )}
              </div>
              <p className="muted">{provider.note}</p>
            </button>
          ))}
        </div>

        {search.needsApiKey && (
          <Field
            label="API key"
            hint={
              search.hasApiKey
                ? 'A key is stored. Enter a new one to replace it.'
                : `${selected?.name ?? 'This provider'} needs a key before search will work.`
            }
          >
            <div className="inline-form">
              <input
                className="input"
                type="password"
                placeholder={search.hasApiKey ? '••••••••••••' : 'Paste the key'}
                value={keyDraft}
                onChange={(event) => setKeyDraft(event.target.value)}
              />
              <button
                className="btn btn-solid btn-sm"
                disabled={!keyDraft.trim() || configure.isPending}
                onClick={() => {
                  configure.mutate({ apiKey: keyDraft })
                  setKeyDraft('')
                }}
              >
                Save
              </button>
              {search.hasApiKey && (
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={() => configure.mutate({ apiKey: '' })}
                >
                  Remove
                </button>
              )}
            </div>
          </Field>
        )}

        {search.needsBaseUrl && (
          <Field label="Instance URL" hint="Where your SearXNG instance is reachable.">
            <div className="inline-form">
              <input
                className="input"
                placeholder={search.baseUrl ?? 'http://localhost:8888'}
                value={urlDraft}
                onChange={(event) => setUrlDraft(event.target.value)}
              />
              <button
                className="btn btn-solid btn-sm"
                disabled={!urlDraft.trim() || configure.isPending}
                onClick={() => {
                  configure.mutate({ baseUrl: urlDraft })
                  setUrlDraft('')
                }}
              >
                Save
              </button>
            </div>
          </Field>
        )}

        {selected?.credentialsUrl && (
          <a
            className="external-link faint"
            href={selected.credentialsUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            {search.needsApiKey ? 'Get a key' : 'How to run one'}
            <ExternalIcon size={11} />
          </a>
        )}

        <div className="panel-foot">
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => test.mutate()}
            disabled={test.isPending}
          >
            {test.isPending ? <span className="spinner" /> : <GlobeIcon size={14} />}
            Test search
          </button>
          <span className="faint">Runs a real query, so you never have to guess.</span>
        </div>

        {testResult && (
          <div className={`test-result ${testResult.ok ? 'is-ok' : 'is-error'}`}>
            {testResult.ok ? (
              <>
                <strong>Working.</strong>
                <ul className="test-hits">
                  {testResult.results?.map((result) => (
                    <li key={result.url}>
                      <span>{result.title}</span>
                      <span className="faint mono">{result.url}</span>
                    </li>
                  ))}
                </ul>
              </>
            ) : (
              <>
                <strong>Not working.</strong> {testResult.error}
              </>
            )}
          </div>
        )}
      </section>

      <SkillsSection overview={overview} />

      <section className="panel">
        <h2 className="panel-title">
          <BrainIcon size={15} />
          Memory
        </h2>
        <p className="faint panel-note">
          Durable facts the model has been asked to keep. Stored on this machine, available in every
          conversation, and never sent anywhere.
        </p>

        <Toggle
          label="Put memories in front of the model automatically"
          hint="Without this, memory only works when the model thinks to look — which small models often do not."
          checked={overview.memory.preload}
          onChange={(preload) => setDefaults.mutate({ memoryPreload: preload })}
        />

        <div className="row">
          <span className="faint">Stored</span>
          <span>
            {overview.memory.count} {overview.memory.count === 1 ? 'memory' : 'memories'}
          </span>
        </div>

        <MemoryList />
      </section>

      <section className="panel">
        <h2 className="panel-title">
          <ToolIcon size={15} />
          What the model sees
        </h2>
        <div className="tool-list">
          {overview.builtins.map((tool) => (
            <div key={tool.name} className="tool-list-row">
              <code className="mono">{tool.name}</code>
              <span className="tag">{tool.group}</span>
              <p className="muted">{tool.description}</p>
            </div>
          ))}
        </div>
      </section>
    </>
  )
}

/**
 * The skills store.
 *
 * A skill is prompt guidance, nothing more — no execution, no sandbox. That is
 * what makes one-click installation safe, and it is also the highest-leverage
 * thing available on a small local model: a 4B model told the specific rules of
 * idiomatic Rust writes markedly better Rust for no extra inference cost.
 *
 * The context cost is shown because it is real. Six skills at once is most of a
 * small model's usable prompt, and a user should be able to see that adding up
 * rather than discover it as a mysterious drop in answer quality.
 */
function SkillsSection({
  overview,
}: {
  overview: Awaited<ReturnType<typeof api.tools.overview>> | undefined
}) {
  const queryClient = useQueryClient()
  const [expanded, setExpanded] = useState<string | null>(null)

  const save = useMutation({
    mutationFn: (enabled: string[]) => api.tools.setSkills(enabled),
    onSuccess: (data) => queryClient.setQueryData(['tools'], data),
  })

  if (!overview) return null

  const { catalogue, enabled, approxTokens } = overview.skills

  const toggle = (slug: string) => {
    const next = enabled.includes(slug)
      ? enabled.filter((held) => held !== slug)
      : [...enabled, slug]
    save.mutate(next)
  }

  const byCategory = new Map<string, typeof catalogue>()
  for (const skill of catalogue) {
    byCategory.set(skill.category, [...(byCategory.get(skill.category) ?? []), skill])
  }

  const CATEGORY_LABEL: Record<string, string> = {
    language: 'Languages',
    practice: 'Engineering practice',
    writing: 'Writing and reasoning',
  }

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">
          <SparkIcon size={15} />
          Skills
        </h2>
        {enabled.length > 0 && (
          <span className="faint">
            {enabled.length} on · about {approxTokens} tokens of context
          </span>
        )}
      </div>

      <p className="faint panel-note">
        Expertise you switch on. Each one adds specific instructions to the model's brief — the
        same lever a hosted assistant uses, and the one that helps a small local model most.
      </p>

      {[...byCategory].map(([category, skills]) => (
        <div key={category} className="skill-group">
          <div className="skill-group-head">{CATEGORY_LABEL[category] ?? category}</div>
          <div className="skill-grid">
            {skills.map((skill) => (
              <div
                key={skill.slug}
                className={`skill-card ${enabled.includes(skill.slug) ? 'is-on' : ''}`}
              >
                <div className="skill-card-head">
                  <span className="skill-card-name">{skill.name}</span>
                  <Switch
                    checked={enabled.includes(skill.slug)}
                    label={`${enabled.includes(skill.slug) ? 'Turn off' : 'Turn on'} ${skill.name}`}
                    onChange={() => toggle(skill.slug)}
                  />
                </div>
                <p className="muted skill-card-blurb">{skill.blurb}</p>
                <button
                  className="link-button faint"
                  onClick={() => setExpanded(expanded === skill.slug ? null : skill.slug)}
                >
                  {expanded === skill.slug ? 'Hide instructions' : 'What it tells the model'}
                </button>
                {expanded === skill.slug && (
                  <pre className="skill-instructions">{skill.instructions}</pre>
                )}
              </div>
            ))}
          </div>
        </div>
      ))}
    </section>
  )
}

function MemoryList() {
  const queryClient = useQueryClient()
  const [expanded, setExpanded] = useState(false)

  const memories = useQuery({
    queryKey: ['memories'],
    queryFn: () => api.memories.list(),
    enabled: expanded,
  })

  const forget = useMutation({
    mutationFn: (id: string) => api.memories.remove(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['memories'] })
      void queryClient.invalidateQueries({ queryKey: ['tools'] })
    },
  })

  if (!expanded) {
    return (
      <button className="btn btn-ghost btn-sm" onClick={() => setExpanded(true)}>
        Review what is remembered
      </button>
    )
  }

  return (
    <div className="memory-list">
      {memories.data?.memories.length === 0 && (
        <p className="faint">Nothing yet. Ask the model to remember something.</p>
      )}
      {memories.data?.memories.map((memory) => (
        <div key={memory.id} className="memory-row">
          <span>{memory.content}</span>
          <div className="memory-row-tags">
            {memory.tags.map((tag) => (
              <span key={tag} className="tag">
                {tag}
              </span>
            ))}
          </div>
          <button
            className="btn btn-ghost btn-icon"
            aria-label="Forget this"
            onClick={() => forget.mutate(memory.id)}
          >
            <TrashIcon size={14} />
          </button>
        </div>
      ))}
    </div>
  )
}

/* ---------- MCP servers ---------- */

function ServerRow({ server, onChanged }: { server: McpServer; onChanged: () => void }) {
  const [showTools, setShowTools] = useState(false)
  const [tokenDraft, setTokenDraft] = useState('')
  const [showToken, setShowToken] = useState(false)

  const setEnabled = useMutation({
    mutationFn: (enabled: boolean) => api.mcp.setEnabled(server.id, enabled),
    onSuccess: onChanged,
  })

  const refresh = useMutation({
    mutationFn: () => api.mcp.refresh(server.id),
    onSuccess: onChanged,
  })

  const setAuth = useMutation({
    mutationFn: (token: string) => api.mcp.setAuth(server.id, token),
    onSuccess: () => {
      setTokenDraft('')
      setShowToken(false)
      onChanged()
    },
  })

  const remove = useMutation({
    mutationFn: () => api.mcp.remove(server.id),
    onSuccess: onChanged,
  })

  const toolCount = server.tool_count ?? server.tools.length

  return (
    <div className={`server-row ${server.enabled ? '' : 'is-off'}`}>
      <div className="server-row-main">
        <div className="server-row-head">
          <span className={`status-dot status-${server.status}`} />
          <span className="server-row-name">{server.name}</span>
          <span className="tag">{server.transport}</span>
          {toolCount > 0 && (
            <button className="tag tag-button" onClick={() => setShowTools((open) => !open)}>
              {toolCount} {toolCount === 1 ? 'tool' : 'tools'}
            </button>
          )}
          {server.has_auth && (
            <span className="tag" title="A token is stored outside the database">
              <KeyIcon size={10} /> key
            </span>
          )}
        </div>

        <span className="faint mono server-row-address">
          {server.url ?? [server.command, ...server.args].filter(Boolean).join(' ')}
        </span>

        {server.status === 'error' && server.last_error && (
          <p className="form-error">{server.last_error}</p>
        )}

        {showTools && (
          <div className="tool-list">
            {server.tools.map((tool) => (
              <div key={tool.name} className="tool-list-row">
                <code className="mono">{tool.name}</code>
                <p className="muted">{tool.description}</p>
              </div>
            ))}
          </div>
        )}

        {showToken && (
          <div className="inline-form">
            <input
              className="input"
              type="password"
              placeholder="Bearer token"
              value={tokenDraft}
              onChange={(event) => setTokenDraft(event.target.value)}
            />
            <button
              className="btn btn-solid btn-sm"
              disabled={!tokenDraft.trim() || setAuth.isPending}
              onClick={() => setAuth.mutate(tokenDraft)}
            >
              Save and reconnect
            </button>
            <button className="btn btn-ghost btn-sm" onClick={() => setShowToken(false)}>
              Cancel
            </button>
          </div>
        )}
      </div>

      <div className="server-row-actions">
        <button
          className="btn btn-ghost btn-icon"
          aria-label={`Reconnect ${server.name}`}
          title="Reconnect"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
        >
          {refresh.isPending ? <span className="spinner" /> : <RefreshIcon size={14} />}
        </button>

        <button
          className="btn btn-ghost btn-icon"
          aria-label="Set token"
          title={server.has_auth ? 'Replace the token' : 'Add a token'}
          onClick={() => setShowToken((open) => !open)}
        >
          <KeyIcon size={14} />
        </button>

        <Switch
          checked={server.enabled}
          label={`${server.enabled ? 'Disable' : 'Enable'} ${server.name}`}
          onChange={(enabled) => setEnabled.mutate(enabled)}
        />

        <button
          className="btn btn-ghost btn-icon"
          aria-label={`Remove ${server.name}`}
          onClick={() => remove.mutate()}
        >
          <TrashIcon size={14} />
        </button>
      </div>
    </div>
  )
}

function StoreCard({
  entry,
  onInstall,
}: {
  entry: McpRegistryEntry
  onInstall: () => void
}) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className={`store-card ${entry.installed ? 'is-installed' : ''}`}>
      <div className="store-card-head">
        <span className="store-card-name">{entry.name}</span>
        {entry.requirement === 'none' && <span className="tag">works now</span>}
        {entry.requirement === 'api_key' && <span className="tag tag-warn">needs a key</span>}
        {entry.requirement === 'local_runtime' && <span className="tag">runs locally</span>}
      </div>

      <p className="muted store-card-blurb">{expanded ? entry.detail : entry.blurb}</p>

      <button className="link-button faint" onClick={() => setExpanded((open) => !open)}>
        {expanded ? 'Less' : 'More'}
      </button>

      <div className="store-card-foot">
        <button
          className="btn btn-sm"
          disabled={entry.installed}
          onClick={onInstall}
        >
          {entry.installed ? (
            <>
              <CheckIcon size={13} /> Installed
            </>
          ) : (
            <>
              <PlusIcon size={13} /> Add
            </>
          )}
        </button>
        <a
          className="external-link faint"
          href={entry.homepage}
          target="_blank"
          rel="noopener noreferrer"
        >
          Docs
          <ExternalIcon size={11} />
        </a>
      </div>
    </div>
  )
}

/* ---------- Small shared controls ---------- */

function Field({
  label,
  hint,
  children,
}: {
  label: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <div className="field field-stacked">
      <div className="field-label">
        <span>{label}</span>
        {hint && <span className="faint field-hint">{hint}</span>}
      </div>
      <div className="field-control">{children}</div>
    </div>
  )
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string
  hint?: string
  checked: boolean
  onChange: (value: boolean) => void
}) {
  return (
    <div className="field">
      <div className="field-label">
        <span>{label}</span>
        {hint && <span className="faint field-hint">{hint}</span>}
      </div>
      <Switch checked={checked} label={label} onChange={onChange} />
    </div>
  )
}

export function Switch({
  checked,
  label,
  onChange,
}: {
  checked: boolean
  label: string
  onChange: (value: boolean) => void
}) {
  return (
    <button
      className={`switch ${checked ? 'is-on' : ''}`}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-knob" />
    </button>
  )
}
