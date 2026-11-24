import { useState } from 'react'
import { Link, Navigate, useLocation, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  api,
  formatBytes,
  type Effort,
  type Subagent,
  type SurfaceSettings,
  type WorkspaceMode,
} from '../lib/api'
import { SliderField } from '../components/Slider'
import { FolderField } from '../components/FolderPicker'
import { HelpTip, TitleWithHelp } from '../components/HelpTip'
import { applyTheme, useUi } from '../store/ui'
import {
  ArrowLeftIcon,
  BracesIcon,
  BrainIcon,
  ChatIcon,
  CheckIcon,
  CpuIcon,
  CubeIcon,
  DriveIcon,
  EyeIcon,
  GlobeIcon,
  KeyIcon,
  PlugIcon,
  PlusIcon,
  PowerIcon,
  RefreshIcon,
  SearchIcon,
  ServerIcon,
  SettingsIcon,
  SparkIcon,
  TerminalIcon,
  ToolIcon,
  UserIcon,
  UsersIcon,
} from '../components/icons'
import { BuiltinPane, McpPane, MemoryList, SearchPane, SkillsPane, Switch } from './Tools'
import { ProvidersPane } from './Providers'

/* ---------- The panes ---------- */

type Group = 'Basics' | 'Models' | 'Agent' | 'System'

interface Pane {
  id: string
  title: string
  group: Group
  icon: React.ReactNode
  /** One short line under the title. */
  subtitle: string
  /** The longer explanation, behind the question mark. */
  help?: string
  /** Extra words the settings search matches. */
  keywords: string
}

const PANES: Pane[] = [
  { id: 'general', title: 'General', group: 'Basics', icon: <SettingsIcon size={15} />, subtitle: 'Appearance and shortcuts.', keywords: 'theme dark light appearance keyboard shortcuts' },
  { id: 'chat', title: 'Chat', group: 'Basics', icon: <ChatIcon size={15} />, subtitle: 'How chat turns behave.', keywords: 'effort orchestrate default' },
  { id: 'coding', title: 'Coding', group: 'Basics', icon: <BracesIcon size={15} />, subtitle: 'How workspaces behave.', keywords: 'agent mode bypass permission workspace' },
  {
    id: 'providers', title: 'Providers', group: 'Models', icon: <KeyIcon size={15} />,
    subtitle: 'Models you reach over the internet.',
    help: 'Your key, your account, your bill. Requests go straight from this machine to the provider. Kuro Free pools free-tier keys into one model.',
    keywords: 'api key openai anthropic openrouter groq free cloud gpu runpod endpoint models',
  },
  {
    id: 'personalization', title: 'Personalization', group: 'Agent', icon: <UserIcon size={15} />,
    subtitle: 'What every model knows about you.',
    help: 'Stored on this machine. It leaves only with a message sent to a provider or free model.',
    keywords: 'memory about you profile remember',
  },
  {
    id: 'subagents', title: 'Subagents', group: 'Agent', icon: <UsersIcon size={15} />,
    subtitle: 'Specialists the coder hands work to.',
    help: 'A subagent gets a fresh context with only the task it was handed, works through it, and returns a short report. Read-only agents are given no tools that write.',
    keywords: 'agents delegate reviewer explorer debugger tester architect specialist',
  },
  {
    id: 'skills', title: 'Skills', group: 'Agent', icon: <SparkIcon size={15} />,
    subtitle: 'Know-how a model loads when it fits.',
    help: 'A turn ranks the enabled skills against the request and sends only what fits its budget.',
    keywords: 'import custom instructions',
  },
  {
    id: 'mcp', title: 'MCP Servers', group: 'Agent', icon: <PlugIcon size={15} />,
    subtitle: 'Tools from other apps and services.',
    help: 'Each server adds tools any model can call. They run on this machine unless the server says otherwise.',
    keywords: 'servers connectors integrations marketplace',
  },
  { id: 'search', title: 'Web Search', group: 'Agent', icon: <GlobeIcon size={15} />, subtitle: 'Where web searches go.', keywords: 'duckduckgo brave tavily searxng web' },
  { id: 'tools', title: 'Built-in Tools', group: 'Agent', icon: <ToolIcon size={15} />, subtitle: 'What a model can call.', keywords: 'builtin functions' },
  { id: 'storage', title: 'Storage', group: 'System', icon: <DriveIcon size={15} />, subtitle: 'Where model files live.', keywords: 'folder disk models directory' },
  { id: 'engine', title: 'Engine', group: 'System', icon: <CpuIcon size={15} />, subtitle: 'How local models run.', help: 'Changes apply the next time a model loads.', keywords: 'context gpu layers threads unload memory' },
  { id: 'server', title: 'Server', group: 'System', icon: <ServerIcon size={15} />, subtitle: "Kuro's local API.", keywords: 'restart stop log address port hardware loaded' },
]

