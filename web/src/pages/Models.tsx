import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  api,
  formatBytes,
  formatCount,
  relativeTime,
  type FitVerdict,
  type HubModel,
} from '../lib/api'
import {
  CheckIcon,
  DownloadIcon,
  ExternalIcon,
  SearchIcon,
  TrashIcon,
} from '../components/icons'

type Source = 'recommended' | 'hub'

export function ModelsPage() {
  const queryClient = useQueryClient()
  const [reference, setReference] = useState('')
  const [pullError, setPullError] = useState<string | null>(null)
  const [source, setSource] = useState<Source>('recommended')

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
    void queryClient.invalidateQueries({ queryKey: ['hub'] })
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
        <h2 className="panel-title">Add by name</h2>
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
          run — use the search below if you do not already know what you want.
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
                  {model.quant && <span className="quant">{model.quant}</span>}
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

              <span className="faint mono model-row-size">
                {formatBytes(model.file_size_bytes)}
              </span>
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
        <div className="panel-head">
          <h2 className="panel-title">Find a model</h2>
          <div className="segmented">
            {(
              [
                ['recommended', 'Recommended'],
                ['hub', 'Hugging Face'],
              ] as const
            ).map(([value, label]) => (
              <button
                key={value}
                className={`segment ${source === value ? 'is-on' : ''}`}
                onClick={() => setSource(value)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        {source === 'recommended' ? (
          <>
            <p className="faint panel-note">
              A short curated list. Fit is estimated from this machine's memory and the model's
              size — a guide, not a benchmark.
            </p>
            <div className="model-cards">
              {recommended.data?.models.map((model) => (
                <div key={model.slug} className="model-card">
                  <div className="model-card-head">
                    <span className="model-card-name">{model.displayName}</span>
                    <FitBadge
                      verdict={model.fit.verdict}
                      label={model.fit.label}
                      note={model.fit.note}
                    />
                  </div>
                  <p className="muted model-card-blurb">{model.blurb}</p>
                  <div className="model-card-tags">
                    <span className="tag">{model.paramCount}</span>
                    <span className="quant">{model.defaultQuant}</span>
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
          </>
        ) : (
          <HubSearch onPull={(reference) => pull.mutate(reference)} pulling={pull.isPending} />
        )}
      </section>
    </div>
  )
}

/**
 * Search Hugging Face from inside Kuro.
 *
 * Filtered to GGUF, so nothing in the results is a model that cannot run here —
 * which is the main thing the Hub's own search will not do for you. Each result
 * lists the quantizations it publishes, smallest first, because that is the choice
 * that decides whether the model fits.
 */
function HubSearch({
  onPull,
  pulling,
}: {
  onPull: (reference: string) => void
  pulling: boolean
}) {
  const [query, setQuery] = useState('')
  const [submitted, setSubmitted] = useState('')

  const results = useQuery({
    queryKey: ['hub', submitted],
    queryFn: () => api.models.searchHub(submitted),
  })

  return (
    <>
      <form
        className="pull-form"
        onSubmit={(event) => {
          event.preventDefault()
          setSubmitted(query)
        }}
      >
        <input
          className="input"
          placeholder="qwen, llama, coding, small…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button className="btn btn-solid" disabled={results.isFetching}>
          {results.isFetching ? <span className="spinner" /> : <SearchIcon size={15} />}
          Search
        </button>
      </form>
      <p className="faint form-hint">
        Only repositories publishing GGUF weights, most downloaded first.{' '}
        <a
          className="external-link"
          href="https://huggingface.co/models?library=gguf&sort=downloads"
          target="_blank"
          rel="noopener noreferrer"
        >
          Browse on Hugging Face
          <ExternalIcon size={11} />
        </a>
      </p>

      {results.isError && (
        <p className="form-error">
          {results.error instanceof Error ? results.error.message : 'The search failed.'}
        </p>
      )}

      <div className="hub-rows">
        {results.data?.models.map((model) => (
          <HubRow key={model.repo} model={model} onPull={onPull} pulling={pulling} />
        ))}
        {results.data?.models.length === 0 && <p className="faint">Nothing matched.</p>}
      </div>
    </>
  )
}

function HubRow({
  model,
  onPull,
  pulling,
}: {
  model: HubModel
  onPull: (reference: string) => void
  pulling: boolean
}) {
  // Default to the smallest published quantization, which is the one most likely
  // to fit; the list is already ordered that way.
  const [quant, setQuant] = useState(model.quants[0] ?? '')

  const blocked = model.split_only || model.gated

  return (
    <div className="hub-row">
      <div className="hub-row-main">
        <div className="hub-row-head">
          <span className="hub-row-name">{model.name}</span>
          {model.owner && <span className="faint">{model.owner}</span>}
          {model.param_count && <span className="tag">{model.param_count}</span>}
          {model.installed && (
            <span className="tag tag-live">
              <CheckIcon size={10} /> installed
            </span>
          )}
          {model.gated && <span className="tag tag-warn">licence needed</span>}
          {model.split_only && <span className="tag tag-warn">split shards</span>}
        </div>

        <div className="hub-row-meta faint">
          <span>{formatCount(model.downloads)} downloads</span>
          {model.likes > 0 && <span>{formatCount(model.likes)} likes</span>}
          {model.last_modified && <span>updated {relativeTime(model.last_modified)}</span>}
          <a
            className="external-link"
            href={`https://huggingface.co/${model.repo}`}
            target="_blank"
            rel="noopener noreferrer"
          >
            Repository
            <ExternalIcon size={11} />
          </a>
        </div>

        {model.split_only && (
          <p className="faint">
            Published only as multi-part shards, which Kuro cannot load yet. Look for a smaller
            quantization in a single file.
          </p>
        )}
        {model.gated && (
          <p className="faint">
            Gated — accept its licence on Hugging Face first, then download by name above.
          </p>
        )}
      </div>

      {model.quants.length > 0 && (
        <div className="hub-row-quants">
          {model.quants.map((option) => (
            <button
              key={option}
              className={`quant quant-button ${option === quant ? 'is-on' : ''}`}
              onClick={() => setQuant(option)}
              title={`Download the ${option} build`}
            >
              {option}
            </button>
          ))}
        </div>
      )}

      {model.fit && (
        <FitBadge verdict={model.fit.verdict} label={model.fit.label} note={model.fit.note} />
      )}

      <button
        className="btn btn-sm"
        disabled={blocked || pulling}
        onClick={() => onPull(quant ? `${model.repo}:${quant}` : model.repo)}
      >
        <DownloadIcon size={13} />
        Download
      </button>
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
