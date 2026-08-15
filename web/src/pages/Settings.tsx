import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, formatBytes, type Effort, type WorkspaceMode } from '../lib/api'
import { SliderField } from '../components/Slider'
import { Switch } from './Tools'
import { FolderField } from '../components/FolderPicker'
import { applyTheme, useUi } from '../store/ui'
import { PowerIcon, RefreshIcon } from '../components/icons'

const KEY_CONTEXT = 'engine.contextSize'
const KEY_GPU_LAYERS = 'engine.gpuLayers'
const KEY_THREADS = 'engine.threads'
const KEY_IDLE = 'engine.idleUnloadMinutes'

const KEY_CHAT_AUTO = 'chat.autoOrchestrate'
const KEY_CODE_AUTO = 'code.autoOrchestrate'
const KEY_CHAT_EFFORT = 'chat.defaultEffort'
const KEY_CODE_EFFORT = 'code.defaultEffort'
const KEY_CODE_MODE = 'code.defaultMode'
const KEY_MODELS_DIR = 'models.directory'
const KEY_ABOUT_YOU = 'memory.aboutYou'

const EFFORTS: { value: Effort; label: string }[] = [
  { value: 'low', label: 'Instant' },
  { value: 'balanced', label: 'Balanced' },
  { value: 'high', label: 'Thinking' },
  { value: 'max', label: 'Extended' },
]

const MODES: { value: WorkspaceMode; label: string; note: string }[] = [
  { value: 'ask', label: 'Ask', note: 'No access to the folder at all.' },
  { value: 'plan', label: 'Plan', note: 'Reads the project. Changes nothing.' },
  {
    value: 'agent',
    label: 'Agent',
    note: 'Changes files and runs build and test commands. Every edit can be undone.',
  },
  {
    value: 'bypass',
    label: 'Bypass',
    note: 'The same, with no limit on which commands may run.',
  },
]

type Tab = 'chat' | 'coder' | 'models' | 'engine' | 'server'

const TABS: { id: Tab; label: string }[] = [
  { id: 'chat', label: 'Chat' },
  { id: 'coder', label: 'Coder' },
  { id: 'models', label: 'Models and storage' },
  { id: 'engine', label: 'Engine' },
  { id: 'server', label: 'Server' },
]

const MAX_CONTEXT = 131072
const MAX_GPU_LAYERS = 999
const MAX_IDLE_MINUTES = 240