const GROUPS: Group[] = ['Basics', 'Models', 'Agent', 'System']

function matches(pane: Pane, query: string): boolean {
  const needle = query.trim().toLowerCase()
  return !needle || `${pane.title} ${pane.subtitle} ${pane.keywords}`.toLowerCase().includes(needle)
}

/* ---------- Shell ---------- */

/** Settings' own sidebar, shown in place of the app's while Settings are open. */
export function SettingsSidebar({ onBack }: { onBack: () => void }) {
  const navigate = useNavigate()
  // Rendered outside the routes, so it reads the pane from the path itself.
  const current = useLocation().pathname.split('/')[2]
  const [query, setQuery] = useState('')

  return (
    <aside className="sidebar settings-side" aria-label="Settings">
      <button className="side-row" onClick={onBack}>
        <ArrowLeftIcon size={15} />
        <span className="side-row-label">Back to app</span>
      </button>

      <div className="side-search-box">
        <SearchIcon size={13} />
        <input
          placeholder="Search settings"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            const first = PANES.find((pane) => matches(pane, query))
            if (event.key === 'Enter' && first) navigate(`/settings/${first.id}`)
            if (event.key === 'Escape') setQuery('')
          }}
        />
      </div>

      <nav className="side-scroll">
        {GROUPS.map((group) => {
          const panes = PANES.filter((pane) => pane.group === group && matches(pane, query))
          if (panes.length === 0) return null
          return (
            <div key={group}>
              <div className="side-heading">
                <span>{group}</span>
              </div>
              {panes.map((pane) => (
                <Link
                  key={pane.id}
                  to={`/settings/${pane.id}`}
                  className={`side-row ${pane.id === current ? 'active' : ''}`}
                  aria-current={pane.id === current ? 'page' : undefined}
                >
                  {pane.icon}
                  <span className="side-row-label">{pane.title}</span>
                </Link>
              ))}
            </div>
          )
        })}
      </nav>
    </aside>
  )
}

/** The page beside the Settings sidebar: a large title, then the pane. */
export function SettingsPage() {
  const { pane: id } = useParams<{ pane: string }>()
  const queryClient = useQueryClient()
  const tools = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })
  const pane = PANES.find((entry) => entry.id === id)

  if (!pane) return <Navigate to="/settings/general" replace />

  const wide = pane.id === 'providers'
  const agents = tools.data?.agents ?? []

  return (
    <div className={`page settings-page ${wide ? 'is-wide' : ''}`}>
      <header className="set-head">
        <div className="set-head-text">
          <h1>{pane.title}</h1>
          <p className="set-head-sub">
            <span>{pane.subtitle}</span>
            {pane.help && <HelpTip text={pane.help} />}
          </p>
        </div>
        <div className="set-head-trail">
          {pane.id === 'providers' && (
            <>
              <button
                className="btn btn-icon"
                title="Refresh"
                aria-label="Refresh"
                onClick={() => {
                  for (const key of ['providers', 'free', 'models']) {
                    void queryClient.invalidateQueries({ queryKey: [key] })
                  }
                }}
              >
                <RefreshIcon size={14} />
              </button>
              <Link className="btn btn-solid" to="?show=add">
                <PlusIcon size={14} />
                Add provider
              </Link>
            </>
          )}
          {pane.id === 'subagents' && agents.length > 0 && (
            <span className="faint">{agents.length} built in</span>
          )}
        </div>
      </header>

      <PaneBody id={pane.id} />
    </div>
  )
}

