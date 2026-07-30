import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  api,
  relativeTime,
  type Provider,
  type ProviderPreset,
  type Surface,
} from '../lib/api'
import { Switch } from './Tools'
import { CloudIcon, ExternalIcon, KeyIcon, RefreshIcon, TrashIcon } from '../components/icons'

/**
 * The shared implementation behind Providers and Cloud.
 *
 * Both screens do the same thing — store a key, probe an OpenAI-compatible
 * endpoint, list what it offers — and the reason they are two screens is that
 * they are two different decisions. Adding OpenAI is agreeing to pay a company
 * per token for their model. Adding a RunPod endpoint is running *your* model on
 * a GPU you rented, where the model, the quantisation and the context length are
 * all still your choices. The first is closer to signing up for a service; the
 * second is closer to running locally on a bigger machine.
 *
 * They shared a screen originally and it made the second look like a variant of
 * the first. Splitting the presentation while keeping one code path is the honest
 * arrangement: the mechanism really is identical, and saying so twice in two
 * components would be the actual duplication.
 */

const KIND_LABEL: Record<ProviderPreset['kind'], string> = {
  aggregator: 'Many models, one key',
  first_party: 'Direct from the model developer',
  rented_gpu: 'Hardware you rent',
  custom: 'Anything OpenAI-compatible',
}

export interface EndpointsPageProps {
  surface: Surface
  title: string
  intro: string
  /** The "how this fits" bullets, which differ between the two screens. */
  notes: string[]
  /** Placeholder for the base URL field, when the preset asks for one. */
  urlPlaceholder: string
}

export function EndpointsPage({
  surface,
  title,
  intro,
  notes,
  urlPlaceholder,
}: EndpointsPageProps) {
  const queryClient = useQueryClient()
  const endpoints = useQuery({ queryKey: ['providers'], queryFn: api.providers.list })

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['providers'] })
    // The composer's picker reads these models from the models endpoint.
    void queryClient.invalidateQueries({ queryKey: ['models'] })
  }

  // Both screens read the same endpoint and show their own half of it, so
  // connecting on one and looking at the other never disagrees.
  const connected = (endpoints.data?.providers ?? []).filter(
    (provider) => provider.surface === surface,
  )
  const presets = useMemo(
    () => (endpoints.data?.presets ?? []).filter((preset) => preset.surface === surface),
    [endpoints.data, surface],
  )

  const grouped = useMemo(() => {
    const byKind = new Map<ProviderPreset['kind'], ProviderPreset[]>()
    for (const preset of presets) {
      byKind.set(preset.kind, [...(byKind.get(preset.kind) ?? []), preset])
    }
    return [...byKind]
  }, [presets])

  return (
    <div className="page">
      <header className="page-head">
        <h1>{title}</h1>
        <p className="muted">{intro}</p>
      </header>

      {connected.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">
            <CloudIcon size={15} />
            Connected
          </h2>
          <div className="server-rows">
            {connected.map((provider) => (
              <EndpointRow key={provider.id} provider={provider} onChanged={refresh} />
            ))}
          </div>
        </section>
      )}

      {grouped.map(([kind, options]) => (
        <section key={kind} className="panel">
          <h2 className="panel-title">{KIND_LABEL[kind]}</h2>
          <div className="store-grid">
            {options.map((preset) => (
              <PresetCard
                key={preset.slug}
                preset={preset}
                urlPlaceholder={urlPlaceholder}
                connected={connected.some((provider) => provider.provider === preset.slug)}
                onAdded={refresh}
              />
            ))}
          </div>
        </section>
      ))}

      <section className="panel">
        <h2 className="panel-title">How this fits</h2>
        <ul className="prose-list muted">
          {notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      </section>
    </div>
  )
}