export function SettingsPage() {
  const queryClient = useQueryClient()
  const { theme, setTheme } = useUi()
  const [tab, setTab] = useState<Tab>('chat')

  const status = useQuery({ queryKey: ['status'], queryFn: api.status, refetchInterval: 5000 })
  const hardware = useQuery({ queryKey: ['hardware'], queryFn: api.hardware })
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.settings.get })
  const tools = useQuery({ queryKey: ['tools'], queryFn: api.tools.overview })

  const save = useMutation({
    mutationFn: (patch: Record<string, unknown>) => api.settings.patch(patch),
    onSuccess: (data) => {
      queryClient.setQueryData(['settings'], data)
      void queryClient.invalidateQueries({ queryKey: ['hardware'] })
      void queryClient.invalidateQueries({ queryKey: ['tools'] })
    },
  })

  const machine = hardware.data?.hardware
  const maxThreads = machine?.logical_cores ?? 16
  const surfaces = tools.data?.surfaces

  return (
    <div className="page">
      <header className="page-head">
        <h1>Settings</h1>
        <div className="settings-tabs">
          {TABS.map((entry) => (
            <button
              key={entry.id}
              className={`settings-tab ${tab === entry.id ? 'is-on' : ''}`}
              onClick={() => setTab(entry.id)}
            >
              {entry.label}
            </button>
          ))}
        </div>
      </header>

      {tab === 'chat' && (
        <SurfaceSection
          title="Chat"
          note="How an ordinary conversation behaves. These do not affect the Code page."
          autoKey={KEY_CHAT_AUTO}
          effortKey={KEY_CHAT_EFFORT}
          auto={surfaces?.chat.autoOrchestrate ?? true}
          effort={surfaces?.chat.defaultEffort ?? 'balanced'}
          autoHint="Higher effort also picks the reasoning guidance that suits the question, and gives the model more room to search before answering."
          onSave={(patch) => save.mutate(patch)}
        />
      )}

      {tab === 'chat' && (
        <MemorySection
          preload={tools.data?.memory.preload ?? true}
          count={tools.data?.memory.count ?? 0}
          aboutYou={String(settings.data?.[KEY_ABOUT_YOU] ?? '')}
          onSaveAbout={(text) => save.mutate({ [KEY_ABOUT_YOU]: text })}
        />
      )}

      {tab === 'chat' && (
        <section className="panel">
          <h2 className="panel-title">Appearance</h2>
          <Field label="Theme" hint="Follows the system by default.">
            <div className="segmented">
              {(['system', 'light', 'dark'] as const).map((option) => (
                <button
                  key={option}
                  className={`segment ${theme === option ? 'is-on' : ''}`}
                  onClick={() => {
                    setTheme(option)
                    applyTheme(option)
                  }}
                >
                  {option}
                </button>
              ))}
            </div>
          </Field>
        </section>
      )}

      {tab === 'coder' && (
        <>
          <SurfaceSection
            title="Coder"
            note="How a coding workspace behaves. Effort matters more here: the first rounds of a coding turn are spent reading the project rather than answering."
            autoKey={KEY_CODE_AUTO}
            effortKey={KEY_CODE_EFFORT}
            auto={surfaces?.code.autoOrchestrate ?? true}
            effort={surfaces?.code.defaultEffort ?? 'high'}
            autoHint="Higher effort buys more rounds of reading, editing and running — and brings in the skills that match what the project is written in, read from its own manifest files."
            onSave={(patch) => save.mutate(patch)}
          />

          <section className="panel">
            <h2 className="panel-title">Permission</h2>
            <p className="faint panel-note">
              The mode a newly opened workspace starts in. It can be changed per workspace at
              any time, from the switch beside the message box.
            </p>

            <div className="mode-choices">
              {MODES.map((mode) => {
                const current = surfaces?.code.defaultMode ?? 'agent'
                return (
                  <button
                    key={mode.value}
                    className={`mode-choice ${current === mode.value ? 'is-on' : ''} is-${mode.value}`}
                    onClick={() => save.mutate({ [KEY_CODE_MODE]: mode.value })}
                  >
                    <span className="mode-choice-label">{mode.label}</span>
                    <span className="muted">{mode.note}</span>
                  </button>
                )
              })}
            </div>

            <p className="faint panel-note">
              Agent is the default because every file it changes is recorded with the previous
              contents and can be undone from the Changes panel. A command that has already
              run cannot be — that is what Bypass removes the limit on, and why it is never
              the default.
            </p>
          </section>

          {tools.data && (
            <section className="panel">
              <h2 className="panel-title">Always on when coding</h2>
              <p className="faint panel-note">
                These go into every coding brief and have no switch. They describe things that
                are not preferences — an assistant that edits a file it has not read is one
                that destroys work.
              </p>
              <ul className="skill-essential-list">
                {tools.data.skills.essentials.map((skill) => (
                  <li key={skill.slug}>
                    <span className="skill-card-name">{skill.name}</span>
                    <span className="muted">{skill.blurb}</span>
                  </li>
                ))}
              </ul>
              <div className="panel-foot">
                <span className="faint">
                  Everything else is a choice, on the Tools page.
                </span>
              </div>
            </section>
          )}
        </>
      )}

      {tab === 'models' && (
        <section className="panel">
          <h2 className="panel-title">Where models are kept</h2>
          <p className="faint panel-note">
            Model files are the only large thing Kuro stores, so this is the only path worth
            moving — point it at an external drive if the boot disk is tight. Everything else
            (the database, logs, the engine) stays put.
          </p>
          <p className="faint panel-note">
            It is read when Kuro starts, so <strong>restart the server</strong> from the Server
            tab after changing it. Anything already downloaded stays where it is and keeps
            working; only new downloads go to the new place.
          </p>
          <FolderField
            value={String(settings.data?.[KEY_MODELS_DIR] ?? '')}
            onChange={(path) => save.mutate({ [KEY_MODELS_DIR]: path })}
            title="Choose where to keep model files"
            placeholder={status.data?.dataDirectory ?? "Kuro's own data directory"}
          />
          <div className="row">
            <span className="faint">Currently</span>
            <span className="mono">
              {String(settings.data?.[KEY_MODELS_DIR] ?? '') ||
                `${status.data?.dataDirectory ?? '—'} (default)`}
            </span>
          </div>
        </section>
      )}

      {tab === 'engine' && (
      <section className="panel">
        <h2 className="panel-title">Engine</h2>
        <p className="faint panel-note">
          Applied the next time a model loads. Drag to adjust, or click the number to type an exact
          value.
        </p>

        <SliderField
          label="Context size"
          hint="How much of the conversation the model can see at once. More costs memory."
          unit="tokens"
          autoValue={machine?.recommended.context_size}
          value={settings.data?.[KEY_CONTEXT]}
          min={512}
          max={MAX_CONTEXT}
          step={512}
          onSave={(value) => save.mutate({ [KEY_CONTEXT]: value })}
        />

        <SliderField
          label="GPU layers"
          hint="Layers offloaded to the GPU. The maximum offloads all of them, which is almost always what you want."
          autoValue={machine?.recommended.gpu_layers}
          value={settings.data?.[KEY_GPU_LAYERS]}
          min={0}
          max={MAX_GPU_LAYERS}
          step={1}
          zeroLabel="CPU only"
          onSave={(value) => save.mutate({ [KEY_GPU_LAYERS]: value })}
        />

        <SliderField
          label="CPU threads"
          hint={`This machine has ${machine?.physical_cores ?? '—'} physical cores. More than that usually slows things down.`}
          autoValue={machine?.recommended.threads}
          value={settings.data?.[KEY_THREADS]}
          min={1}
          max={maxThreads}
          step={1}
          onSave={(value) => save.mutate({ [KEY_THREADS]: value })}
        />

        <SliderField
          label="Unload after"
          hint="How long an unused model stays in memory before its memory is given back."
          unit="min"
          autoValue={30}
          value={settings.data?.[KEY_IDLE]}
          min={0}
          max={MAX_IDLE_MINUTES}
          step={5}
          zeroLabel="never unload"
          onSave={(value) => save.mutate({ [KEY_IDLE]: value })}
        />
      </section>
      )}

      {tab === 'server' && (
      <section className="panel">
        <h2 className="panel-title">Server</h2>
        <Row label="Status" value={status.data ? 'Running' : 'Unreachable'} />
        <Row label="Address" value={status.data?.address ?? '—'} mono />
        <Row label="Uptime" value={status.data ? formatUptime(status.data.uptimeSeconds) : '—'} />
        <Row label="Version" value={status.data?.version ?? '—'} mono />
        <Row label="Data directory" value={status.data?.dataDirectory ?? '—'} mono />

        <ShutdownControl />

        <div className="loaded-models">
          <span className="faint">Loaded models</span>
          {status.data?.loadedModels.length === 0 ? (
            <span className="faint">none</span>
          ) : (
            status.data?.loadedModels.map((engine) => (
              <div key={engine.model_id} className="loaded-row">
                <span className="mono">{engine.model_id}</span>
                <span className="faint mono">
                  port {engine.port} · pid {engine.pid} · idle {engine.idle_seconds}s
                </span>
              </div>
            ))
          )}
        </div>
      </section>
      )}

      {tab === 'server' && (
      <section className="panel">
        <h2 className="panel-title">Hardware</h2>
        <Row label="Chip" value={machine?.chip ?? '—'} />
        <Row label="Memory" value={formatBytes(machine?.total_memory_bytes)} />
        <Row
          label="Cores"
          value={
            machine ? `${machine.physical_cores} physical · ${machine.logical_cores} logical` : '—'
          }
        />
        <Row label="GPU" value={machine?.gpu_available ? machine.gpu_backend : 'CPU only'} />
      </section>
      )}

    </div>
  )
}

