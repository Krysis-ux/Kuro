import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { HubAvatar, ModelLogo } from '../components/BrandLogo'
import {
  api,
  formatBytes,
  formatCount,
  publisherOf,
  relativeTime,
  type FitVerdict,
  type HubModel,
  type RecommendedModel,
} from '../lib/api'
import {
  CheckIcon,
  DownloadIcon,
  ExternalIcon,
  HeartIcon,
  SearchIcon,
  TrashIcon,
} from '../components/icons'

type Source = 'recommended' | 'hub'

/** Where a repository's detail page lives. */
export function modelPath(repo: string): string {
  return `/models/${repo.split('/').map(encodeURIComponent).join('/')}`
}

/**
 * Start a download and refresh everything that shows what is installed.
 *
 * Shared with the detail page, which downloads the same way and needs the
 * same lists to notice.
 */
export function usePull(onDone?: () => void) {
  const queryClient = useQueryClient()
  const [error, setError] = useState<string | null>(null)

  const pull = useMutation({
    mutationFn: (model: string) => api.models.pull(model),
    onSuccess: () => {
      setError(null)
      onDone?.()
      for (const key of ['models', 'recommended', 'downloads', 'hub', 'hub-card']) {
        void queryClient.invalidateQueries({ queryKey: [key] })
      }
    },
    onError: (caught: Error) => setError(caught.message),
  })

  return { pull, error }
}