function PaneBody({ id }: { id: string }) {
  const tools = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })
  const overview = tools.data

  switch (id) {
    case 'general': return <GeneralPane />
    case 'chat': return <SurfacePane surface="chat" />
    case 'coding': return <CodingPane />
    case 'providers': return <ProvidersPane />
    case 'personalization': return <PersonalizationPane />
    case 'subagents': return <SubagentsPane agents={overview?.agents} />
    case 'mcp': return <McpPane />
    case 'storage': return <StoragePane />
    case 'engine': return <EnginePane />
    case 'server': return <ServerPane />
  }

  // The remaining panes all read the tools overview.
  if (tools.isError) {
    return (
      <section className="panel">
        <p className="form-error">Kuro did not answer.</p>
        <button className="btn" onClick={() => void tools.refetch()}>
          <RefreshIcon size={14} />
          Try again
        </button>
      </section>
    )
  }
  if (!overview) return <p className="faint">Loading…</p>
  if (id === 'skills') return <SkillsPane overview={overview} />
  if (id === 'search') return <SearchPane overview={overview} />
  return <BuiltinPane overview={overview} />
}

/* ---------- Shared bits ---------- */

/** Keys the Rust side reads. `0` (or `-1`) means "decide automatically". */
const KEY = {
  context: 'engine.contextSize',
  gpuLayers: 'engine.gpuLayers',
  threads: 'engine.threads',
  idle: 'engine.idleUnloadMinutes',
  modelsDir: 'models.directory',
  aboutYou: 'memory.aboutYou',
  codeMode: 'code.defaultMode',
} as const

const EFFORTS: { value: Effort; label: string }[] = [
  { value: 'low', label: 'Instant' },
  { value: 'balanced', label: 'Balanced' },
  { value: 'high', label: 'Thinking' },
  { value: 'max', label: 'Extended' },
]

const MODES: { value: WorkspaceMode; label: string; note: string }[] = [
  { value: 'ask', label: 'Ask', note: 'No folder access.' },
  { value: 'plan', label: 'Plan', note: 'Reads, changes nothing.' },
  { value: 'agent', label: 'Agent', note: 'Edits and runs builds. Undoable.' },
  { value: 'bypass', label: 'Bypass', note: 'Any command, no limit.' },
]

/**
 * Context is capped well below what some models advertise: a context larger than memory allows does
 * not fail at the slider, it fails minutes later when the engine is killed.
 */
const MAX_CONTEXT = 131072
const MAX_GPU_LAYERS = 999
const MAX_IDLE_MINUTES = 240

function useSettings() {
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.settings.get })
  const save = useMutation({
    mutationFn: (patch: Record<string, unknown>) => api.settings.patch(patch),
    onSuccess: (data) => {
      queryClient.setQueryData(['settings'], data)
      void queryClient.invalidateQueries({ queryKey: ['hardware'] })
      // The tools overview reports the resolved per-surface settings.
      void queryClient.invalidateQueries({ queryKey: ['tools'] })
    },
  })
  return { settings: settings.data, save: (patch: Record<string, unknown>) => save.mutate(patch) }
}

function Field({ label, help, children }: { label: string; help?: string; children: React.ReactNode }) {
  return (
    <div className="field">
      <TitleWithHelp title={label} help={help} />
      <div className="field-control">{children}</div>
    </div>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="row">
      <span className="muted">{label}</span>
      <span className={mono ? 'mono' : undefined}>{value}</span>
    </div>
  )
}

