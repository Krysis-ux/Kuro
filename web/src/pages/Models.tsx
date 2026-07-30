import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, formatBytes, type FitVerdict } from '../lib/api'
import { DownloadIcon, TrashIcon } from '../components/icons'

export function ModelsPage() {
  const queryClient = useQueryClient()
  const [reference, setReference] = useState('')
  const [pullError, setPullError] = useState<string | null>(null)

  const installed = useQuery({ queryKey: ['models'], queryFn: api.models.list })
  const recommended = useQuery({ queryKey: ['recommended'], queryFn: api.models.recommended })

  // While anything is downloading, poll often enough for the bar to feel live.
  const downloads = useQuery({
    queryKey: ['downloads'],
    queryFn: api.downloads.list,
    refetchInterval: (query) => {
      const active = query.state.data?.downloads.some(
        (download) => download.status === 'downloading' || download.status === 'queued',
      )
      return active ? 700 : false
    },
  })

  const refreshAll = () => {
    void queryClient.invalidateQueries({ queryKey: ['models'] })
    void queryClient.invalidateQueries({ queryKey: ['recommended'] })
    void queryClient.invalidateQueries({ queryKey: ['downloads'] })
  }

  const pull = useMutation({
    mutationFn: (model: string) => api.models.pull(model),
    onSuccess: () => {
      setReference('')
      setPullError(null)
      refreshAll()
    },
    onError: (error: Error) => setPullError(error.message),
  })

  const remove = useMutation({
    mutationFn: (id: string) => api.models.remove(id),
    onSuccess: refreshAll,
  })

  const activeDownloads =
    downloads.data?.downloads.filter(
      (download) => download.status === 'downloading' || download.status === 'queued',
    ) ?? []

  // A finished download changes what is installed, so refresh once it lands.
  if (activeDownloads.length === 0 && downloads.isFetched && installed.isStale) {
    void queryClient.invalidateQueries({ queryKey: ['models'] })
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>Models</h1>
        <p className="muted">Weights are stored on this machine and never leave it.</p>
      </header>

      <section className="panel">
        <h2 className="panel-title">Add a model</h2>
        <form
          className="pull-form"
          onSubmit={(event) => {
            event.preventDefault()
            if (reference.trim()) pull.mutate(reference.trim())
          }}
        >
          <input
            className="input"
            placeholder="unsloth/Qwen3-4B-Instruct-2507-GGUF  ·  or a Hugging Face URL"
            value={reference}
            onChange={(event) => setReference(event.target.value)}
          />
          <button className="btn btn-solid" disabled={!reference.trim() || pull.isPending}>
            {pull.isPending ? <span className="spinner" /> : <DownloadIcon size={15} />}
            Download
          </button>
        </form>
        <p className="faint form-hint">
          Add <code className="mono">:Q5_K_M</code> to choose a quantization. Only GGUF weights can
          run.
        </p>
        {pullError && <p className="form-error">{pullError}</p>}
      </section>

      {activeDownloads.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">Downloading</h2>
          {activeDownloads.map((download) => {
            const fraction =
              download.total_bytes && download.total_bytes > 0
                ? download.downloaded_bytes / download.total_bytes
                : 0
            return (
              <div key={download.id} className="download">
                <div className="download-head">
                  <span>{download.label}</span>
                  <span className="faint mono">
                    {formatBytes(download.downloaded_bytes)} / {formatBytes(download.total_bytes)}
                  </span>
                </div>
                <div className="progress">
                  <div className="progress-fill" style={{ width: `${fraction * 100}%` }} />
                </div>
              </div>
            )
          })}
        </section>
      )}

      <section className="panel">
        <h2 className="panel-title">Installed</h2>
        {installed.data?.models.length === 0 && (
          <p className="faint">Nothing installed yet. Pick one from the list below.</p>
        )}
        <div className="model-rows">
          {installed.data?.models.map(({ model, loaded, fit }) => (
            <div key={model.id} className="model-row">
              <div className="model-row-main">
                <span className="model-row-name">{model.id}</span>
                <div className="model-row-tags">
                  {model.quant && <span className="tag">{model.quant}</span>}
                  {model.capabilities.map((capability) => (
                    <span key={capability} className="tag">
                      {capability}
                    </span>
                  ))}
                  {loaded && <span className="tag tag-live">loaded</span>}
                  {model.status === 'error' && <span className="tag tag-error">error</span>}
                </div>
                {model.error && <p className="form-error">{model.error}</p>}
              </div>

              <span className="faint mono model-row-size">{formatBytes(model.file_size_bytes)}</span>
              {fit && <FitBadge verdict={fit.verdict} label={fit.label} note={fit.note} />}

              <button
                className="btn btn-ghost btn-icon"
                aria-label={`Delete ${model.id}`}
                onClick={() => remove.mutate(model.id)}
              >
                <TrashIcon size={15} />
              </button>
            </div>
          ))}
        </div>
      </section>

      <section className="panel">
        <h2 className="panel-title">Recommended</h2>
        <p className="faint panel-note">
          Fit is estimated from this machine's memory and the model's size. It is a guide, not a
          benchmark.
        </p>
        <div className="model-cards">
          {recommended.data?.models.map((model) => (
            <div key={model.slug} className="model-card">
              <div className="model-card-head">
                <span className="model-card-name">{model.displayName}</span>
                <FitBadge verdict={model.fit.verdict} label={model.fit.label} note={model.fit.note} />
              </div>
              <p className="muted model-card-blurb">{model.blurb}</p>
              <div className="model-card-tags">
                <span className="tag">{model.paramCount}</span>
                <span className="tag">{model.defaultQuant}</span>
                <span className="tag">{formatBytes(model.approxSizeBytes)}</span>
                {model.capabilities.map((capability) => (
                  <span key={capability} className="tag">
                    {capability}
                  </span>
                ))}
              </div>
              <button
                className="btn model-card-action"
                disabled={model.installed || pull.isPending}
                onClick={() => pull.mutate(model.slug)}
              >
                {model.installed ? 'Installed' : 'Download'}
              </button>
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}

function FitBadge({
  verdict,
  label,
  note,
}: {
  verdict: FitVerdict
  label: string
  note: string
}) {
  return (
    <span className={`fit fit-${verdict}`} title={note}>
      {label}
    </span>
  )
}
