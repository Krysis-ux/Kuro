import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, formatBytes } from '../lib/api'
import { CloudIcon } from '../components/icons'
import { applyTheme, useUi } from '../store/ui'

/** Keys the Rust side reads. `0` (or `-1`) means "decide automatically". */
const KEY_CONTEXT = 'engine.contextSize'
const KEY_GPU_LAYERS = 'engine.gpuLayers'
const KEY_THREADS = 'engine.threads'
const KEY_IDLE = 'engine.idleUnloadMinutes'

export function SettingsPage() {
  const queryClient = useQueryClient()
  const { theme, setTheme } = useUi()

  const status = useQuery({ queryKey: ['status'], queryFn: api.status, refetchInterval: 5000 })
  const hardware = useQuery({ queryKey: ['hardware'], queryFn: api.hardware })
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.settings.get })

  const save = useMutation({
    mutationFn: (patch: Record<string, unknown>) => api.settings.patch(patch),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['settings'] })
      void queryClient.invalidateQueries({ queryKey: ['hardware'] })
    },
  })

  const machine = hardware.data?.hardware

  return (
    <div className="page">
      <header className="page-head">
        <h1>Settings</h1>
      </header>

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

      <section className="panel">
        <h2 className="panel-title">Engine</h2>
        <p className="faint panel-note">
          Applied the next time a model loads. Leave a field empty for automatic.
        </p>

        <NumberField
          label="Context size"
          hint={`Automatic: ${machine?.recommended.context_size ?? '—'} tokens`}
          value={settings.data?.[KEY_CONTEXT]}
          onSave={(value) => save.mutate({ [KEY_CONTEXT]: value })}
        />
        <NumberField
          label="GPU layers"
          hint={`Automatic: ${machine?.recommended.gpu_layers ?? '—'} (all layers offloaded)`}
          value={settings.data?.[KEY_GPU_LAYERS]}
          onSave={(value) => save.mutate({ [KEY_GPU_LAYERS]: value })}
        />
        <NumberField
          label="CPU threads"
          hint={`Automatic: ${machine?.recommended.threads ?? '—'}`}
          value={settings.data?.[KEY_THREADS]}
          onSave={(value) => save.mutate({ [KEY_THREADS]: value })}
        />
        <NumberField
          label="Unload after (minutes)"
          hint="How long an unused model stays in memory. 0 keeps it loaded."
          value={settings.data?.[KEY_IDLE]}
          onSave={(value) => save.mutate({ [KEY_IDLE]: value })}
        />
      </section>

      <section className="panel">
        <h2 className="panel-title">Server</h2>
        <Row label="Status" value={status.data ? 'Running' : 'Unreachable'} />
        <Row label="Address" value={status.data?.address ?? '—'} mono />
        <Row
          label="Uptime"
          value={status.data ? formatUptime(status.data.uptimeSeconds) : '—'}
        />
        <Row label="Version" value={status.data?.version ?? '—'} mono />
        <Row label="Data directory" value={status.data?.dataDirectory ?? '—'} mono />

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

      <section className="panel">
        <h2 className="panel-title">Hardware</h2>
        <Row label="Chip" value={machine?.chip ?? '—'} />
        <Row label="Memory" value={formatBytes(machine?.total_memory_bytes)} />
        <Row label="Cores" value={machine ? `${machine.physical_cores} physical` : '—'} />
        <Row
          label="GPU"
          value={machine?.gpu_available ? machine.gpu_backend : 'CPU only'}
        />
      </section>

      <section className="panel">
        <h2 className="panel-title">Cloud</h2>
        <p className="faint panel-note">
          Kuro is local-first. Cloud is optional and always uses your own provider account.
        </p>

        <div className="cloud-card is-disabled">
          <div className="cloud-card-head">
            <CloudIcon size={16} />
            <span>Your cloud accounts</span>
            <span className="tag">Soon</span>
          </div>
          <p className="muted">
            Connect RunPod, Vast.ai, Lambda Labs or any OpenAI-compatible endpoint. Kuro
            orchestrates through your account; credentials stay in the macOS Keychain.
          </p>
        </div>

        <div className="cloud-card is-disabled">
          <div className="cloud-card-head">
            <CloudIcon size={16} />
            <span>Kuro Cloud</span>
            <span className="tag">Coming soon</span>
          </div>
          <p className="muted">A hosted option for models this machine cannot run.</p>
        </div>
      </section>

      <section className="panel">
        <h2 className="panel-title">API</h2>
        <p className="faint panel-note">
          Kuro speaks the OpenAI API. Point any existing tool at it — no code changes needed.
        </p>
        <pre className="code-block mono">
{`OPENAI_BASE_URL=${status.data?.address ?? 'http://127.0.0.1:8420'}/v1
OPENAI_API_KEY=not-needed`}
        </pre>
      </section>
    </div>
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

/** Number input that only writes when the value actually changes. */
function NumberField({
  label,
  hint,
  value,
  onSave,
}: {
  label: string
  hint: string
  value: unknown
  onSave: (value: number | null) => void
}) {
  const stored = typeof value === 'number' ? String(value) : ''
  const [draft, setDraft] = useState(stored)

  // Adopt the stored value once it loads, without clobbering an in-progress edit.
  useEffect(() => setDraft(stored), [stored])

  const commit = () => {
    const trimmed = draft.trim()
    if (trimmed === stored) return
    onSave(trimmed === '' ? null : Number(trimmed))
  }

  return (
    <Field label={label} hint={hint}>
      <input
        className="input field-input"
        type="number"
        min={0}
        placeholder="Auto"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => event.key === 'Enter' && commit()}
      />
    </Field>
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