function Segmented<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: string }[]
  value: T
  onChange: (value: T) => void
}) {
  return (
    <div className="segmented" role="radiogroup">
      {options.map((option) => (
        <button
          key={option.value}
          role="radio"
          aria-checked={value === option.value}
          className={`segment ${value === option.value ? 'is-on' : ''}`}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  )
}

/* ---------- Basics ---------- */

const SHORTCUTS: [string, string][] = [
  ['New chat', '⌘N'],
  ['Send', '↩'],
  ['New line', '⇧↩'],
  ['Slash commands', '/'],
]

function GeneralPane() {
  const { theme, setTheme } = useUi()
  return (
    <>
      <section className="panel">
        <h2 className="panel-title">Appearance</h2>
        <Field label="Theme">
          <Segmented
            options={[
              { value: 'system', label: 'System' },
              { value: 'light', label: 'Light' },
              { value: 'dark', label: 'Dark' },
            ]}
            value={theme}
            onChange={(option) => {
              setTheme(option)
              applyTheme(option)
            }}
          />
        </Field>
      </section>
      <section className="panel">
        <h2 className="panel-title">Keyboard Shortcuts</h2>
        {SHORTCUTS.map(([name, keys]) => (
          <div key={name} className="row">
            <span>{name}</span>
            <kbd className="kbd">{keys}</kbd>
          </div>
        ))}
      </section>
    </>
  )
}

/** One surface's two settings: whether effort may do more than lengthen answers, and where it starts. */
function SurfaceCard({ surface, values }: { surface: 'chat' | 'code'; values?: SurfaceSettings }) {
  const { save } = useSettings()
  const autoHelp =
    surface === 'chat'
      ? 'Higher effort also picks reasoning guidance that suits the question, and gives the model more room to search.'
      : 'Higher effort buys more rounds of reading, editing and running, and brings in skills for the languages the project uses.'

  return (
    <section className="panel">
      <Field label="Auto-orchestrate" help={autoHelp}>
        <Switch
          checked={values?.autoOrchestrate ?? true}
          label="Auto-orchestrate"
          onChange={(value) => save({ [`${surface}.autoOrchestrate`]: value })}
        />
      </Field>
      <Field label="Starting effort" help="Where the effort control beside the message box starts. It can still be changed per message.">
        <Segmented
          options={EFFORTS}
          value={values?.defaultEffort ?? (surface === 'chat' ? 'balanced' : 'high')}
          onChange={(value) => save({ [`${surface}.defaultEffort`]: value })}
        />
      </Field>
    </section>
  )
}

function SurfacePane({ surface }: { surface: 'chat' | 'code' }) {
  const tools = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })
  return <SurfaceCard surface={surface} values={tools.data?.surfaces[surface]} />
}

function CodingPane() {
  const tools = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })
  const { save } = useSettings()
  const current = tools.data?.surfaces.code.defaultMode ?? 'agent'
  const essentials = tools.data?.skills.essentials ?? []

  return (
    <>
      <SurfaceCard surface="code" values={tools.data?.surfaces.code} />

      <section className="panel">
        <h2 className="panel-title">
          <TitleWithHelp
            title="Starting Permission"
            help="The mode a new workspace opens in; change it per workspace beside the message box. Agent is the default because every edit can be undone from Changes. A command that already ran cannot be, which is why Bypass is never the default."
          />
        </h2>
        <div className="mode-choices">
          {MODES.map((mode) => (
            <button
              key={mode.value}
              className={`mode-choice ${current === mode.value ? 'is-on' : ''} is-${mode.value}`}
              onClick={() => save({ [KEY.codeMode]: mode.value })}
            >
              <span className="mode-choice-label">{mode.label}</span>
              <span className="muted">{mode.note}</span>
            </button>
          ))}
        </div>
      </section>

      {essentials.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">
            <TitleWithHelp title="Always On" help="These go into every coding brief and have no switch: an assistant that edits a file it has not read destroys work." />
          </h2>
          {essentials.map((skill) => (
            <div key={skill.slug} className="row">
              <span>{skill.name}</span>
              <span className="faint row-trail" title={skill.blurb}>{skill.blurb}</span>
            </div>
          ))}
        </section>
      )}
    </>
  )
}

/* ---------- Agent ---------- */

function PersonalizationPane() {
  const queryClient = useQueryClient()
  const { settings, save } = useSettings()
  const tools = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })
  const stored = String(settings?.[KEY.aboutYou] ?? '')
  const [draft, setDraft] = useState<string | null>(null)
  const text = draft ?? stored

  const setPreload = useMutation({
    mutationFn: (value: boolean) => api.tools.setDefaults({ memoryPreload: value }),
    onSuccess: (data) => queryClient.setQueryData(['tools'], data),
  })

  const count = tools.data?.memory.count ?? 0

  return (
    <>
      <section className="panel">
        <h2 className="panel-title">
          <TitleWithHelp title="About You" help="Your work, your stack, how you like answers. Anything you would otherwise retype. Sent at the start of every conversation." />
        </h2>
        <textarea
          className="input about-you"
          rows={4}
          placeholder="I write Rust and TypeScript. Short answers, code first."
          value={text}
          onChange={(event) => setDraft(event.target.value)}
        />
        <div className="panel-foot">
          <span className="faint">{text === stored ? 'Sent with every conversation.' : 'Not saved yet.'}</span>
          <span className="spacer" />
          <button
            className="btn btn-solid"
            disabled={text === stored}
            onClick={() => {
              save({ [KEY.aboutYou]: text })
              setDraft(null)
            }}
          >
            Save
          </button>
        </div>
      </section>

      <section className="panel">
        <h2 className="panel-title">
          <BrainIcon size={15} />
          Memory
        </h2>
        <Field label="Preload memories" help="Puts saved memories in front of the model. Without it, memory works only when the model thinks to look, which small models often do not.">
          <Switch
            checked={tools.data?.memory.preload ?? true}
            label="Preload memories"
            onChange={(value) => setPreload.mutate(value)}
          />
        </Field>
        <Row label="Saved by models" value={`${count} ${count === 1 ? 'memory' : 'memories'}`} />
        <MemoryList />
      </section>
    </>
  )
}

