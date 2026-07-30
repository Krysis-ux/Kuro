import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, formatBytes } from '../lib/api'
import { SliderField } from '../components/Slider'
import { applyTheme, useUi } from '../store/ui'
import { PowerIcon, RefreshIcon } from '../components/icons'

/** Keys the Rust side reads. `0` (or `-1`) means "decide automatically". */
const KEY_CONTEXT = 'engine.contextSize'
const KEY_GPU_LAYERS = 'engine.gpuLayers'
const KEY_THREADS = 'engine.threads'
const KEY_IDLE = 'engine.idleUnloadMinutes'

/**
 * Upper bounds for the engine sliders.
 *
 * Context is capped well below what some models advertise, because a context
 * larger than memory allows does not fail at the slider — it fails minutes later
 * when the engine is killed. The exact field still accepts anything, for someone
 * who knows their machine better than this heuristic does.
 */
const MAX_CONTEXT = 131072
const MAX_GPU_LAYERS = 999
const MAX_IDLE_MINUTES = 240

export function SettingsPage() {
  const queryClient = useQueryClient()
  const { theme, setTheme } = useUi()

  const status = useQuery({ queryKey: ['status'], queryFn: api.status, refetchInterval: 5000 })
  const hardware = useQuery({ queryKey: ['hardware'], queryFn: api.hardware })
  const settings = useQuery({ queryKey: ['settings'], queryFn: api.settings.get })

  const save = useMutation({
    mutationFn: (patch: Record<string, unknown>) => api.settings.patch(patch),
    onSuccess: (data) => {
      queryClient.setQueryData(['settings'], data)
      void queryClient.invalidateQueries({ queryKey: ['hardware'] })
    },
  })

  const machine = hardware.data?.hardware
  const maxThreads = machine?.logical_cores ?? 16

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

/**
 * Stop the server.
 *
 * Two clicks, because the consequence is not obvious from the button: this closes
 * the API every other tool on the machine may be pointed at, and the only way back
 * is a terminal. Confirmation is the difference between a deliberate stop and a
 * misclick that looks like a crash.
 */
function ShutdownControl() {
  const [confirming, setConfirming] = useState(false)
  const [stopped, setStopped] = useState(false)
  const [restartError, setRestartError] = useState<string | null>(null)

  const stop = useMutation({
    mutationFn: () => api.shutdown(),
    onSuccess: () => setStopped(true),
    // A connection error here usually means it worked and the socket closed
    // before the response arrived, which is success, not failure.
    onError: () => setStopped(true),
  })

  /**
   * Restart, then wait for the successor before reloading.
   *
   * Reloading immediately would land on a dead port; a fixed delay would be a
   * guess. Polling `/api/health` is the only version that is both correct and as
   * fast as the machine allows.
   */
  const restart = useMutation({
    mutationFn: async () => {
      setRestartError(null)
      // The request itself may not complete — the server is going down — so a
      // failure here is not yet a failure of the restart.
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
