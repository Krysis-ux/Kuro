import { Link, useNavigate, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { HubAvatar, ModelLogo } from '../components/BrandLogo'
import { HubMarkdown } from '../components/HubMarkdown'
import {
  api,
  formatBytes,
  formatCount,
  relativeTime,
  type HubFile,
  type HubModelCard,
  type HubPaper,
} from '../lib/api'
import {
  CheckIcon,
  DownloadIcon,
  ExternalIcon,
  FileIcon,
  HeartIcon,
  InfoIcon,
} from '../components/icons'
import { DownloadsPanel, FitBadge, modelPath, usePull } from './Models'

const HUB = 'https://huggingface.co'

/**
 * One Hugging Face repository, with everything the Hub shows about it.
 *
 * The question this page answers first is "which file do I download", so the
 * quantizations lead — each with its real size and how it fits this machine —
 * and the model card, papers and specs follow. Works for repositories without
 * GGUF weights too, which is how a quantized model's original is shown.
 */
export function ModelDetailPage() {
  const { owner = '', name = '' } = useParams()
  const repo = `${owner}/${name}`
  const navigate = useNavigate()

  const card = useQuery({
    queryKey: ['hub-card', repo],
    queryFn: () => api.models.details(repo),
    staleTime: 5 * 60_000,
  })
  const { pull, error: pullError } = usePull()

  return (
    <div className="page page-wide">
      <div className="detail">
        <button className="back-link" onClick={() => navigate(-1)}>
          ← Back
        </button>

        <Hero repo={repo} owner={owner} name={name} card={card.data} />

        <DownloadsPanel />

        {card.isError && (
          <section className="panel">
            <p className="form-error">
              {card.error instanceof Error ? card.error.message : 'The model could not be read.'}
            </p>
            <a className="external-link" href={`${HUB}/${repo}`} target="_blank" rel="noopener noreferrer">
              Open on Hugging Face <ExternalIcon size={11} />
            </a>
          </section>
        )}

        {card.isPending && <DetailSkeleton />}

        {card.data && (
          <>
            {card.data.gated && (
              <p className="detail-banner">
                Gated — accept its licence on Hugging Face before downloading.
              </p>
            )}
            {card.data.base_models.length > 0 && <BaseModels repos={card.data.base_models} />}

            <div className="detail-grid">
              <div className="detail-aside">
                <FilesPanel
                  card={card.data}
                  pulling={pull.isPending}
                  error={pullError}
                  onPull={(file) => pull.mutate(`${repo}:${file.quant ?? ''}`)}
                />
                <SpecsPanel card={card.data} />
                {card.data.papers.length > 0 && <PapersPanel papers={card.data.papers} />}
                <ExtrasPanel card={card.data} />
              </div>

              <section className="panel detail-main">
                <h2 className="panel-title">
                  <FileIcon size={14} />
                  Model card
                </h2>
                {card.data.readme?.trim() ? (
                  <HubMarkdown repo={card.data.repo} markdown={card.data.readme} />
                ) : (
                  <p className="faint">This repository has no model card.</p>
                )}
              </section>
            </div>
          </>
        )}
      </div>
    </div>
  )
}

function Hero({
  repo,
  owner,
  name,
  card,
}: {
  repo: string
  owner: string
  name: string
  card: HubModelCard | undefined
}) {
  const task = card?.pipeline_tag?.replace(/-/g, ' ')

  return (
    <header className="detail-hero">
      <ModelLogo name={name} owner={owner} size={64} className="detail-hero-logo" />
      <div className="detail-hero-main">
        <a className="owner-line" href={`${HUB}/${owner}`} target="_blank" rel="noopener noreferrer">
          <HubAvatar owner={owner} size={18} />
          {owner}
        </a>
        <h1>{name}</h1>
        {card && (
          <div className="detail-stats">
            <span title="Downloads in the last month">
              <DownloadIcon size={13} />
              {formatCount(card.downloads)}
            </span>
            <span>
              <HeartIcon size={13} />
              {formatCount(card.likes)}
            </span>
            {card.last_modified && <span>Updated {relativeTime(card.last_modified)}</span>}
            {task && <span className="tag">{task}</span>}
            {card.license && <span className="tag">{card.license}</span>}
          </div>
        )}
      </div>
      <a className="btn" href={`${HUB}/${repo}`} target="_blank" rel="noopener noreferrer">
        Hugging Face
        <ExternalIcon size={12} />
      </a>
    </header>
  )
}

/** The model this one was quantized or fine-tuned from, which usually has the fuller card. */
function BaseModels({ repos }: { repos: string[] }) {
  return (
    <div className="base-models">
      <span className="faint">Built from</span>
      {repos.map((base) => {
        const [baseOwner = '', baseName = base] = base.split('/')
        return (
          <Link key={base} className="base-model" to={modelPath(base)}>
            <ModelLogo name={baseName} owner={baseOwner} size={22} />
            <span>{base}</span>
            <span className="faint">→</span>
          </Link>
        )
      })}
    </div>
  )
}

function FilesPanel({
  card,
  pulling,
  error,
  onPull,
}: {
  card: HubModelCard
  pulling: boolean
  error: string | null
  onPull: (file: HubFile) => void
}) {
  // The largest file that still runs comfortably is the one most people want;
  // one that merely fits is only offered when nothing runs comfortably.
  const largestFirst = [...card.files].reverse()
  const best =
    largestFirst.find((file) => file.fit.verdict === 'great') ??
    largestFirst.find((file) => file.fit.verdict === 'fits')

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">
          <DownloadIcon size={14} />
          Quantizations
        </h2>
        {card.files.length > 0 && <span className="faint panel-meta">{card.files.length} files</span>}
      </div>

      {card.files.length === 0 ? (
        <p className="faint">
          No single-file GGUF weights here
          {card.base_models.length === 0 ? ' — this is the original model.' : '.'} Search for a
          GGUF build of it to run it in Kuro.
        </p>
      ) : (
        <ul className="quant-list">
          {card.files.map((file) => (
            <li
              key={file.filename}
              className={`quant-row ${file === best ? 'is-best' : ''}`}
              title={file.filename}
            >
              <div className="quant-row-main">
                <span className="quant">{file.quant ?? 'GGUF'}</span>
                <span className="faint">{bitsLabel(file.quant)}</span>
                {file === best && <span className="tag tag-live">best fit</span>}
              </div>
              <span className="mono quant-row-size">{formatBytes(file.size)}</span>
              <FitBadge verdict={file.fit.verdict} label={file.fit.label} note={file.fit.note} />
              <button
                className="btn btn-sm btn-icon"
                aria-label={file.installed ? `${file.quant} is installed` : `Download ${file.quant}`}
                title={file.installed ? 'Installed' : `Download ${formatBytes(file.size)}`}
                disabled={file.installed || pulling || card.gated || !file.quant}
                onClick={() => onPull(file)}
              >
                {file.installed ? <CheckIcon size={14} /> : <DownloadIcon size={14} />}
              </button>
            </li>
          ))}
        </ul>
      )}

      {card.split_quants.length > 0 && (
        <p className="faint panel-foot-note">
          Also published as split shards, which Kuro cannot load yet:{' '}
          {card.split_quants.join(', ')}.
        </p>
      )}
      {card.has_projector && (
        <p className="faint panel-foot-note">
          Ships a vision projector (<code className="mono">mmproj</code>) for image input.
        </p>
      )}
      {error && <p className="form-error">{error}</p>}
    </section>
  )
}