const AGENT_ICON: Record<string, React.ReactNode> = {
  explore: <SearchIcon size={15} />,
  frontend: <EyeIcon size={15} />,
  backend: <ServerIcon size={15} />,
  debug: <TerminalIcon size={15} />,
  test: <CheckIcon size={15} />,
  review: <EyeIcon size={15} />,
  architect: <CubeIcon size={15} />,
  refactor: <RefreshIcon size={15} />,
}

function SubagentsPane({ agents }: { agents?: Subagent[] }) {
  if (!agents || agents.length === 0) {
    return (
      <section className="panel empty-card">
        <UsersIcon size={20} />
        <strong>No subagents</strong>
        <span className="faint">Restart Kuro to load them.</span>
      </section>
    )
  }

  return (
    <>
      <p className="set-hint">
        <BracesIcon size={13} />
        Used in Code. Ask for one by name: “have the reviewer check this”.
      </p>
      <div className="agent-grid">
        {agents.map((agent) => (
          <article key={agent.slug} className="agent-card">
            <span className="icon-tile">{AGENT_ICON[agent.slug] ?? <UserIcon size={15} />}</span>
            <div className="agent-card-body">
              <div className="agent-card-head">
                <strong>{agent.name}</strong>
                <span className={`tag ${agent.writes ? '' : 'tag-live'}`}>
                  {agent.writes ? 'Can edit' : 'Read-only'}
                </span>
                <span className="spacer" />
                <HelpTip text={agent.when} />
              </div>
              <span className="agent-card-blurb muted">{agent.blurb}</span>
              {agent.skills.length > 0 && (
                <div className="chip-row">
                  {agent.skills.map((skill) => (
                    <span key={skill} className="chip">{skill}</span>
                  ))}
                </div>
              )}
            </div>
          </article>
        ))}
      </div>
    </>
  )
}

/* ---------- System ---------- */

function StoragePane() {
  const { settings, save } = useSettings()
  const status = useQuery({ queryKey: ['status'], queryFn: api.status })
  const folder = String(settings?.[KEY.modelsDir] ?? '')

  return (
    <section className="panel">
      <h2 className="panel-title">
        <TitleWithHelp
          title="Models Folder"
          help="Model files are the only large thing Kuro stores, so this is the one path worth moving. Read at start-up: restart the server after changing it. Downloads already made stay where they are."
        />
      </h2>
      <FolderField
        value={folder}
        onChange={(path) => save({ [KEY.modelsDir]: path })}
        title="Choose where to keep model files"
        placeholder={status.data?.dataDirectory ?? "Kuro's own data directory"}
      />
      <Row label="Currently" value={folder || `${status.data?.dataDirectory ?? '—'} (default)`} mono />
    </section>
  )
}

function EnginePane() {
  const { settings, save } = useSettings()
  const hardware = useQuery({ queryKey: ['hardware'], queryFn: api.hardware })
  const machine = hardware.data?.hardware

  return (
    <section className="panel">
      <SliderField
        label="Context size"
        hint="What the model sees at once."
        unit="tokens"
        autoValue={machine?.recommended.context_size}
        value={settings?.[KEY.context]}
        min={512}
        max={MAX_CONTEXT}
        step={512}
        onSave={(value) => save({ [KEY.context]: value })}
      />
      <SliderField
        label="GPU layers"
        hint="The maximum offloads every layer."
        autoValue={machine?.recommended.gpu_layers}
        value={settings?.[KEY.gpuLayers]}
        min={0}
        max={MAX_GPU_LAYERS}
        step={1}
        zeroLabel="CPU only"
        onSave={(value) => save({ [KEY.gpuLayers]: value })}
      />
      <SliderField
        label="CPU threads"
        hint={`${machine?.physical_cores ?? '—'} physical cores here.`}
        autoValue={machine?.recommended.threads}
        value={settings?.[KEY.threads]}
        min={1}
        max={machine?.logical_cores ?? 16}
        step={1}
        onSave={(value) => save({ [KEY.threads]: value })}
      />
      <SliderField
        label="Unload after"
        hint="Idle time before memory is given back."
        unit="min"
        autoValue={30}
        value={settings?.[KEY.idle]}
        min={0}
        max={MAX_IDLE_MINUTES}
        step={5}
        zeroLabel="never unload"
        onSave={(value) => save({ [KEY.idle]: value })}
      />
    </section>
  )
}

