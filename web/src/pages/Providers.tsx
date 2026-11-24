import { useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime, type Provider, type ProviderPreset } from '../lib/api'
import { ProviderLogo } from '../components/BrandLogo'
import { ConfirmDialog } from '../components/ConfirmDialog'
import { HelpTip, TitleWithHelp } from '../components/HelpTip'
import {
  ArrowLeftIcon,
  CheckIcon,
  ChevronIcon,
  ExternalIcon,
  GiftIcon,
  KeyIcon,
  RefreshIcon,
  TrashIcon,
} from '../components/icons'
import { FreeDetail } from './Free'
import { Switch } from './Tools'

/**
 * Every model reached over the internet, in one place: Kuro Free, the providers
 * you hold a key for, and endpoints on hardware you rent. A provider rents you
 * their model per token; a cloud endpoint runs yours on a GPU you rent. Both
 * store a key and probe an OpenAI-compatible endpoint, so they share this page
 * and are told apart by their group.
 *
 * What the right side shows lives in the URL (`?show=`), so the header's Add
 * provider button and the old /free and /cloud links can point straight at it.
 */
export function ProvidersPane() {
  const [params, setParams] = useSearchParams()
  const providers = useQuery({ queryKey: ['providers'], queryFn: api.providers.list })
  const free = useQuery({ queryKey: ['free'], queryFn: api.free.overview })

  const connected = providers.data?.providers ?? []
  const presets = providers.data?.presets ?? []
  const show =
    params.get('show') ??
    (connected[0]?.id ?? ((free.data?.keyCount ?? 0) > 0 ? 'free' : 'add'))
  const select = (next: string) => setParams({ show: next }, { replace: true })

  const freeReady = (free.data?.availableCount ?? 0) > 0

  return (
    <div className="prov">
      <nav className="prov-list" aria-label="Providers">
        <div className="side-heading"><span>Pools</span></div>
        <button className={`side-row ${show === 'free' ? 'active' : ''}`} onClick={() => select('free')}>
          <GiftIcon size={15} />
          <span className="side-row-label">Kuro Free</span>
          <span className={`status-dot ${freeReady ? 'status-connected' : ''}`} />
        </button>

        <ProviderGroup title="Providers" items={connected.filter((p) => p.surface === 'provider')} show={show} onSelect={select} />
        <ProviderGroup title="Your Hardware" items={connected.filter((p) => p.surface === 'cloud')} show={show} onSelect={select} />

        {connected.length === 0 && !providers.isLoading && (
          <>
            <div className="side-heading"><span>Providers</span></div>
            <span className="faint prov-none">None connected yet</span>
          </>
        )}
        {providers.isError && <p className="form-error prov-none">Kuro did not answer.</p>}
      </nav>

      <div className="prov-detail">
        <Detail
          show={show}
          connected={connected}
          presets={presets}
          onSelect={select}
        />
      </div>
    </div>
  )
}

function ProviderGroup({
  title,
  items,
  show,
  onSelect,
}: {
  title: string
  items: Provider[]
  show: string
  onSelect: (id: string) => void
}) {
  if (items.length === 0) return null
  return (
    <>
      <div className="side-heading"><span>{title}</span></div>
      {items.map((provider) => (
        <button
          key={provider.id}
          className={`side-row ${show === provider.id ? 'active' : ''}`}
          onClick={() => onSelect(provider.id)}
        >
          <ProviderLogo slug={provider.provider} label={provider.label} size={18} bare />
          <span className="side-row-label">{provider.label}</span>
          <span className={`status-dot ${provider.enabled ? statusClass(provider.status) : ''}`} />
        </button>
      ))}
    </>
  )
}

function statusClass(status: Provider['status']): string {
  if (status === 'ok') return 'status-connected'
  if (status === 'error') return 'status-error'
  return ''
}

function Detail({
  show,
  connected,
  presets,
  onSelect,
}: {
  show: string
  connected: Provider[]
  presets: ProviderPreset[]
  onSelect: (id: string) => void
}) {
  if (show === 'free') return <FreeDetail />
  if (show === 'add') return <AddProviderGrid presets={presets} connected={connected} onPick={(slug) => onSelect(`preset:${slug}`)} />

  if (show.startsWith('preset:')) {
    const preset = presets.find((entry) => entry.slug === show.slice('preset:'.length))
    if (!preset) return <span className="faint">Loading…</span>
    return (
      <ConnectForm
        key={preset.slug}
        preset={preset}
        alreadyConnected={connected.some((provider) => provider.provider === preset.slug)}
        onBack={() => onSelect('add')}
        onDone={onSelect}
      />
    )
  }

  const provider = connected.find((entry) => entry.id === show)
  if (!provider) return <span className="faint">Not found</span>
  return <ProviderDetail provider={provider} onRemoved={() => onSelect('add')} />
}

/* ---------- One provider ---------- */

