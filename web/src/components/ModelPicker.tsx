import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  formatBytes,
  friendlyModelName,
  isFreeModel,
  isRemoteModel,
  publisherOf,
  quantOf,
  type InstalledModel,
  type RemoteModel,
} from '../lib/api'
import { CheckIcon, ChevronIcon, CloudIcon, CubeIcon, GiftIcon, SearchIcon } from './icons'

/** Tallest the menu gets. Matches the `max-height` budget used in the CSS. */
const MENU_HEIGHT = 420
/** Kept clear of the window edge so the menu never sits flush against it. */
const VIEWPORT_MARGIN = 12

interface Placement {
  side: 'above' | 'below'
  align: 'left' | 'right'
  /** How tall the menu may be here, so a tight fit scrolls instead of clipping. */
  maxHeight: number
}

interface ModelPickerProps {
  installed: InstalledModel[]
  remote: RemoteModel[]
  selected: string | null
  onSelect: (id: string) => void
}

/**
 * The model chooser.
 *
 * A native `<select>` cannot show what a person actually needs to choose between:
 * the quantization, the file size, whether the model is already resident, and —
 * once providers exist — whether picking it sends the conversation off the
 * machine. So this is a listbox.
 *
 * Two ordering decisions carry weight. Local models come first, always, because
 * that is the default this application argues for. And models are grouped by
 * publisher, because "which Qwen is this" is the question a flat list leaves
 * unanswered.
 */
export function ModelPicker({ installed, remote, selected, onSelect }: ModelPickerProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const anchorRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const navigate = useNavigate()
  const placement = usePlacement(anchorRef, open)

  const ready = useMemo(
    () => installed.filter((entry) => entry.model.status === 'ready'),
    [installed],
  )

  // Fall back to the only installed model so a first-time user never has to
  // choose before sending anything.
  const active = selected ?? ready[0]?.model.id ?? remote[0]?.id ?? null

  useEffect(() => {
    if (!open) return

    const close = (event: MouseEvent) => {
      if (!anchorRef.current?.contains(event.target as Node)) setOpen(false)
    }
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false)
    }

    document.addEventListener('mousedown', close)
    document.addEventListener('keydown', escape)
    return () => {
      document.removeEventListener('mousedown', close)
      document.removeEventListener('keydown', escape)
    }
  }, [open])

  // Typing is the fastest way through a long list, so the field takes focus.
  useEffect(() => {
    if (open) searchRef.current?.focus()
  }, [open])

  const groups = useMemo(() => buildGroups(ready, remote, query), [ready, remote, query])
  const total = ready.length + remote.length

  if (total === 0) {
    return (
      <button className="model-trigger is-empty" onClick={() => navigate('/models')}>
        <CubeIcon size={13} />
        No models yet
      </button>
    )
  }

  const activeQuant = active ? quantOf(active) : null
  const activeIsRemote = active ? isRemoteModel(active) : false
  const activeIsFree = active ? isFreeModel(active) : false

  return (
    <div className="menu-anchor" ref={anchorRef}>
      <button
        className="model-trigger"
        onClick={() => {
          setOpen((value) => !value)
          setQuery('')
        }}
        aria-haspopup="listbox"
        aria-expanded={open}
        title={active ?? 'Choose a model'}
      >
        {activeIsFree ? (
          <GiftIcon size={13} />
        ) : activeIsRemote ? (
          <CloudIcon size={13} />
        ) : (
          <CubeIcon size={13} />
        )}
        <span className="model-trigger-name">
          {active ? friendlyModelName(active) : 'Choose a model'}
        </span>
        {activeQuant && <span className="quant">{activeQuant}</span>}
        <ChevronIcon size={12} />
      </button>

      {open && (
        <div
          className={`model-menu fade-in is-${placement.side} is-${placement.align}`}
          role="listbox"
          style={{ maxHeight: `${placement.maxHeight}px` }}
        >
          <div className="model-menu-search">
            <SearchIcon size={13} className="search-icon" />
            <input
              ref={searchRef}
              className="input"
              placeholder="Search models…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>

          <div className="model-menu-list">
            {groups.length === 0 && <p className="faint model-menu-empty">Nothing matched.</p>}

            {groups.map((group) => (
              <div key={group.key} className="model-group">
                <div className="model-group-head">
                  {group.free ? (
                    <GiftIcon size={11} />
                  ) : group.remote ? (
                    <CloudIcon size={11} />
                  ) : (
                    <CubeIcon size={11} />
                  )}
                  <span>{group.label}</span>
                  {group.remote && <span className="tag tag-warn">leaves this machine</span>}
                </div>

                {group.options.map((option) => (
                  <button
                    key={option.id}
                    className={`model-option ${option.id === active ? 'is-on' : ''} ${
                      option.pooled ? 'is-pooled' : ''
                    }`}
                    role="option"
                    aria-selected={option.id === active}
                    onClick={() => {
                      onSelect(option.id)
                      setOpen(false)
                    }}
                  >
                    <span className="model-option-check">
                      {option.id === active && <CheckIcon size={13} />}
                    </span>

                    <span className="model-option-name">
                      {option.pooled && <GiftIcon size={12} />}
                      {option.name}
                    </span>

                    <span className="model-option-tags">
                      {option.note && <span className="faint">{option.note}</span>}
                      {option.quant && <span className="quant">{option.quant}</span>}
                      {option.size && <span className="faint mono">{option.size}</span>}
                      {option.loaded && <span className="tag tag-live">loaded</span>}
                    </span>
                  </button>
                ))}
              </div>
            ))}
          </div>

          <button className="model-menu-foot" onClick={() => navigate('/models')}>
            Manage models
          </button>
        </div>
      )}
    </div>
  )
}