export function ModelsPage() {
  const queryClient = useQueryClient()
  const [reference, setReference] = useState('')
  const [source, setSource] = useState<Source>('recommended')
  // Which model the user has asked to delete, pending confirmation. Deleting
  // weights is one of the few things in Kuro with nothing behind it.
  const [pendingDelete, setPendingDelete] = useState<{ id: string; bytes: number } | null>(
    null,
  )

  const installed = useQuery({ queryKey: ['models'], queryFn: api.models.list })
  const recommended = useQuery({ queryKey: ['recommended'], queryFn: api.models.recommended })
  const { pull, error: pullError } = usePull(() => setReference(''))

  const remove = useMutation({
    mutationFn: (id: string) => api.models.remove(id),
    onSuccess: () => {
      setPendingDelete(null)
      void queryClient.invalidateQueries({ queryKey: ['models'] })
      void queryClient.invalidateQueries({ queryKey: ['recommended'] })
      void queryClient.invalidateQueries({ queryKey: ['hub'] })
    },
  })

  const models = installed.data?.models ?? []
  const totalBytes = models.reduce((sum, { model }) => sum + (model.file_size_bytes ?? 0), 0)

  return (
    <div className="page">
      <header className="page-head">
        <h1>Models</h1>
        <p className="muted">Weights are stored on this machine and never leave it.</p>
      </header>

      <DownloadsPanel />

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Your library</h2>
          {models.length > 0 && (
            <span className="faint panel-meta">
              {models.length} {models.length === 1 ? 'model' : 'models'} ·{' '}
              {formatBytes(totalBytes)}
            </span>
          )}
        </div>

        {installed.isFetched && models.length === 0 && (
          <p className="faint">Nothing installed yet. Pick one from the list below.</p>
        )}

        <div className="library">
          {models.map(({ model, loaded, fit }) => {
            const owner = publisherOf(model.id)
            const title = model.display_name || model.id
            return (
              <div key={model.id} className="library-row">
                <ModelLogo name={model.hf_repo ?? model.id} owner={owner} size={40} />

                <div className="library-row-main">
                  {model.hf_repo ? (
                    <Link className="library-row-name" to={modelPath(model.hf_repo)}>
                      {title}
                    </Link>
                  ) : (
                    <span className="library-row-name">{title}</span>
                  )}
                  <div className="library-row-tags">
                    {model.quant && <span className="quant">{model.quant}</span>}
                    {model.param_count && <span className="tag">{model.param_count}</span>}
                    {model.capabilities.map((capability) => (
                      <span key={capability} className="tag">
                        {capability}
                      </span>
                    ))}
                    {loaded && <span className="tag tag-live">loaded</span>}
                    {model.status === 'error' && <span className="tag tag-error">error</span>}
                    {owner && <span className="faint library-row-owner">via {owner}</span>}
                  </div>
                  {model.error && <p className="form-error">{model.error}</p>}
                </div>

                <span className="faint mono library-row-size">
                  {formatBytes(model.file_size_bytes)}
                </span>
                {fit && <FitBadge verdict={fit.verdict} label={fit.label} note={fit.note} />}

                <button
                  className="btn btn-ghost btn-icon"
                  aria-label={`Delete ${model.id}`}
                  onClick={() =>
                    setPendingDelete({ id: model.id, bytes: model.file_size_bytes ?? 0 })
                  }
                >
                  <TrashIcon size={15} />
                </button>
              </div>
            )
          })}
        </div>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">Discover</h2>
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
              Grouped by what each one is <em>for</em>, because that is the question somebody
              actually has. Fit is estimated from this machine's memory and the model's size —
              a guide, not a benchmark, and it says nothing about whether a model is any good
              at the thing you want.
            </p>

            {(recommended.data?.purposes ?? []).map((purpose) => {
              const forThis = (recommended.data?.models ?? []).filter((model) =>
                model.purposes.includes(purpose.id),
              )
              if (forThis.length === 0) return null

              return (
                <div key={purpose.id} className="purpose-group">
                  <div className="purpose-head">
                    <h3 className="purpose-title">{purpose.label}</h3>
                    <p className="faint purpose-blurb">{purpose.blurb}</p>
                  </div>
                  <div className="model-cards">
                    {forThis.map((model) => (
                      <RecommendedCard
                        key={model.slug}
                        model={model}
                        pulling={pull.isPending}
                        onPull={() => pull.mutate(model.slug)}
                      />
                    ))}
                  </div>
                </div>
              )
            })}
          </>
        ) : (
          <HubSearch onPull={(repo) => pull.mutate(repo)} pulling={pull.isPending} />
        )}
        {pullError && <p className="form-error">{pullError}</p>}
      </section>

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
            placeholder="owner/repository  ·  or a Hugging Face URL"
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
          run — or open any model above to see every quantization it publishes.
        </p>
      </section>

      {pendingDelete && (
        <ConfirmDialog
          title="Delete this model?"
          confirmLabel="Delete"
          busy={remove.isPending}
          onCancel={() => setPendingDelete(null)}
          onConfirm={() => remove.mutate(pendingDelete.id)}
          body={
            <>
              <p>
                <code className="mono">{pendingDelete.id}</code> will be removed from this
                computer, freeing {formatBytes(pendingDelete.bytes)}.
              </p>
              <p className="faint">
                The weights are deleted from disk. Getting it back means downloading it again.
              </p>
            </>
          }
        />
      )}
    </div>
  )
}

/** Downloads in flight, polled often enough for the bars to feel live. */
export function DownloadsPanel() {
  const queryClient = useQueryClient()
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

  const active =
    downloads.data?.downloads.filter(
      (download) => download.status === 'downloading' || download.status === 'queued',
    ) ?? []

  // A finished download changes what is installed, so refresh once the last one lands.
  const activeCount = active.length
  const wasActive = useRef(false)
  useEffect(() => {
    if (wasActive.current && activeCount === 0) {
      for (const key of ['models', 'recommended', 'hub', 'hub-card']) {
        void queryClient.invalidateQueries({ queryKey: [key] })
      }
    }
    wasActive.current = activeCount > 0
  }, [activeCount, queryClient])

  if (active.length === 0) return null

  return (
    <section className="panel">
      <h2 className="panel-title">Downloading</h2>
      {active.map((download) => {
        const fraction =
          download.total_bytes && download.total_bytes > 0
            ? download.downloaded_bytes / download.total_bytes
            : 0
        return (
          <div key={download.id} className="download">
            <ModelLogo name={download.label} size={28} />
            <div className="download-main">
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
          </div>
        )
      })}
    </section>
  )
}

