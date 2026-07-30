import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime, type Provider, type ProviderPreset } from '../lib/api'
import { Switch } from './Tools'
import {
  CloudIcon,
  ExternalIcon,
  KeyIcon,
  RefreshIcon,
  TrashIcon,
} from '../components/icons'

const KIND_LABEL: Record<ProviderPreset['kind'], string> = {
  aggregator: 'Many models, one key',
  first_party: 'Direct from the model developer',
  rented_gpu: 'Hardware you rented',
  custom: 'Anything OpenAI-compatible',
}

/**
 * Providers.
 *
 * Kuro's argument is that models should run on your machine. This page is the
 * honest exception: sometimes the machine cannot, and the alternative to
 * supporting that is a user keeping a second application open.
 *
 * The framing throughout is "your account, your key, your bill". There is nothing
 * hosted by Kuro here, and the page says so rather than leaving room for the
 * assumption.
 */
export function ProvidersPage() {
  const queryClient = useQueryClient()
  const providers = useQuery({ queryKey: ['providers'], queryFn: api.providers.list })

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['providers'] })
    // The composer's picker reads provider models from the models endpoint.
    void queryClient.invalidateQueries({ queryKey: ['models'] })
  }

  const connected = providers.data?.providers ?? []
  const presets = providers.data?.presets ?? []

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
        <h1>Providers</h1>
        <p className="muted">
          Talk to a model you do not run yourself. Your account, your key, your bill — the request
          goes straight from this machine to the provider, and Kuro is not in the middle of it.
        </p>
      </header>

      {connected.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">Connected</h2>
          <div className="server-rows">
            {connected.map((provider) => (
              <ProviderRow key={provider.id} provider={provider} onChanged={refresh} />
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
          <li>
            Provider models appear in the same picker as local ones, marked as leaving this machine.
            Local models stay at the top of the list.
          </li>
          <li>
            Everything else works identically: conversations, the effort control, web search, MCP
            tools, the request inspector.
          </li>
          <li>
            Keys are written to an owner-only file next to the database, never into it, and are never
            sent back to this page once saved.
          </li>
          <li>
            Kuro speaks one wire format — the OpenAI API. Anthropic is reached through its
            compatibility endpoint, which is why it needs no special handling.
          </li>
        </ul>
      </section>
    </div>
  )
}

function PresetCard({
  preset,
  connected,
  onAdded,
}: {
  preset: ProviderPreset
  connected: boolean
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
        setError(result.provider.last_error ?? 'The provider did not answer.')
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
                placeholder="https://your-pod-8000.proxy.runpod.net/v1"
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

function ProviderRow({ provider, onChanged }: { provider: Provider; onChanged: () => void }) {
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
          <span className={`status-dot status-${provider.status === 'ok' ? 'connected' : provider.status}`} />
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