/**
 * Which way the menu opens.
 *
 * It used to open upward unconditionally, which is right in the composer — the
 * trigger is a few pixels off the bottom of the window — and wrong everywhere
 * else. The Code page puts the same picker in a header, where opening upward
 * meant the list rendered off the top of the screen with only its last row
 * visible, overlapping the toolbar.
 *
 * Measured on open rather than on every render: the trigger does not move while
 * the menu is up, and a resize observer here would recompute during the fade-in
 * and make the menu jump.
 */
function usePlacement(anchor: React.RefObject<HTMLElement | null>, open: boolean): Placement {
  const [placement, setPlacement] = useState<Placement>({
    side: 'above',
    align: 'right',
    maxHeight: MENU_HEIGHT,
  })

  useLayoutEffect(() => {
    if (!open) return
    const trigger = anchor.current?.getBoundingClientRect()
    if (!trigger) return

    const below = window.innerHeight - trigger.bottom - VIEWPORT_MARGIN
    const above = trigger.top - VIEWPORT_MARGIN

    // Below when it fits there, and otherwise whichever side has more room.
    // Preferring below on a tie is what makes a header-mounted picker behave
    // like every other dropdown a person has used.
    const side = below >= MENU_HEIGHT || below >= above ? 'below' : 'above'

    // The menu is right-aligned by default, which pushes it off-screen when the
    // trigger sits near the left edge — a narrow window, or a picker in a
    // sidebar.
    const menuWidth = Math.min(340, window.innerWidth - VIEWPORT_MARGIN * 2)
    const align = trigger.right - menuWidth < VIEWPORT_MARGIN ? 'left' : 'right'

    setPlacement({
      side,
      align,
      maxHeight: Math.max(200, Math.min(MENU_HEIGHT, side === 'below' ? below : above)),
    })
  }, [anchor, open])

  return placement
}

interface Option {
  id: string
  name: string
  quant: string | null
  size: string | null
  loaded: boolean
  /** A pool of every free model on this provider, rather than one model. */
  pooled?: boolean
  /** Shown beside a pooled row, so "free models" is a number and not a claim. */
  note?: string
}

interface Group {
  key: string
  label: string
  remote: boolean
  /** The pooled free tiers, which are remote but not a provider you pay. */
  free: boolean
  options: Option[]
}

/**
 * Group by publisher for local models and by provider for remote ones.
 *
 * The two cannot share a grouping key: `anthropic` as a publisher of local
 * weights and Anthropic as a provider you pay are different things, and merging
 * them would hide exactly the distinction the picker exists to make.
 */
function buildGroups(
  installed: InstalledModel[],
  remote: RemoteModel[],
  query: string,
): Group[] {
  const needle = query.trim().toLowerCase()
  const matches = (haystack: string) => !needle || haystack.toLowerCase().includes(needle)

  const local = new Map<string, Option[]>()

  for (const entry of installed) {
    const id = entry.model.id
    if (!matches(id) && !matches(entry.model.display_name)) continue

    const publisher = publisherOf(id) ?? entry.model.family ?? 'Installed'
    const name = friendlyModelName(id)

    const option: Option = {
      id,
      // The publisher is already the group heading, so it is not repeated.
      name: publisher && name.startsWith(`${publisher}/`)
        ? name.slice(publisher.length + 1)
        : name,
      quant: quantOf(id) ?? entry.model.quant?.toUpperCase() ?? null,
      size: entry.model.file_size_bytes ? formatBytes(entry.model.file_size_bytes) : null,
      loaded: entry.loaded,
    }

    const held = local.get(publisher) ?? []
    held.push(option)
    local.set(publisher, held)
  }

  const byProvider = new Map<string, { label: string; options: Option[] }>()

  for (const model of remote) {
    if (!matches(model.name) && !matches(model.connector_label)) continue

    const held = byProvider.get(model.connector_id) ?? {
      label: model.connector_label,
      options: [],
    }
    held.options.push({
      id: model.id,
      name: model.name,
      quant: null,
      size: null,
      loaded: false,
      pooled: model.pooled,
      note: model.pooled ? `${model.pool_size} free models` : undefined,
    })
    byProvider.set(model.connector_id, held)
  }

  const groups: Group[] = []

  for (const [publisher, options] of [...local].sort(compareByKey)) {
    groups.push({
      key: `local:${publisher}`,
      label: publisher,
      remote: false,
      free: false,
      options,
    })
  }

  // The free pool goes directly after the local models and ahead of the paid
  // providers, because it is the closest thing to "free" that is not local, and
  // somebody scanning this list is usually scanning in that order.
  const remoteGroups: Group[] = [...byProvider].map(([id, { label, options }]) => ({
    key: `remote:${id}`,
    label,
    remote: true,
    free: id === 'kuro-free',
    options,
  }))
  remoteGroups.sort((left, right) => Number(right.free) - Number(left.free))
  groups.push(...remoteGroups)

  return groups
}

function compareByKey(a: [string, Option[]], b: [string, Option[]]): number {
  return a[0].localeCompare(b[0])
}