function MemorySection({
  preload,
  count,
  aboutYou,
  onSaveAbout,
}: {
  preload: boolean
  count: number
  aboutYou: string
  onSaveAbout: (text: string) => void
}) {
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState(aboutYou)
  const [saved, setSaved] = useState(false)
  const [touched, setTouched] = useState(false)

  if (!touched && draft !== aboutYou) setDraft(aboutYou)

  const setPreload = useMutation({
    mutationFn: (value: boolean) => api.tools.setDefaults({ memoryPreload: value }),
    onSuccess: (data) => queryClient.setQueryData(['tools'], data),
  })

  return (
    <section className="panel">
      <h2 className="panel-title">Memory</h2>
      <p className="faint panel-note">
        On, always. It reads and writes only what you have asked to be kept, and it never
        leaves this machine — so there is no switch for it beside the message box any more.
      </p>

      <div className="field field-stacked">
        <div className="field-label">
          <span>What should models know about you?</span>
          <span className="faint field-hint">
            Included at the start of every conversation. Your work, your stack, how you want
            to be answered — anything you would otherwise retype.
          </span>
        </div>
        <textarea
          className="input about-you"
          rows={5}
          placeholder="I write Rust and TypeScript. Prefer short answers with code over explanation. I am on macOS."
          value={draft}
          onChange={(event) => {
            setTouched(true)
            setSaved(false)
            setDraft(event.target.value)
          }}
        />
        <div className="panel-foot">
          <button
            className="btn btn-solid btn-sm"
            disabled={draft === aboutYou}
            onClick={() => {
              onSaveAbout(draft)
              setTouched(false)
              setSaved(true)
            }}
          >
            Save
          </button>
          <span className="faint">
            {saved && draft === aboutYou
              ? 'Saved. Every new message includes it.'
              : draft === aboutYou
                ? 'Nothing to save.'
                : 'Not saved yet.'}
          </span>
        </div>
      </div>

      <div className="field">
        <div className="field-label">
          <span>Put saved memories in front of the model automatically</span>
          <span className="faint field-hint">
            Without this, memory only works when the model thinks to look — which small
            models often do not.
          </span>
        </div>
        <Switch
          checked={preload}
          label="Preload memories"
          onChange={(value) => setPreload.mutate(value)}
        />
      </div>

      <div className="row">
        <span className="faint">Saved by models</span>
        <span>
          {count} {count === 1 ? 'memory' : 'memories'} · review them on the Tools page
        </span>
      </div>
    </section>
  )
}