function ServerPane() {
  const status = useQuery({ queryKey: ['status'], queryFn: api.status, refetchInterval: 5000 })
  const hardware = useQuery({ queryKey: ['hardware'], queryFn: api.hardware })
  const machine = hardware.data?.hardware
  const loaded = status.data?.loadedModels ?? []

  return (
    <>
      <section className="panel">
        <Row label="Status" value={status.data ? 'Running' : 'Unreachable'} />
        <Row label="Address" value={status.data?.address ?? '—'} mono />
        <Row label="Uptime" value={status.data ? formatUptime(status.data.uptimeSeconds) : '—'} />
        <Row label="Version" value={status.data?.version ?? '—'} mono />
        <Row label="Data folder" value={status.data?.dataDirectory ?? '—'} mono />
        <ServerControls />
      </section>

      <section className="panel">
        <h2 className="panel-title">Loaded Models</h2>
        {loaded.length === 0 ? (
          <span className="faint">None</span>
        ) : (
          loaded.map((engine) => (
            <div key={engine.model_id} className="row">
              <span className="mono">{engine.model_id}</span>
              <span className="faint mono">
                :{engine.port} · idle {engine.idle_seconds}s
              </span>
            </div>
          ))
        )}
      </section>

      <section className="panel">
        <h2 className="panel-title">Hardware</h2>
        <Row label="Chip" value={machine?.chip ?? '—'} />
        <Row label="Memory" value={formatBytes(machine?.total_memory_bytes)} />
        <Row label="Cores" value={machine ? `${machine.physical_cores} physical · ${machine.logical_cores} logical` : '—'} />
        <Row label="GPU" value={machine?.gpu_available ? machine.gpu_backend : 'CPU only'} />
      </section>
    </>
  )
}

/**
 * Restart and stop. Stop asks first: it closes the API every other tool on the machine may be
 * pointed at, and the only way back is a terminal. /
 */
function ServerControls() {
  const [confirming, setConfirming] = useState(false)
  const [stopped, setStopped] = useState(false)

  const stop = useMutation({
    mutationFn: () => api.shutdown(),
    onSuccess: () => setStopped(true),
    // A connection error usually means it worked and the socket closed first.
    onError: () => setStopped(true),
  })

  // Wait for the successor before reloading: reloading at once lands on a dead port.
  const restart = useMutation({
    mutationFn: async () => {
      await api.restart().catch(() => undefined)
      await api.waitUntilHealthy()
    },
    onSuccess: () => window.location.reload(),
  })

  if (stopped) {
    return (
      <div className="row">
        <span className="muted">Stopped</span>
        <code className="mono">kuro serve</code>
      </div>
    )
  }

  return (
    <div className="row">
      <span className="muted">
        {restart.isPending ? 'Restarting…' : confirming ? 'Unload every model and stop?' : 'Control'}
      </span>
      <div className="inline-form">
        {restart.error && <span className="form-error">{restart.error.message}</span>}
        {confirming ? (
          <>
            <button className="btn" onClick={() => setConfirming(false)}>Cancel</button>
            <button className="btn btn-danger" onClick={() => stop.mutate()} disabled={stop.isPending}>
              <PowerIcon size={13} />
              Stop
            </button>
          </>
        ) : (
          <>
            <button className="btn" onClick={() => restart.mutate()} disabled={restart.isPending}>
              {restart.isPending ? <span className="spinner" /> : <RefreshIcon size={13} />}
              Restart
            </button>
            <button className="btn" onClick={() => setConfirming(true)}>
              <PowerIcon size={13} />
              Stop
            </button>
          </>
        )}
      </div>
    </div>
  )
}

function formatUptime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}