function RecommendedCard({
  model,
  pulling,
  onPull,
}: {
  model: RecommendedModel
  pulling: boolean
  onPull: () => void
}) {
  return (
    <article className="model-card">
      <Link className="card-link" to={modelPath(model.repo)} aria-label={`About ${model.displayName}`} />
      <div className="model-card-head">
        <ModelLogo name={model.repo} size={40} />
        <div className="model-card-title">
          <span className="model-card-name">{model.displayName}</span>
          <span className="faint model-card-sub">
            {model.paramCount} · {formatBytes(model.approxSizeBytes)}
          </span>
        </div>
      </div>
      <p className="muted model-card-blurb">{model.blurb}</p>
      <div className="model-card-tags">
        <span className="quant">{model.defaultQuant}</span>
        {model.capabilities.map((capability) => (
          <span key={capability} className="tag">
            {capability}
          </span>
        ))}
      </div>
      <div className="model-card-foot">
        <FitBadge verdict={model.fit.verdict} label={model.fit.label} note={model.fit.note} />
        <button
          className="btn btn-sm card-action"
          disabled={model.installed || pulling}
          onClick={onPull}
        >
          {model.installed ? <CheckIcon size={13} /> : <DownloadIcon size={13} />}
          {model.installed ? 'Installed' : 'Download'}
        </button>
      </div>
    </article>
  )
}

/**
 * Search Hugging Face from inside Kuro.
 *
 * Filtered to GGUF, so nothing in the results is a model that cannot run here —
 * which is the main thing the Hub's own search will not do for you. Opening a
 * result shows every quantization with its real size and fit.
 */
function HubSearch({
  onPull,
  pulling,
}: {
  onPull: (repo: string) => void
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
          placeholder="Search Hugging Face…"
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

      <div className="hub-grid">
        {results.data?.models.map((model) => (
          <HubCard key={model.repo} model={model} onPull={onPull} pulling={pulling} />
        ))}
        {results.data?.models.length === 0 && <p className="faint">Nothing matched.</p>}
      </div>
    </>
  )
}

function HubCard({
  model,
  onPull,
  pulling,
}: {
  model: HubModel
  onPull: (repo: string) => void
  pulling: boolean
}) {
  const blocked = model.split_only || model.gated

  return (
    <article className="model-card hub-card-item">
      <Link className="card-link" to={modelPath(model.repo)} aria-label={`About ${model.repo}`} />
      <div className="model-card-head">
        <ModelLogo name={model.name} owner={model.owner} size={40} />
        <div className="model-card-title">
          <span className="model-card-name">{model.name}</span>
          {model.owner && (
            <span className="owner-line faint">
              <HubAvatar owner={model.owner} size={14} />
              {model.owner}
            </span>
          )}
        </div>
      </div>

      <div className="hub-card-stats faint">
        <span>
          <DownloadIcon size={12} />
          {formatCount(model.downloads)}
        </span>
        {model.likes > 0 && (
          <span>
            <HeartIcon size={12} />
            {formatCount(model.likes)}
          </span>
        )}
        {model.last_modified && <span>{relativeTime(model.last_modified)}</span>}
      </div>

      <div className="model-card-tags">
        {model.param_count && <span className="tag">{model.param_count}</span>}
        {model.quants.length > 0 && (
          <span className="tag">
            {model.quants.length} {model.quants.length === 1 ? 'quant' : 'quants'}
          </span>
        )}
        {model.installed && (
          <span className="tag tag-live">
            <CheckIcon size={10} /> installed
          </span>
        )}
        {model.gated && <span className="tag tag-warn">licence needed</span>}
        {model.split_only && <span className="tag tag-warn">split shards</span>}
      </div>

      <div className="model-card-foot">
        {model.fit ? (
          <FitBadge verdict={model.fit.verdict} label={model.fit.label} note={model.fit.note} />
        ) : (
          <span />
        )}
        <button
          className="btn btn-sm card-action"
          disabled={blocked || pulling}
          title={blocked ? 'Open it to see why this one cannot be downloaded' : undefined}
          onClick={() => onPull(model.repo)}
        >
          <DownloadIcon size={13} />
          Download
        </button>
      </div>
    </article>
  )
}

export function FitBadge({
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