function SurfaceSection({
  title,
  note,
  autoKey,
  effortKey,
  auto,
  effort,
  autoHint,
  onSave,
}: {
  title: string
  note: string
  autoKey: string
  effortKey: string
  auto: boolean
  effort: Effort
  autoHint: string
  onSave: (patch: Record<string, unknown>) => void
}) {
  return (
    <section className="panel">
      <h2 className="panel-title">{title}</h2>
      <p className="faint panel-note">{note}</p>

      <div className="field">
        <div className="field-label">
          <span>Auto-orchestrate</span>
          <span className="faint field-hint">{autoHint}</span>
        </div>
        <Switch
          checked={auto}
          label="Auto-orchestrate"
          onChange={(value) => onSave({ [autoKey]: value })}
        />
      </div>

      <Field
        label="Starting effort"
        hint="What the control beside the message box starts at. It can still be changed per message."
      >
        <div className="segmented">
          {EFFORTS.map((option) => (
            <button
              key={option.value}
              className={`segment ${effort === option.value ? 'is-on' : ''}`}
              onClick={() => onSave({ [effortKey]: option.value })}
            >
              {option.label}
            </button>
          ))}
        </div>
      </Field>
    </section>
  )
}

function ShutdownControl() {
  const [confirming, setConfirming] = useState(false)
  const [stopped, setStopped] = useState(false)
  const [restartError, setRestartError] = useState<string | null>(null)

  const stop = useMutation({
    mutationFn: () => api.shutdown(),
    onSuccess: () => setStopped(true),
    onError: () => setStopped(true),
  })

  const restart = useMutation({
    mutationFn: async () => {
      setRestartError(null)
      await api.restart().catch(() => undefined)
      await api.waitUntilHealthy()
    },
    onSuccess: () => window.location.reload(),
    onError: (error: Error) => setRestartError(error.message),
  })

  if (stopped) {
    return (
      <div className="row">
        <span className="faint">Server</span>
        <span className="muted">
          Stopped. Run <code className="mono">kuro serve</code> to start it again.
        </span>
      </div>
    )
  }

  if (restart.isPending) {
    return (
      <div className="row">
        <span className="faint">Server</span>
        <span className="muted inline-form">
          <span className="spinner" />
          Restarting — waiting for it to come back…
        </span>
      </div>
    )
  }

  return (
    <>
      <div className="row">
        <span className="faint">Restart</span>
        <div className="inline-form shutdown-confirm">
          <span className="faint">Applies engine settings and clears a stuck engine.</span>
          <button className="btn btn-ghost btn-sm" onClick={() => restart.mutate()}>
            <RefreshIcon size={13} />
            Restart server
          </button>
        </div>
      </div>

      {restartError && (
        <div className="row">
          <span className="faint">Restart</span>
          <span className="form-error">{restartError}</span>
        </div>
      )}

      <div className="row">
        <span className="faint">Shut down</span>
        {confirming ? (
          <div className="inline-form shutdown-confirm">
            <span className="muted">Unload every model and stop the server?</span>
            <button
              className="btn btn-danger btn-sm"
              onClick={() => stop.mutate()}
              disabled={stop.isPending}
            >
              {stop.isPending ? <span className="spinner" /> : <PowerIcon size={13} />}
              Stop
            </button>
            <button className="btn btn-ghost btn-sm" onClick={() => setConfirming(false)}>
              Cancel
            </button>
          </div>
        ) : (
          <button className="btn btn-ghost btn-sm" onClick={() => setConfirming(true)}>
            <PowerIcon size={13} />
            Stop server
          </button>
        )}
      </div>
    </>
  )
}

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
    <div className="field">
      <div className="field-label">
        <span>{label}</span>
        {hint && <span className="faint field-hint">{hint}</span>}
      </div>
      <div className="field-control">{children}</div>
    </div>
  )
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="row">
      <span className="faint">{label}</span>
      <span className={mono ? 'mono' : undefined}>{value}</span>
    </div>
  )
}

function formatUptime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}