function ProviderDetail({ provider, onRemoved }: { provider: Provider; onRemoved: () => void }) {
  const queryClient = useQueryClient()
  const [editingKey, setEditingKey] = useState(false)
  const [key, setKey] = useState('')
  const [confirmRemove, setConfirmRemove] = useState(false)
  const [modelQuery, setModelQuery] = useState('')

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['providers'] })
    // The composer's picker reads these models from the models endpoint.
    void queryClient.invalidateQueries({ queryKey: ['models'] })
  }

  const test = useMutation({ mutationFn: () => api.providers.test(provider.id), onSuccess: refresh })
  const setEnabled = useMutation({
    mutationFn: (enabled: boolean) => api.providers.setEnabled(provider.id, enabled),
    onSuccess: refresh,
  })
  const replaceKey = useMutation({
    mutationFn: (next: string) => api.providers.replaceKey(provider.id, next),
    onSuccess: () => {
      setKey('')
      setEditingKey(false)
      refresh()
    },
  })
  const remove = useMutation({
    mutationFn: () => api.providers.remove(provider.id),
    onSuccess: () => {
      setConfirmRemove(false)
      refresh()
      onRemoved()
    },
  })

  const failure = test.error ?? setEnabled.error ?? replaceKey.error ?? remove.error
  const needle = modelQuery.trim().toLowerCase()
  const shown = needle ? provider.models.filter((model) => model.toLowerCase().includes(needle)) : provider.models

  return (
    <div className={`prov-pane ${provider.enabled ? '' : 'is-off'}`}>
      <div className="prov-title">
        <ProviderLogo slug={provider.provider} label={provider.label} size={36} />
        <div className="prov-title-text">
          <strong>{provider.label}</strong>
          <span className="faint mono truncate">{provider.base_url}</span>
        </div>
        <span className="spacer" />
        <Switch
          checked={provider.enabled}
          label={provider.enabled ? 'Hide from the model picker' : 'Show in the model picker'}
          onChange={(enabled) => setEnabled.mutate(enabled)}
        />
      </div>

      <div className="prov-status">
        <span className={`status-dot ${statusClass(provider.status)}`} />
        <span className="muted">{statusLine(provider)}</span>
        {!provider.hasKey && <span className="tag tag-warn">no key</span>}
      </div>
      {provider.status === 'error' && provider.last_error && <p className="form-error">{provider.last_error}</p>}
      {failure && <p className="form-error">{failure.message}</p>}

      <div className="inline-form">
        <button className="btn" onClick={() => test.mutate()} disabled={test.isPending}>
          {test.isPending ? <span className="spinner" /> : <RefreshIcon size={13} />}
          {test.isPending ? 'Checking…' : 'Check'}
        </button>
        <button className="btn" onClick={() => setEditingKey((open) => !open)}>
          <KeyIcon size={13} />
          Replace key
        </button>
        <span className="spacer" />
        <button className="btn" onClick={() => setConfirmRemove(true)}>
          <TrashIcon size={13} />
          Remove
        </button>
      </div>

      {editingKey && (
        <div className="inline-form">
          <input
            className="input"
            type="password"
            placeholder="New API key"
            autoFocus
            value={key}
            onChange={(event) => setKey(event.target.value)}
            onKeyDown={(event) => event.key === 'Enter' && key.trim() && replaceKey.mutate(key.trim())}
          />
          <button
            className="btn btn-solid"
            disabled={!key.trim() || replaceKey.isPending}
            onClick={() => replaceKey.mutate(key.trim())}
          >
            Save
          </button>
        </div>
      )}

      <div className="prov-models-head">
        <strong>Models</strong>
        <span className="faint">{provider.models.length}</span>
        <span className="spacer" />
        {provider.models.length > 12 && (
          <input
            className="input prov-filter"
            placeholder="Filter models"
            value={modelQuery}
            onChange={(event) => setModelQuery(event.target.value)}
          />
        )}
      </div>
      {provider.models.length === 0 ? (
        <span className="faint">None listed. Check to ask again.</span>
      ) : shown.length === 0 ? (
        <span className="faint">Nothing matched</span>
      ) : (
        <ul className="prov-models">
          {shown.map((model) => (
            <li key={model} className="mono truncate" title={model}>{model}</li>
          ))}
        </ul>
      )}

      {confirmRemove && (
        <ConfirmDialog
          title={`Remove ${provider.label}?`}
          body="Its key is deleted from this machine."
          confirmLabel="Remove"
          busy={remove.isPending}
          onConfirm={() => remove.mutate()}
          onCancel={() => setConfirmRemove(false)}
        />
      )}
    </div>
  )
}

function statusLine(provider: Provider): string {
  const state =
    provider.status === 'ok' ? 'Connected' : provider.status === 'error' ? 'Not answering' : 'Not checked yet'
  return provider.last_tested_at ? `${state} · checked ${relativeTime(provider.last_tested_at)}` : state
}

/* ---------- Adding one ---------- */

