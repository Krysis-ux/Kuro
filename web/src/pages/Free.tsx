import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  api,
  type FreeProvider,
  type FreeProviderUsage,
  type FreeUsage,
} from '../lib/api'
import {
  ChartIcon,
  CheckIcon,
  ExternalIcon,
  GiftIcon,
  KeyIcon,
  RefreshIcon,
  TrashIcon,
} from '../components/icons'

/**
 * Free models.
 *
 * Every company running an inference API gives some of it away, and individually
 * each allowance is a toy. The reason nobody uses them together is the
 * bookkeeping: a dozen keys, a dozen dashboards, and no way to know which one
 * still has quota this hour.
 *
 * This screen is that bookkeeping. Paste in whichever keys you have and they
 * become one model in the picker, called Kuro Free, which sends each request to
 * whichever provider is currently able to serve it.
 *
 * The page is written to prevent one specific misunderstanding: that Kuro is
 * giving away inference. It is not. There is no shared account and no key here
 * that Kuro supplies, and nothing works until you have added one of your own.
 * Saying that once at the top is cheaper than letting somebody discover it from
 * an error message.
 */
export function FreePage() {
  const queryClient = useQueryClient()
  const overview = useQuery({ queryKey: ['free'], queryFn: api.free.overview })
  // A separate query from the overview, which is re-read after every key edit.
  // This is the one request whose cost grows with the size of the message
  // table, and coupling it to key editing would re-run it constantly.
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
  // Shared endpoints are their own group, listed last, because the order on
  // screen has to be the order requests are routed in — a page that mixed them
  // among the keyed providers would be claiming something the pool will not do.
  const shared = providers.filter((provider) => provider.keyless)
  const owned = providers.filter((provider) => !provider.keyless)
  const withKeys = owned.filter((provider) => provider.hasKey)
  const without = owned.filter((provider) => !provider.hasKey)

  return (
    <div className="page">
      <header className="page-head">
        <h1>Free models</h1>
        <p className="muted">
          Most inference providers have a free tier. Add the keys you have and Kuro pools them
          into one model — it picks whichever provider can answer, and moves to the next when
          one runs out of allowance for the day.
        </p>
      </header>

      <section className="panel">
        <h2 className="panel-title">
          <GiftIcon size={15} />
          How this works
        </h2>
        <p className="faint panel-note">
          <strong>Kuro does not supply any keys.</strong> Every key below is one you sign up
          for yourself, on a free tier, in your own name. Nothing here works until you have
          added at least one — and once you have, <code className="mono">Kuro Free</code>{' '}
          appears in the model picker beside your local models.
        </p>
        <p className="faint panel-note">
          These requests leave this machine, exactly as a paid provider's would. Free tiers are
          also the ones most likely to train on what they receive, so keep anything private on
          a local model.
        </p>

        {held && (
          <div className="row">
            <span className="faint">Ready</span>
            <span>
              {held.availableCount === 0
                ? 'No providers yet'
                : `${held.availableCount} of ${held.keyCount} added ${
                    held.keyCount === 1 ? 'key' : 'keys'
                  } working`}
            </span>
          </div>
        )}

        {held && held.keyCount > 0 && (
          <div className="free-flavours">
            {held.flavours.map((flavour) => (
              <div
                key={flavour.id}
                className={`free-flavour ${flavour.available ? '' : 'is-off'}`}
              >
                <div className="free-flavour-head">
                  <code className="mono">{flavour.id}</code>
                  {flavour.available ? (
                    <span className="tag tag-live">ready</span>
                  ) : (
                    <span className="tag">no provider for this</span>
                  )}
                </div>
                <p className="muted">{flavour.blurb}</p>
              </div>
            ))}
          </div>
        )}
      </section>

      {overview.isError && (
        <section className="panel">
          <p className="form-error">The free-provider list could not be read.</p>
        </section>
      )}

      {withKeys.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">Your keys</h2>
          <div className="server-rows">
            {withKeys.map((provider) => (
              <ProviderRow key={provider.slug} provider={provider} />
            ))}
          </div>
        </section>
      )}

      <section className="panel">
        <h2 className="panel-title">{withKeys.length > 0 ? 'Add more' : 'Providers'}</h2>
        <p className="faint panel-note">
          Each of these is free to sign up for and needs no card. Adding more is what makes the
          pool worth having: when one is out of allowance, the next one answers.
        </p>
        <div className="server-rows">
          {without.map((provider) => (
            <ProviderRow key={provider.slug} provider={provider} />
          ))}
        </div>
      </section>

      {shared.length > 0 && (
        <section className="panel">
          <h2 className="panel-title">Shared endpoints</h2>
          <p className="faint panel-note">
            These answer with no account and no key at all, which is what makes Kuro Free work
            before you have signed up for anything. They are used <strong>last</strong> — any
            provider you hold a key for is tried first, even when it has fallen back to a model
            Kuro had to guess at. They are shared, so they are rate limited by your address and
            often busy, and several keep or train on what they are sent.
          </p>

          <label className="row toggle-row">
            <input
              type="checkbox"
              checked={held?.allowKeyless ?? true}
              onChange={(event) => setKeyless.mutate(event.target.checked)}
              disabled={setKeyless.isPending}
            />
            <span>Use shared endpoints when nothing else can answer</span>
          </label>

          <div className="server-rows">
            {shared.map((provider) => (
              <ProviderRow key={provider.slug} provider={provider} />
            ))}
          </div>
        </section>
      )}

      <UsagePanel usage={usage.data} />
    </div>
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
      <span className="faint">Monthly allowance, if you know it</span>
      <input
        className="input limit-input"
        inputMode="numeric"
        placeholder="e.g. 1000000 tokens"
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
    <section className="panel">
      <h2 className="panel-title">
        <ChartIcon size={15} />
        What you have used
      </h2>

      {nothingYet ? (
        <p className="faint panel-note">
          Nothing yet this month. Once a free provider answers a message, what it cost appears
          here.
        </p>
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

          {usage.month.unreportedTurns > 0 && (
            <p className="faint panel-note">
              {usage.month.unreportedTurns} {usage.month.unreportedTurns === 1 ? 'turn' : 'turns'}{' '}
              this month returned no token counts, so those are not included above. Several free
              tiers do not report them.
            </p>
          )}
        </>
      )}
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
        {used !== null ? ` · ${used.toFixed(used < 1 ? 2 : 0)}% of what you entered` : ''}
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
    <div className={`server-row ${provider.hasKey ? '' : 'is-off'}`}>
      <div className="server-row-main">
        <div className="server-row-head">
          <span
            className={`status-dot ${
              provider.trouble ? 'status-error' : provider.hasKey ? 'status-connected' : ''
            }`}
          />
          <span className="server-row-name">{provider.name}</span>
          {provider.hasKey && (
            <span className="tag">
              <KeyIcon size={10} /> key stored
            </span>
          )}
          {provider.trouble === 'rate_limited' && (
            <span className="tag tag-warn">out of allowance for now</span>
          )}
          {provider.trouble === 'rejected' && (
            <span className="tag tag-warn">key refused</span>
          )}
          {/* The server has always been able to say this; the type omitted it,
              so the tag never appeared. */}
          {provider.trouble === 'model_gone' && (
            <span className="tag tag-warn">its models moved — rechecking</span>
          )}
          {provider.keyless && <span className="tag">no account needed</span>}
          {provider.expired && <span className="tag tag-warn">trial ended</span>}
          {provider.privacy.trains && (
            <span className="tag tag-warn" title="Prompts sent here may be kept and used to train models">
              may train on prompts
            </span>
          )}
          {provider.privacy.logged && !provider.privacy.trains && (
            <span className="tag">may be logged</span>
          )}
        </div>

        <p className="muted">{provider.allowance}</p>

        {provider.baseUrl.includes('ACCOUNT_ID') && provider.hasKey && (
          <p className="form-error">
            This provider's address needs your account id substituted into it, which Kuro
            cannot do for you yet. Add it as a custom endpoint under Cloud instead.
          </p>
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
              className="btn btn-solid btn-sm"
              disabled={!draft.trim() || save.isPending}
              onClick={() => save.mutate(draft.trim())}
            >
              Save
            </button>
            {editing && (
              <button className="btn btn-ghost btn-sm" onClick={() => setEditing(false)}>
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

        <a
          className="external-link faint"
          href={provider.credentialsUrl}
          target="_blank"
          rel="noopener noreferrer"
        >
          {provider.keyless ? 'How this endpoint works' : 'Get a free key'}
          <ExternalIcon size={11} />
        </a>
      </div>

      {provider.hasKey && <LimitField provider={provider} />}

      {provider.hasKey && (
        <div className="server-row-actions">
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => test.mutate()}
            disabled={test.isPending}
            title="Ask this provider whether the key works"
          >
            {test.isPending ? <span className="spinner" /> : <CheckIcon size={13} />}
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
