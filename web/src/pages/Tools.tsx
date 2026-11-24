import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  api,
  type CustomSkill,
  type McpRegistryEntry,
  type McpServer,
  type SearchResult,
  type SkillImport,
  type ToolsOverview,
} from '../lib/api'
import { AddServerDialog } from '../components/AddServerDialog'
import { ProviderLogo } from '../components/BrandLogo'
import { HelpTip, TitleWithHelp } from '../components/HelpTip'
import {
  CheckIcon,
  ChevronIcon,
  GitHubIcon,
  ExternalIcon,
  GlobeIcon,
  KeyIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  TrashIcon,
} from '../components/icons'

/*
 * The Agent panes of Settings: MCP servers, web search, built-in tools, skills
 * and the memory list. Kuro's own capabilities and other people's MCP servers
 * are the same thing from the model's side.
 */

/* ---------- MCP servers ---------- */

export function McpPane() {
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [prefill, setPrefill] = useState<McpRegistryEntry | null>(null)
  const [storeQuery, setStoreQuery] = useState('')

  const servers = useQuery({
    queryKey: ['mcp', 'servers'],
    queryFn: () => api.mcp.servers(true),
  })
  const registry = useQuery({ queryKey: ['mcp', 'registry'], queryFn: api.mcp.registry })

  const matches = filterRegistry(registry.data?.entries ?? [], storeQuery)

  const refreshAll = () => {
    void queryClient.invalidateQueries({ queryKey: ['mcp'] })
    void queryClient.invalidateQueries({ queryKey: ['tools'] })
  }

  const openDialog = (entry: McpRegistryEntry | null) => {
    setPrefill(entry)
    setDialogOpen(true)
  }

  return (
    <>
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Connected</h2>
          <div className="panel-actions">
            <button
              className="btn"
              onClick={() => void servers.refetch()}
              disabled={servers.isFetching}
            >
              {servers.isFetching ? <span className="spinner" /> : <RefreshIcon size={14} />}
              Reconnect
            </button>
            <button className="btn btn-solid" onClick={() => openDialog(null)}>
              <PlusIcon size={14} />
              Add server
            </button>
          </div>
        </div>

        {servers.isError && <p className="form-error">Kuro did not answer.</p>}
        {servers.data?.servers.length === 0 ? (
          <span className="faint">None yet</span>
        ) : (
          <div className="server-rows">
            {servers.data?.servers.map((server) => (
              <ServerRow key={server.id} server={server} onChanged={refreshAll} />
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <TitleWithHelp
              title="Marketplace"
              help="A curated list, not a directory: each is keyless or run by whoever owns what it connects to. Any MCP server works through Add server."
            />
          </h2>
          <div className="store-search">
            <SearchIcon size={13} className="search-icon" />
            <input
              className="input"
              placeholder="Search servers"
              value={storeQuery}
              onChange={(event) => setStoreQuery(event.target.value)}
            />
          </div>
        </div>

        {registry.isError && <p className="form-error">The list could not be read.</p>}
        {matches.length === 0 && !registry.isLoading && (
          <span className="faint">{storeQuery.trim() ? 'Nothing matched' : 'Nothing to show'}</span>
        )}

        <div className="store-grid">
          {matches.map((entry) => (
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
    </>
  )
}

/** Name, blurb and detail all count: someone after a browser types "browser", and the entry is Playwright. */
function filterRegistry(entries: McpRegistryEntry[], query: string): McpRegistryEntry[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return entries

  return entries.filter((entry) =>
    [entry.name, entry.blurb, entry.detail, entry.slug].some((field) =>
      field.toLowerCase().includes(needle),
    ),
  )
}

/* ---------- Web search ---------- */

export function SearchPane({ overview }: { overview: ToolsOverview }) {
  const queryClient = useQueryClient()
  const [testResult, setTestResult] = useState<{
    ok: boolean
    results?: SearchResult[]
    error?: string
  } | null>(null)
  const [keyDraft, setKeyDraft] = useState('')
  const [urlDraft, setUrlDraft] = useState('')

  const configure = useMutation({
    mutationFn: (patch: { provider?: string; baseUrl?: string; apiKey?: string }) =>
      api.tools.configureSearch(patch),
    onSuccess: (data) => {
      queryClient.setQueryData(['tools'], data)
      setTestResult(null)
    },
  })

  const test = useMutation({
    mutationFn: () => api.tools.testSearch(),
    onSuccess: (data) => setTestResult(data),
  })

  const { search } = overview
  const selected = search.providers.find((provider) => provider.id === search.provider)

  return (
    <>
      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <TitleWithHelp
              title="Search With"
              help="Turn it on per message with the Web switch in the composer. Kuro searches before the model answers, so it works on any model."
            />
          </h2>
          <button className="btn" onClick={() => test.mutate()} disabled={test.isPending}>
            {test.isPending ? <span className="spinner" /> : <GlobeIcon size={14} />}
            Test
          </button>
        </div>

        <div className="provider-choices">
          {search.providers.map((provider) => (
            <button
              key={provider.id}
              className={`provider-choice ${provider.id === search.provider ? 'is-on' : ''}`}
              onClick={() => configure.mutate({ provider: provider.id })}
              disabled={configure.isPending}
              title={provider.note}
            >
              <div className="provider-choice-head">
                <ProviderLogo slug={provider.id} label={provider.name} size={22} />
                <span>{provider.name}</span>
                {provider.id === search.provider && <CheckIcon size={13} />}
                {!provider.needsApiKey && !provider.needsBaseUrl && (
                  <span className="tag">no key</span>
                )}
              </div>
            </button>
          ))}
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

      {(search.needsApiKey || search.needsBaseUrl) && (
        <section className="panel">
          {search.needsApiKey && (
            <Field label="API key" hint={search.hasApiKey ? 'A key is stored. A new one replaces it.' : undefined}>
              <div className="inline-form">
                <input
                  className="input"
                  type="password"
                  placeholder={search.hasApiKey ? '••••••••••••' : 'Paste the key'}
                  value={keyDraft}
                  onChange={(event) => setKeyDraft(event.target.value)}
                />
                <button
                  className="btn btn-solid"
                  disabled={!keyDraft.trim() || configure.isPending}
                  onClick={() => {
                    configure.mutate({ apiKey: keyDraft })
                    setKeyDraft('')
                  }}
                >
                  Save
                </button>
                {search.hasApiKey && (
                  <button className="btn" onClick={() => configure.mutate({ apiKey: '' })}>
                    Remove
                  </button>
                )}
              </div>
            </Field>
          )}

          {search.needsBaseUrl && (
            <Field label="Instance URL">
              <div className="inline-form">
                <input
                  className="input"
                  placeholder={search.baseUrl ?? 'http://localhost:8888'}
                  value={urlDraft}
                  onChange={(event) => setUrlDraft(event.target.value)}
                />
                <button
                  className="btn btn-solid"
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
        </section>
      )}
    </>
  )
}

/* ---------- Built-in tools ---------- */

export function BuiltinPane({ overview }: { overview: ToolsOverview }) {
  return (
    <section className="panel">
      <div className="tool-list">
        {overview.builtins.map((tool) => (
          <div key={tool.name} className="tool-list-row" title={tool.description}>
            <code className="mono">{tool.name}</code>
            <span className="tag">{tool.group}</span>
            <span className="muted tool-list-desc">{tool.description}</span>
          </div>
        ))}
      </div>
    </section>
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
/**
 * Adding skills of your own.
 *
 * Two ways in, because there are two things people have: a file they wrote, and
 * a repository somebody else wrote. Both land in the same place and become
 * ordinary skills — the same `/slug`, the same switch, the same budget — which
 * is the point. A skill you added should not be a second-class thing living in
 * its own corner of the interface.
 */
function AddSkills({ custom }: { custom: CustomSkill[] }) {
  const queryClient = useQueryClient()
  const [url, setUrl] = useState('')
  const [note, setNote] = useState<SkillImport | null>(null)
  const [failed, setFailed] = useState<string | null>(null)
  const [open, setOpen] = useState(false)
  const fileRef = useRef<HTMLInputElement>(null)

  const settle = (data: ToolsOverview) => {
    queryClient.setQueryData(['tools'], data)
    setFailed(null)
  }
  const blame = (caught: unknown) =>
    setFailed(caught instanceof Error ? caught.message : 'That could not be added.')

  const upload = useMutation({
    mutationFn: ({ name, text }: { name: string; text: string }) =>
      api.tools.uploadSkill(name, text),
    onSuccess: (data) => {
      settle(data)
      setNote(null)
      setOpen(true)
    },
    onError: blame,
  })

  const importRepo = useMutation({
    mutationFn: (repo: string) => api.tools.importSkills(repo),
    onSuccess: (data) => {
      settle(data)
      setNote(data.imported)
      setUrl('')
      setOpen(true)
    },
    onError: blame,
  })

  const remove = useMutation({
    mutationFn: (slug: string) => api.tools.deleteSkill(slug),
    onSuccess: settle,
    onError: blame,
  })

  const busy = upload.isPending || importRepo.isPending

  return (
    <div className="skill-add">
      <div className="skill-add-row">
        <input
          className="input mono"
          placeholder="github.com/anthropics/skills"
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && url.trim()) importRepo.mutate(url.trim())
          }}
          disabled={busy}
        />
        <button
          className="btn btn-solid"
          onClick={() => url.trim() && importRepo.mutate(url.trim())}
          disabled={!url.trim() || busy}
          title="Read every SKILL.md in this repository"
        >
          <GitHubIcon size={13} />
          {importRepo.isPending ? 'Reading…' : 'Import'}
        </button>
        <button
          className="btn"
          onClick={() => fileRef.current?.click()}
          disabled={busy}
          title="Add a SKILL.md from this computer"
        >
          <PlusIcon size={13} />
          {upload.isPending ? 'Adding…' : 'Upload'}
        </button>
        <HelpTip text="A repository is read, not cloned: Kuro takes the instructions from each SKILL.md. Scripts a skill ships with are not imported." />
      </div>

      {failed && <p className="form-error">{failed}</p>}

      {note && (
        <div className="skill-import-note">
          <strong>{note.repo}</strong>
          <span className="faint">
            {note.added.length} added
            {note.skipped.length > 0 &&
              ` · ${note.skipped.join(', ')} already built in`}
          </span>
          {note.needsScripts.length > 0 && (
            <span className="faint">
              Need scripts that were not imported:{' '}
              <span className="mono">{note.needsScripts.join(', ')}</span>
            </span>
          )}
        </div>
      )}

      {custom.length > 0 && (
        <div className="skill-mine">
          <button className="skill-group-head skill-mine-head" onClick={() => setOpen(!open)}>
            <ChevronIcon size={12} className={open ? 'is-open' : ''} />
            {custom.length} skill{custom.length === 1 ? '' : 's'} you added
          </button>

          {open && (
            <ul className="skill-mine-list">
              {custom.map((skill) => (
                <li key={skill.slug}>
                  <div className="skill-mine-main">
                    <span className="skill-mine-name">{skill.name}</span>
                    <span className="faint mono skill-mine-slug">/{skill.slug}</span>
                  </div>
                  <span className="faint skill-mine-source">
                    {skill.source === 'upload' ? 'uploaded' : shortSource(skill.source)}
                  </span>
                  <button
                    className="btn btn-ghost btn-icon"
                    onClick={() => remove.mutate(skill.slug)}
                    disabled={remove.isPending}
                    aria-label={`Remove ${skill.name}`}
                    title="Remove this skill"
                  >
                    <TrashIcon size={13} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <input
        ref={fileRef}
        type="file"
        accept=".md,.markdown,text/markdown"
        hidden
        onChange={(event) => {
          const file = event.target.files?.[0]
          event.target.value = ''
          if (!file) return
          void file.text().then((text) => upload.mutate({ name: file.name, text }))
        }}
      />
    </div>
  )
}

/** `https://github.com/owner/name` reads better as `owner/name`. */
function shortSource(source: string): string {
  return source.replace(/^https?:\/\/github\.com\//, '')
}

export function SkillsPane({ overview }: { overview: ToolsOverview }) {
  const queryClient = useQueryClient()
  const [expanded, setExpanded] = useState<string | null>(null)

  const save = useMutation({
    mutationFn: (enabled: string[]) => api.tools.setSkills(enabled),
    onSuccess: (data) => queryClient.setQueryData(['tools'], data),
  })

  const { catalogue, enabled, budgets } = overview.skills

  // What a coding turn at the default effort will actually carry. The number
  // worth showing, because it is the one that does not move when a switch is
  // flipped — see the note on the heading below.
  const perTurn =
    budgets?.code.find((entry) => entry.effort === 'balanced')?.tokens ?? null

  const toggle = (slug: string) => {
    const next = enabled.includes(slug)
      ? enabled.filter((held) => held !== slug)
      : [...enabled, slug]
    save.mutate(next)
  }

  /**
   * Turn a whole group on or off at once.
   *
   * Forty-four switches is a lot of clicking to answer "give me everything for
   * writing code". Doing exactly that is now the default, and it is no longer
   * something the user needs protecting from: a turn ranks what is switched on
   * against what was asked and sends what fits its budget, so a skill that is
   * on but irrelevant costs nothing.
   */
  const setMany = (slugs: string[], on: boolean) => {
    const next = on
      ? [...new Set([...enabled, ...slugs])]
      : enabled.filter((held) => !slugs.includes(held))
    save.mutate(next)
  }

  const byCategory = new Map<string, typeof catalogue>()
  for (const skill of catalogue) {
    byCategory.set(skill.category, [...(byCategory.get(skill.category) ?? []), skill])
  }

  // Falls back to the raw slug for a category from a newer build, so an unknown
  // one is an odd heading rather than a section that disappears.
  const CATEGORY_LABEL: Record<string, string> = {
    language: 'Languages',
    practice: 'Engineering practice',
    design: 'Interface and design',
    writing: 'Writing and reasoning',
  }

  return (
    <>
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">Add Your Own</h2>
      </div>
      <AddSkills custom={overview.skills.custom ?? []} />
    </section>

    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">Built In</h2>
        <div className="panel-head-actions">
          {enabled.length > 0 && (
            // Deliberately no longer the sum of everything switched on. That
            // number ran to tens of thousands with the whole catalogue enabled
            // and read as the running cost of leaving switches on — so people
            // switched skills off to protect a context window that was never
            // actually at risk. What a turn spends is capped regardless of how
            // many are on, and the cap is the honest figure.
            <span
              className="faint"
              title="Switching a skill on makes it available. Each turn picks the ones your message calls for, up to the budget."
            >
              {enabled.length} on
              {perTurn !== null && ` · up to ~${perTurn} tokens per turn`}
            </span>
          )}
          <button
            className="btn"
            onClick={() =>
              setMany(
                catalogue.map((skill) => skill.slug),
                enabled.length < catalogue.length,
              )
            }
          >
            {enabled.length < catalogue.length ? 'Enable all' : 'Turn all off'}
          </button>
        </div>
      </div>

      {[...byCategory].map(([category, skills]) => (
        <div key={category} className="skill-group">
          <div className="skill-group-head">
            <span>{CATEGORY_LABEL[category] ?? category}</span>
            <button
              className="link-button faint"
              onClick={() => {
                const slugs = skills.map((skill) => skill.slug)
                setMany(slugs, !slugs.every((slug) => enabled.includes(slug)))
              }}
            >
              {skills.every((skill) => enabled.includes(skill.slug)) ? 'none' : 'all'}
            </button>
          </div>
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
                <span className="muted skill-card-blurb" title={skill.blurb}>{skill.blurb}</span>
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
    </>
  )
}

export function MemoryList() {
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
      <button className="btn" onClick={() => setExpanded(true)}>
        Review memories
      </button>
    )
  }

  return (
    <div className="memory-list">
      {memories.data?.memories.length === 0 && (
        <span className="faint">Nothing yet</span>
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
  return (
    <div className={`store-card ${entry.installed ? 'is-installed' : ''}`}>
      <div className="store-card-head">
        <span className="store-card-name">{entry.name}</span>
        {entry.detail && <HelpTip text={entry.detail} />}
        {entry.requirement === 'none' && <span className="tag">works now</span>}
        {entry.requirement === 'api_key' && <span className="tag tag-warn">needs a key</span>}
        {entry.requirement === 'local_runtime' && <span className="tag">runs locally</span>}
      </div>

      <span className="muted store-card-blurb">{entry.blurb}</span>

      <div className="store-card-foot">
        <button
          className="btn"
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
      <TitleWithHelp title={label} help={hint} />
      <div className="field-control">{children}</div>
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