function PresetCard({
  preset,
  connected,
  urlPlaceholder,
  onAdded,
}: {
  preset: ProviderPreset
  connected: boolean
  urlPlaceholder: string
  onAdded: () => void
}) {
  const [open, setOpen] = useState(false)
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [label, setLabel] = useState('')
  const [error, setError] = useState<string | null>(null)

  const add = useMutation({
    mutationFn: () =>
      api.providers.add({
        provider: preset.slug,
        apiKey,
        ...(preset.needs_url ? { baseUrl } : {}),
        ...(label.trim() ? { label: label.trim() } : {}),
      }),
    onSuccess: (result) => {
      if (result.provider.status === 'error') {
        setError(result.provider.last_error ?? 'The endpoint did not answer.')
        onAdded()
        return
      }
      setApiKey('')
      setBaseUrl('')
      setLabel('')
      setOpen(false)
      onAdded()
    },
    onError: (caught: Error) => setError(caught.message),
  })

  const canSubmit = apiKey.trim().length > 0 && (!preset.needs_url || baseUrl.trim().length > 0)

  return (
    <div className={`store-card ${connected ? 'is-installed' : ''}`}>
      <div className="store-card-head">
        <span className="store-card-name">{preset.name}</span>
        {connected && <span className="tag tag-live">connected</span>}
      </div>

      <p className="muted store-card-blurb">{preset.blurb}</p>

      {open ? (
        <div className="preset-form">
          {preset.needs_url && (
            <label className="dialog-field">
              <span>
                Base URL <span className="required">*</span>
              </span>
              <input
                className="input mono"
                placeholder={urlPlaceholder}
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
              />
            </label>
          )}

          <label className="dialog-field">
            <span>
              API key <span className="required">*</span>
            </span>
            <input
              className="input"
              type="password"
              placeholder={preset.key_hint ?? 'Paste the key'}
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
            />
          </label>

          {connected && (
            <label className="dialog-field">
              <span>Name</span>
              <input
                className="input"
                placeholder={`${preset.name} (second account)`}
                value={label}
                onChange={(event) => setLabel(event.target.value)}
              />
            </label>
          )}

          {error && <p className="form-error">{error}</p>}

          <div className="preset-form-foot">
            <button
              className="btn btn-solid btn-sm"
              disabled={!canSubmit || add.isPending}
              onClick={() => {
                setError(null)
                add.mutate()
              }}
            >
              {add.isPending ? <span className="spinner" /> : <KeyIcon size={13} />}
              {add.isPending ? 'Checking…' : 'Connect'}
            </button>
            <button className="btn btn-ghost btn-sm" onClick={() => setOpen(false)}>
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="store-card-foot">
          <button className="btn btn-sm" onClick={() => setOpen(true)}>
            <CloudIcon size={13} />
            {connected ? 'Add another' : 'Connect'}
          </button>
          {preset.credentials_url && (
            <a
              className="external-link faint"
              href={preset.credentials_url}
              target="_blank"
              rel="noopener noreferrer"
            >
              Get a key
              <ExternalIcon size={11} />
            </a>
          )}
        </div>
      )}
    </div>
  )
}

function EndpointRow({ provider, onChanged }: { provider: Provider; onChanged: () => void }) {
  const [showModels, setShowModels] = useState(false)
  const [keyDraft, setKeyDraft] = useState('')
  const [showKey, setShowKey] = useState(false)

  const test = useMutation({
    mutationFn: () => api.providers.test(provider.id),
    onSuccess: onChanged,
  })

  const setEnabled = useMutation({
    mutationFn: (enabled: boolean) => api.providers.setEnabled(provider.id, enabled),
    onSuccess: onChanged,
  })

  const replaceKey = useMutation({
    mutationFn: (key: string) => api.providers.replaceKey(provider.id, key),
    onSuccess: () => {
      setKeyDraft('')
      setShowKey(false)
      onChanged()
    },
  })

  const remove = useMutation({
    mutationFn: () => api.providers.remove(provider.id),
    onSuccess: onChanged,
  })

  return (
    <div className={`server-row ${provider.enabled ? '' : 'is-off'}`}>
      <div className="server-row-main">
        <div className="server-row-head">
          <span
            className={`status-dot status-${provider.status === 'ok' ? 'connected' : provider.status}`}
          />
          <span className="server-row-name">{provider.label}</span>
          {provider.models.length > 0 && (
            <button className="tag tag-button" onClick={() => setShowModels((open) => !open)}>
              {provider.models.length} models
            </button>
          )}
          {!provider.hasKey && <span className="tag tag-warn">no key</span>}
          {provider.last_tested_at && (
            <span className="faint">checked {relativeTime(provider.last_tested_at)}</span>
          )}
        </div>

        <span className="faint mono server-row-address">{provider.base_url}</span>

        {provider.status === 'error' && provider.last_error && (
          <p className="form-error">{provider.last_error}</p>
        )}

        {showModels && (
          <div className="model-chip-list">
            {provider.models.map((model) => (
              <code key={model} className="mono model-chip">
                {model}
              </code>
            ))}
          </div>
        )}

        {showKey && (
          <div className="inline-form">
            <input
              className="input"
              type="password"
              placeholder="New API key"
              value={keyDraft}
              onChange={(event) => setKeyDraft(event.target.value)}
            />
            <button
              className="btn btn-solid btn-sm"
              disabled={!keyDraft.trim() || replaceKey.isPending}
              onClick={() => replaceKey.mutate(keyDraft)}
            >
              Save and check
            </button>
            <button className="btn btn-ghost btn-sm" onClick={() => setShowKey(false)}>
              Cancel
            </button>
          </div>
        )}
      </div>

      <div className="server-row-actions">
        <button
          className="btn btn-ghost btn-icon"
          aria-label={`Check ${provider.label}`}
          title="Check and refresh the model list"
          onClick={() => test.mutate()}
          disabled={test.isPending}
        >
          {test.isPending ? <span className="spinner" /> : <RefreshIcon size={14} />}
        </button>

        <button
          className="btn btn-ghost btn-icon"
          aria-label="Replace key"
          title="Replace the key"
          onClick={() => setShowKey((open) => !open)}
        >
          <KeyIcon size={14} />
        </button>

        <Switch
          checked={provider.enabled}
          label={`${provider.enabled ? 'Disable' : 'Enable'} ${provider.label}`}
          onChange={(enabled) => setEnabled.mutate(enabled)}
        />

        <button
          className="btn btn-ghost btn-icon"
          aria-label={`Remove ${provider.label}`}
          title="Remove, and delete its key"
          onClick={() => remove.mutate()}
        >
          <TrashIcon size={14} />
        </button>
      </div>
    </div>
  )
}
