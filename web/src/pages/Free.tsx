import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ProviderLogo } from '../components/BrandLogo'
import { HelpTip, TitleWithHelp } from '../components/HelpTip'
import { Switch } from './Tools'
import {
  api,
  type FreeProvider,
  type FreeProviderUsage,
  type FreeUsage,
} from '../lib/api'
import { ExternalIcon, GiftIcon, RefreshIcon, TrashIcon } from '../components/icons'

/**
 * Kuro Free: your own free-tier keys pooled into one model, which sends each
 * request to whichever provider still has allowance. Shown inside Providers.
 *
 * Kuro supplies no keys; nothing works until you add one of your own. Shared
 * endpoints are listed last because they are tried last: the order on screen is
 * the order requests are routed in.
 */
export function FreeDetail() {
  const queryClient = useQueryClient()
  const overview = useQuery({ queryKey: ['free'], queryFn: api.free.overview })
  // Apart from the overview, which is re-read after every key edit: this is the
  // one request whose cost grows with the message table.
  const usage = useQuery({ queryKey: ['free', 'usage'], queryFn: api.free.usage })

  const setKeyless = useMutation({
    mutationFn: (enabled: boolean) => api.free.setKeyless(enabled),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['free'] })
      void queryClient.invalidateQueries({ queryKey: ['models'] })
    },
  })

  const held = overview.data
  const providers = held?.providers ?? []
  const shared = providers.filter((provider) => provider.keyless)
  const owned = providers.filter((provider) => !provider.keyless)
  const withKeys = owned.filter((provider) => provider.hasKey)
  const without = owned.filter((provider) => !provider.hasKey)

  const statusLine = !held
    ? 'Loading…'
    : held.keyCount === 0
      ? 'Add a free key to turn it on.'
      : `${held.availableCount} of ${held.keyCount} ${held.keyCount === 1 ? 'key' : 'keys'} working`

  return (
    <div className="prov-pane">
      <div className="prov-title">
        <span className="icon-tile icon-tile-lg">
          <GiftIcon size={17} />
        </span>
        <div className="prov-title-text">
          <span className="title-help">
            <strong>Kuro Free</strong>
            <HelpTip text="Kuro supplies no keys. Each one is yours, on a free tier, in your own name. Requests leave this machine, and free tiers are the likeliest to train on them: keep private work local." />
          </span>
          <span className="muted">{statusLine}</span>
        </div>
      </div>

      {held && held.keyCount > 0 && (
        <div className="free-flavours">
          {held.flavours.map((flavour) => (
            <div key={flavour.id} className="row" title={flavour.blurb}>
              <code className="mono">{flavour.id}</code>
              <span className={`tag ${flavour.available ? 'tag-live' : ''}`}>
                {flavour.available ? 'ready' : 'no provider'}
              </span>
            </div>
          ))}
        </div>
      )}

      {overview.isError && <p className="form-error">The free-provider list could not be read.</p>}

      {withKeys.length > 0 && (
        <FreeGroup title="Your Keys">
          {withKeys.map((provider) => (
            <ProviderRow key={provider.slug} provider={provider} />
          ))}
        </FreeGroup>
      )}

      <FreeGroup
        title={withKeys.length > 0 ? 'Add More' : 'Add a Free Key'}
        help="Each is free to sign up for and needs no card. More keys make the pool sturdier: when one runs out, the next answers."
      >
        {without.map((provider) => (
          <ProviderRow key={provider.slug} provider={provider} />
        ))}
      </FreeGroup>

      {shared.length > 0 && (
        <FreeGroup title="Shared Endpoints">
          <div className="row">
            <TitleWithHelp
              title="Use as a last resort"
              help="These answer with no account, so Kuro Free works before you sign up for anything. They are rate limited, often busy, and several keep or train on what they are sent."
            />
            <Switch
              checked={held?.allowKeyless ?? true}
              label="Use shared endpoints as a last resort"
              onChange={(enabled) => setKeyless.mutate(enabled)}
            />
          </div>
          {shared.map((provider) => (
            <ProviderRow key={provider.slug} provider={provider} />
          ))}
        </FreeGroup>
      )}

      <UsagePanel usage={usage.data} />
    </div>
  )
}

function FreeGroup({ title, help, children }: { title: string; help?: string; children: React.ReactNode }) {
  return (
    <section className="free-group">
      <span className="tile-group-title muted">
        <TitleWithHelp title={title} help={help} />
      </span>
      <div className="sunken-card">{children}</div>
    </section>
  )
}