function SpecsPanel({ card }: { card: HubModelCard }) {
  const rows: [string, string | null | undefined][] = [
    ['Parameters', card.gguf?.total ? formatParams(card.gguf.total) : null],
    ['Architecture', card.gguf?.architecture],
    [
      'Context',
      card.gguf?.context_length ? `${formatCount(card.gguf.context_length)} tokens` : null,
    ],
    ['Task', card.pipeline_tag?.replace(/-/g, ' ')],
    ['Library', card.library_name],
    ['License', card.license],
    ['Languages', card.languages.length > 0 ? card.languages.slice(0, 8).join(', ') : null],
    ['Published', card.created_at ? new Date(card.created_at).toLocaleDateString() : null],
  ]
  const shown = rows.filter(([, value]) => value)

  if (shown.length === 0) return null

  return (
    <section className="panel">
      <h2 className="panel-title">
        <InfoIcon size={14} />
        At a glance
      </h2>
      <dl className="spec-list">
        {shown.map(([label, value]) => (
          <div key={label} className="spec-row">
            <dt className="faint">{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

function PapersPanel({ papers }: { papers: HubPaper[] }) {
  return (
    <section className="panel">
      <h2 className="panel-title">
        <FileIcon size={14} />
        Research
      </h2>
      <div className="papers">
        {papers.map((paper) => (
          <article key={paper.id} className="paper">
            <a
              className="paper-title"
              href={`${HUB}/papers/${paper.id}`}
              target="_blank"
              rel="noopener noreferrer"
            >
              {paper.title ?? `arXiv ${paper.id}`}
            </a>
            <div className="paper-meta faint">
              {paper.authors.length > 0 && (
                <span>
                  {paper.authors.slice(0, 3).join(', ')}
                  {paper.authors.length > 3 ? ' et al.' : ''}
                </span>
              )}
              {paper.published_at && <span>{new Date(paper.published_at).getFullYear()}</span>}
              {paper.upvotes ? <span>▲ {paper.upvotes}</span> : null}
            </div>
            {paper.summary && <p className="muted paper-summary">{paper.summary}</p>}
            <div className="paper-links">
              <a
                className="external-link"
                href={`https://arxiv.org/abs/${paper.id}`}
                target="_blank"
                rel="noopener noreferrer"
              >
                arXiv <ExternalIcon size={11} />
              </a>
              <a
                className="external-link"
                href={`https://arxiv.org/pdf/${paper.id}`}
                target="_blank"
                rel="noopener noreferrer"
              >
                PDF <ExternalIcon size={11} />
              </a>
            </div>
          </article>
        ))}
      </div>
    </section>
  )
}

/** Datasets, Spaces, tags and the chat template: the Hub's sidebar, condensed. */
function ExtrasPanel({ card }: { card: HubModelCard }) {
  const template = card.gguf?.chat_template
  if (card.datasets.length + card.spaces.length + card.tags.length === 0 && !template) return null

  return (
    <section className="panel extras">
      {card.datasets.length > 0 && (
        <ChipGroup label="Trained on" items={card.datasets} href={(id) => `${HUB}/datasets/${id}`} />
      )}
      {card.spaces.length > 0 && (
        <ChipGroup label="Spaces using it" items={card.spaces} href={(id) => `${HUB}/spaces/${id}`} />
      )}
      {card.tags.length > 0 && <ChipGroup label="Tags" items={card.tags} />}
      {template && (
        <details className="chat-template">
          <summary className="faint">Chat template</summary>
          <pre className="mono">{template}</pre>
        </details>
      )}
    </section>
  )
}

function ChipGroup({
  label,
  items,
  href,
}: {
  label: string
  items: string[]
  href?: (item: string) => string
}) {
  return (
    <div className="chip-group">
      <span className="chip-group-label faint">{label}</span>
      <div className="chips">
        {items.map((item) =>
          href ? (
            <a key={item} className="tag tag-link" href={href(item)} target="_blank" rel="noopener noreferrer">
              {item}
            </a>
          ) : (
            <span key={item} className="tag">
              {item}
            </span>
          ),
        )}
      </div>
    </div>
  )
}

function DetailSkeleton() {
  return (
    <div className="detail-grid" aria-busy="true">
      <div className="detail-aside">
        <div className="panel skeleton" style={{ height: 260 }} />
        <div className="panel skeleton" style={{ height: 180 }} />
      </div>
      <div className="panel skeleton detail-main" style={{ height: 520 }} />
    </div>
  )
}

/** `Q4_K_M` → `4-bit`, `BF16` → `16-bit`. */
function bitsLabel(quant: string | null): string {
  const bits = quant?.match(/\d+/)?.[0]
  return bits ? `${bits}-bit` : ''
}

/** `8190735360` → `8.2B`. */
function formatParams(total: number): string {
  if (total >= 1e9) return `${(total / 1e9).toFixed(1).replace(/\.0$/, '')}B`
  return `${Math.round(total / 1e6)}M`
}