const KIND_GROUPS: { kind: ProviderPreset['kind']; title: string; help: string }[] = [
  { kind: 'aggregator', title: 'Many Models, One Key', help: 'One account reaches models from many makers.' },
  { kind: 'first_party', title: 'Model Makers', help: 'Straight from the company that trained the model.' },
  { kind: 'rented_gpu', title: 'Your Hardware', help: 'Run your own models on a GPU you rent. Start the server there first, then give Kuro its address.' },
  { kind: 'custom', title: 'Anything Else', help: 'Any endpoint that speaks the OpenAI API. The base URL usually ends in /v1.' },
]

function AddProviderGrid({
  presets,
  connected,
  onPick,
}: {
  presets: ProviderPreset[]
  connected: Provider[]
  onPick: (slug: string) => void
}) {
  const known = new Set(KIND_GROUPS.map((group) => group.kind))
  // Presets of a kind this build does not know still get listed.
  const groups = [
    ...KIND_GROUPS,
    { kind: 'other', title: 'More', help: '' },
  ].map((group) => ({
    ...group,
    members: presets.filter((preset) =>
      group.kind === 'other' ? !known.has(preset.kind) : preset.kind === group.kind,
    ),
  }))

  return (
    <div className="prov-pane">
      <strong className="prov-pane-title">Add provider</strong>
      {groups.map((group) =>
        group.members.length === 0 ? null : (
          <div key={group.kind} className="tile-group">
            <span className="tile-group-title muted">
              <TitleWithHelp title={group.title} help={group.help || undefined} />
            </span>
            <div className="tile-grid">
              {group.members.map((preset) => (
                <button key={preset.slug} className="tile" onClick={() => onPick(preset.slug)} title={preset.blurb}>
                  <ProviderLogo slug={preset.slug} label={preset.name} size={28} />
                  <span className="truncate">{preset.name}</span>
                  <span className="spacer" />
                  {connected.some((provider) => provider.provider === preset.slug) && (
                    <CheckIcon size={13} className="ok-text" />
                  )}
                  <ChevronIcon size={12} className="tile-chevron" />
                </button>
              ))}
            </div>
          </div>
        ),
      )}
    </div>
  )
}

function ConnectForm({
  preset,
  alreadyConnected,
  onBack,
  onDone,
}: {
  preset: ProviderPreset
  alreadyConnected: boolean
  onBack: () => void
  onDone: (id: string) => void
}) {
  const queryClient = useQueryClient()
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [label, setLabel] = useState('')

  const add = useMutation({
    mutationFn: () =>
      api.providers.add({
        provider: preset.slug,
        apiKey: apiKey.trim(),
        ...(preset.needs_url ? { baseUrl: baseUrl.trim() } : {}),
        ...(label.trim() ? { label: label.trim() } : {}),
      }),
    onSuccess: ({ provider }) => {
      void queryClient.invalidateQueries({ queryKey: ['providers'] })
      void queryClient.invalidateQueries({ queryKey: ['models'] })
      // Saved even when the endpoint did not answer: it is listed with the reason.
      onDone(provider.id)
    },
  })

  const canSubmit = apiKey.trim() !== '' && (!preset.needs_url || baseUrl.trim() !== '')
  const submit = () => canSubmit && !add.isPending && add.mutate()
  const urlHint =
    preset.surface === 'cloud' ? 'https://your-pod-8000.proxy.runpod.net/v1' : 'https://api.example.com/v1'

  return (
    <div className="prov-pane prov-form">
      <div className="prov-title">
        <button className="btn btn-ghost btn-icon" onClick={onBack} aria-label="Back" title="Back">
          <ArrowLeftIcon size={15} />
        </button>
        <ProviderLogo slug={preset.slug} label={preset.name} size={28} />
        <strong>{preset.name}</strong>
        <HelpTip text={preset.blurb} />
      </div>

      {preset.needs_url && (
        <label className="form-field">
          <span>Base URL</span>
          <input className="input mono" placeholder={urlHint} value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
        </label>
      )}
      <label className="form-field">
        <span>API key</span>
        <input
          className="input"
          type="password"
          autoFocus
          placeholder={preset.key_hint ?? 'Paste the key'}
          value={apiKey}
          onChange={(event) => setApiKey(event.target.value)}
          onKeyDown={(event) => event.key === 'Enter' && submit()}
        />
      </label>
      {alreadyConnected && (
        <label className="form-field">
          <span>Name</span>
          <input className="input" placeholder={`${preset.name} (second account)`} value={label} onChange={(event) => setLabel(event.target.value)} />
        </label>
      )}

      {add.error && <p className="form-error">{add.error.message}</p>}

      <div className="inline-form">
        {preset.credentials_url && (
          <a className="external-link faint" href={preset.credentials_url} target="_blank" rel="noopener noreferrer">
            Get a key
            <ExternalIcon size={11} />
          </a>
        )}
        <span className="spacer" />
        <button className="btn" onClick={onBack}>Cancel</button>
        <button className="btn btn-solid" onClick={submit} disabled={!canSubmit || add.isPending}>
          {add.isPending ? 'Checking…' : 'Connect'}
        </button>
      </div>
    </div>
  )
}