/**
 * What the user says this provider's monthly allowance is.
 *
 * Asked for rather than discovered, because it cannot be discovered: the
 * providers state these in requests per minute, tokens per day, neurons and
 * dollars of credit, none of them expose it over the API, and all of them
 * change it without notice. A number Kuro guessed at would be worse than no
 * number, because it would look measured.
 */
function LimitField({ provider }: { provider: FreeProvider }) {
  const queryClient = useQueryClient()
  const stored = provider.limit?.tokensPerMonth
  const [draft, setDraft] = useState(stored ? String(stored) : '')

  const save = useMutation({
    mutationFn: (value: string) => {
      const parsed = Number(value.replace(/[,\s_]/g, ''))
      return Number.isFinite(parsed) && parsed > 0
        ? api.free.setLimit(provider.slug, { tokensPerMonth: parsed })
        : api.free.clearLimit(provider.slug)
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['free'] })
      void queryClient.invalidateQueries({ queryKey: ['free', 'usage'] })
    },
  })

  return (
    <label className="limit-field">
      <span className="faint">Monthly limit</span>
      <input
        className="input limit-input"
        inputMode="numeric"
        placeholder="tokens"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => {
          if (draft !== (stored ? String(stored) : '')) save.mutate(draft)
        }}
      />
    </label>
  )
}

/** Thousands separated, because these numbers get long fast. */
function count(value: number): string {
  return value.toLocaleString()
}

/**
 * What the keys have actually been spent on.
 *
 * Kuro measures this itself, from the token counts providers return, so it is
 * the one number on this screen that is not somebody's marketing copy. It is
 * still a floor rather than a total, and the panel says by how much rather than
 * hedging: a provider that returns no counts contributes a turn and no tokens,
 * and the number of those turns is shown so it can be judged.
 */
function UsagePanel({ usage }: { usage?: FreeUsage }) {
  if (!usage) return null

  const nothingYet = usage.month.turns === 0

  return (
    <section className="free-group">
      <span className="tile-group-title muted">
        <TitleWithHelp
          title="Usage"
          help={
            usage.month.unreportedTurns > 0
              ? `${usage.month.unreportedTurns} ${usage.month.unreportedTurns === 1 ? 'turn' : 'turns'} this month returned no token counts, so the totals are a floor.`
              : undefined
          }
        />
      </span>
      <div className="sunken-card">
      {nothingYet ? (
        <span className="faint">Nothing used this month</span>
      ) : (
        <>
          <div className="usage-totals">
            <Figure label="Today" value={count(usage.day.totalTokens)} unit="tokens" />
            <Figure label="This month" value={count(usage.month.totalTokens)} unit="tokens" />
            <Figure
              label="Per message"
              value={usage.averages.tokensPerTurn === null
                ? '—'
                : count(usage.averages.tokensPerTurn)}
              unit="average"
            />
            <Figure
              label="Per day"
              value={usage.averages.tokensPerDayThisMonth === null
                ? '—'
                : count(usage.averages.tokensPerDayThisMonth)}
              unit="this month"
            />
          </div>

          <div className="usage-rows">
            {usage.month.providers.map((row) => (
              <UsageRow key={row.providerSlug} row={row} />
            ))}
          </div>

        </>
      )}
      </div>
    </section>
  )
}

function Figure({ label, value, unit }: { label: string; value: string; unit: string }) {
  return (
    <div className="usage-figure">
      <span className="faint usage-figure-label">{label}</span>
      <strong className="usage-figure-value">{value}</strong>
      <span className="faint usage-figure-unit">{unit}</span>
    </div>
  )
}

/**
 * One provider's month, with a bar only where a limit was entered.
 *
 * No limit means no bar — not a bar at zero. Kuro cannot discover what these
 * allowances are (the providers state them in incompatible units and none
 * expose them over the API), and drawing an empty bar would imply it knew.
 */
function UsageRow({ row }: { row: FreeProviderUsage }) {
  const limit = row.limit?.tokensPerMonth ?? null
  const used = limit ? Math.min(100, (row.totalTokens / limit) * 100) : null

  return (
    <div className="usage-row">
      <div className="usage-row-head">
        <span className="usage-row-name">{row.name}</span>
        <span className="faint mono">
          {count(row.totalTokens)}
          {limit ? ` / ${count(limit)}` : ''}
        </span>
      </div>
      {used !== null && (
        <div className="usage-bar">
          <div className="usage-bar-fill" style={{ width: `${used}%` }} />
        </div>
      )}
      <span className="faint usage-row-note">
        {row.turns} {row.turns === 1 ? 'message' : 'messages'}
        {used !== null ? ` · ${used.toFixed(used < 1 ? 2 : 0)}% of your limit` : ''}
      </span>
    </div>
  )
}

function ProviderRow({ provider }: { provider: FreeProvider }) {
  const queryClient = useQueryClient()
  const [draft, setDraft] = useState('')
  const [editing, setEditing] = useState(false)
  const [result, setResult] = useState<{ ok: boolean; error?: string } | null>(null)

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['free'] })
    // A new key can add rows to the model picker, so that is stale too.
    void queryClient.invalidateQueries({ queryKey: ['models'] })
  }

  const save = useMutation({
    mutationFn: (key: string) => api.free.setKey(provider.slug, key),
    onSuccess: () => {
      setDraft('')
      setEditing(false)
      setResult(null)
      refresh()
    },
  })

  const remove = useMutation({
    mutationFn: () => api.free.removeKey(provider.slug),
    onSuccess: () => {
      setResult(null)
      refresh()
    },
  })

  const test = useMutation({
    mutationFn: () => api.free.test(provider.slug),
    onSuccess: (data) => {
      setResult(data)
      refresh()
    },
  })

  return (
    <div className={`server-row free-row ${provider.hasKey || provider.keyless ? '' : 'is-off'}`}>
      <ProviderLogo slug={provider.slug} label={provider.name} size={24} />
      <div className="server-row-main">
        <div className="server-row-head">
          <span
            className={`status-dot ${
              provider.trouble ? 'status-error' : provider.hasKey ? 'status-connected' : ''
            }`}
          />
          <span className="server-row-name">{provider.name}</span>
          {provider.trouble === 'rate_limited' && (
            <span className="tag tag-warn" title="Out of allowance for now">used up</span>
          )}
          {provider.trouble === 'rejected' && (
            <span className="tag tag-warn">key refused</span>
          )}
          {/* The server has always been able to say this; the type omitted it,
              so the tag never appeared. */}
          {provider.trouble === 'model_gone' && (
            <span className="tag tag-warn" title="Its models moved; Kuro is rechecking">rechecking</span>
          )}
          {provider.expired && <span className="tag tag-warn">trial ended</span>}
          {provider.privacy.trains && (
            <span className="tag tag-warn" title="Prompts sent here may be kept and used to train models">
              may train
            </span>
          )}
          {provider.privacy.logged && !provider.privacy.trains && (
            <span className="tag" title="Prompts sent here may be logged">logged</span>
          )}
        </div>

        <span className="muted truncate" title={provider.allowance}>{provider.allowance}</span>

        {provider.hasKey && <LimitField provider={provider} />}

        {provider.baseUrl.includes('ACCOUNT_ID') && provider.hasKey && (
          <p className="form-error">Needs your account id. Add it as a custom provider instead.</p>
        )}

        {/* A shared endpoint takes no key, and the server refuses one. Offering
            the field anyway would be an invitation to a dead end. */}
        {!provider.keyless && (editing || !provider.hasKey) && (
          <div className="inline-form">
            <input
              className="input"
              type="password"
              placeholder={provider.keyHint ?? 'Paste the key'}
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && draft.trim()) save.mutate(draft.trim())
              }}
            />
            <button
              className="btn btn-solid"
              disabled={!draft.trim() || save.isPending}
              onClick={() => save.mutate(draft.trim())}
            >
              Save
            </button>
            {editing && (
              <button className="btn" onClick={() => setEditing(false)}>
                Cancel
              </button>
            )}
          </div>
        )}

        {result && (
          <p className={result.ok ? 'faint' : 'form-error'}>
            {result.ok ? 'Working.' : result.error}
          </p>
        )}

        {!provider.hasKey && (
          <a
            className="external-link faint"
            href={provider.credentialsUrl}
            target="_blank"
            rel="noopener noreferrer"
          >
            {provider.keyless ? 'How it works' : 'Get a free key'}
            <ExternalIcon size={11} />
          </a>
        )}
      </div>

      {provider.hasKey && (
        <div className="server-row-actions">
          <button
            className="btn"
            onClick={() => test.mutate()}
            disabled={test.isPending}
            title="Ask this provider whether the key works"
          >
            {test.isPending ? <span className="spinner" /> : null}
            Test
          </button>
          <button
            className="btn btn-ghost btn-icon"
            aria-label={`Replace the key for ${provider.name}`}
            title="Replace the key"
            onClick={() => setEditing((open) => !open)}
          >
            <RefreshIcon size={14} />
          </button>
          <button
            className="btn btn-ghost btn-icon"
            aria-label={`Remove the key for ${provider.name}`}
            onClick={() => remove.mutate()}
          >
            <TrashIcon size={14} />
          </button>
        </div>
      )}
    </div>
  )
}
