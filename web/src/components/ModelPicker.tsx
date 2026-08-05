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
import {
  CheckIcon,
  ChevronIcon,
  CloudIcon,
  CubeIcon,
  ExternalIcon,
  GiftIcon,
  SearchIcon,
} from './icons'

/** Tallest the menu gets. Matches the `max-height` budget used in the CSS. */
const MENU_HEIGHT = 460
/** Kept clear of the window edge so the menu never sits flush against it. */
const VIEWPORT_MARGIN = 12

/**
 * How many models a provider may list before the list is cut short.
 *
 * OpenRouter advertises several hundred. Rendering all of them costs a visible
 * pause on open and produces a scrollbar nobody reaches the bottom of, so the
 * list is capped and the search box is what reaches the rest. The cap applies
 * only to an unfiltered view: once somebody types, they have said what they are
 * looking for and every match should appear.
 */
const MODELS_PER_PROVIDER = 40

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
 * Three ordering decisions carry weight. Local models come first, always,
 * because that is the default this application argues for. Local models are
 * grouped by publisher, because "which Qwen is this" is the question a flat list
 * leaves unanswered. And every provider is a *closed* section.
 *
 * That last one is recent and was forced by the catalogue sizes. A single
 * OpenRouter key reaches several hundred models and an NVIDIA key around sixty;
 * rendered flat, as this menu used to, they are one undifferentiated scroll in
 * which the four rows somebody actually wanted are invisible. So a provider is
 * one row until it is opened, and what it opens into is tagged: free or billed,
 * and what the model was trained for.
 */
export function ModelPicker({ installed, remote, selected, onSelect }: ModelPickerProps) {
  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [expanded, setExpanded] = useState<string[]>([])
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

  // The provider holding the current model opens with the menu, so the row that
  // is checked is the row that is on screen. Reset per opening rather than kept,
  // because a section left open from last time is not where attention is now.
  useEffect(() => {
    if (!open) return
    const holder = remote.find((model) => model.id === active)
    setExpanded(holder ? [`remote:${holder.connector_id}`] : [])
  }, [open, active, remote])

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

  // Searching opens everything: a closed section that contains the only match is
  // a search that appears to have found nothing.
  const searching = query.trim().length > 0

  // A local group is never a disclosure. There are a handful of them, they are
  // what this application is for, and hiding them behind the same toggle as a
  // four-hundred-model catalogue rendered the publisher heading over an empty
  // space — a group that looked broken rather than closed.
  const isOpen = (group: Group) =>
    !group.remote || searching || expanded.includes(group.key)

  const toggle = (key: string) =>
    setExpanded((keys) =>
      keys.includes(key) ? keys.filter((held) => held !== key) : [...keys, key],
    )

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
              placeholder="Search every provider…"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          </div>

          <div className="model-menu-list">
            {groups.length === 0 && <p className="faint model-menu-empty">Nothing matched.</p>}

            {groups.map((group, index) => (
              <ModelGroup
                key={group.key}
                group={group}
                // The heading that introduces the provider sections, shown once
                // above the first of them.
                sectionHead={
                  group.remote && !groups[index - 1]?.remote ? 'API providers' : null
                }
                open={isOpen(group)}
                onToggle={() => toggle(group.key)}
                active={active}
                onSelect={(id) => {
                  onSelect(id)
                  setOpen(false)
                }}
                truncate={!searching}
              />
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
 * One heading and the rows under it.
 *
 * Local groups are always open — there are a handful of them and they are the
 * point of the application. Provider groups are a disclosure, because a
 * catalogue of four hundred is not a list to scroll past on the way to
 * something else.
 */
function ModelGroup({
  group,
  sectionHead,
  open,
  onToggle,
  active,
  onSelect,
  truncate,
}: {
  group: Group
  sectionHead: string | null
  open: boolean
  onToggle: () => void
  active: string | null
  onSelect: (id: string) => void
  truncate: boolean
}) {
  // How many rows this group has been asked to show. Grows a page at a time
  // rather than jumping to four hundred, which is what the cap is protecting
  // against in the first place.
  const [limit, setLimit] = useState(MODELS_PER_PROVIDER)

  // A new search is a new question, so the list starts from the top again.
  useEffect(() => setLimit(MODELS_PER_PROVIDER), [truncate])

  const shown = truncate ? group.options.slice(0, limit) : group.options
  const hidden = group.options.length - shown.length

  const icon = group.free ? (
    <GiftIcon size={11} />
  ) : group.remote ? (
    <CloudIcon size={11} />
  ) : (
    <CubeIcon size={11} />
  )

  return (
    <>
      {sectionHead && <div className="model-section-head">{sectionHead}</div>}

      <div className={`model-group ${group.remote ? 'is-collapsible' : ''}`}>
        {group.remote ? (
          <button
            className="model-group-head is-button"
            aria-expanded={open}
            // Spelled out because the visible label is three separate spans and
            // a decorative icon, which a screen reader runs together into
            // something like "OpenRouter 362" with no hint that it opens.
            aria-label={`${group.label}, ${group.options.length} models`}
            onClick={onToggle}
          >
            {icon}
            <span className="model-group-name">{group.label}</span>
            <span className="model-group-count">{group.options.length}</span>
            <ChevronIcon size={11} className={open ? 'is-open' : ''} />
          </button>
        ) : (
          <div className="model-group-head">
            {icon}
            <span className="model-group-name">{group.label}</span>
          </div>
        )}

        {open &&
          shown.map((option) => (
            <button
              key={option.id}
              className={`model-option ${option.id === active ? 'is-on' : ''} ${
                option.pooled ? 'is-pooled' : ''
              } ${option.unavailable ? 'is-unavailable' : ''}`}
              role="option"
              aria-selected={option.id === active}
              // Picking a model that has already refused produces an error the
              // user can do nothing about. The row stays visible and says why
              // instead.
              disabled={Boolean(option.unavailable)}
              onClick={() => onSelect(option.id)}
              title={option.unavailable ? `${option.name} — ${option.unavailable}` : option.id}
            >
              <span className="model-option-check">
                {option.id === active && <CheckIcon size={13} />}
              </span>

              <span className="model-option-name">
                {option.pooled && <GiftIcon size={12} />}
                {option.name}
              </span>

              <span className="model-option-tags">
                {option.unavailable ? (
                  <>
                    <span className="faint model-option-why">{option.unavailable}</span>
                    {option.fixUrl && (
                      // Rendered as a span rather than a nested anchor: this row
                      // is a button, and a link inside a button is invalid markup
                      // that browsers resolve inconsistently.
                      <span
                        className="model-option-fix"
                        role="link"
                        tabIndex={0}
                        title="Open the page that enables this model"
                        onClick={(event) => {
                          event.stopPropagation()
                          window.open(option.fixUrl, '_blank', 'noopener,noreferrer')
                        }}
                      >
                        <ExternalIcon size={12} />
                      </span>
                    )}
                  </>
                ) : (
                  <>
                    {option.note && <span className="faint">{option.note}</span>}
                    {/* Cost first: it is the tag somebody is scanning for, and
                        the one whose absence costs money. */}
                    {option.cost && (
                      <span className={`tag ${option.cost === 'free' ? 'tag-live' : ''}`}>
                        {option.cost}
                      </span>
                    )}
                    {option.specialities.map((speciality) => (
                      <span key={speciality} className="tag">
                        {speciality}
                      </span>
                    ))}
                    {option.size && <span className="faint mono">{option.size}</span>}
                    {option.quant && <span className="quant">{option.quant}</span>}
                    {option.loaded && <span className="tag tag-live">loaded</span>}
                  </>
                )}
              </span>
            </button>
          ))}

        {open && hidden > 0 && (
          <button
            className="model-group-more"
            onClick={() => setLimit((current) => current + MODELS_PER_PROVIDER)}
          >
            Show {Math.min(hidden, MODELS_PER_PROVIDER)} more
            <span className="faint"> · {hidden} left</span>
          </button>
        )}
      </div>
    </>
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
    const menuWidth = Math.min(360, window.innerWidth - VIEWPORT_MARGIN * 2)
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
  /** `free` or `paid`. Absent on a local model, which costs neither. */
  cost?: 'free' | 'paid'
  /** What the model was trained for, as the server read it off the name. */
  specialities: string[]
  /** Why this cannot be picked. The row greys out and stops responding. */
  unavailable?: string
  /** A page that would make it work. Rendered as a link on the greyed row. */
  fixUrl?: string
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
      specialities: [],
    }

    const held = local.get(publisher) ?? []
    held.push(option)
    local.set(publisher, held)
  }

  const byProvider = new Map<string, { label: string; options: Option[] }>()

  for (const model of remote) {
    // A speciality is worth searching by: "coder" should find every coding
    // model on every provider, which is the whole reason the tags exist.
    const searchable = [model.name, model.connector_label, ...model.specialities]
    if (!searchable.some(matches)) continue

    const held = byProvider.get(model.connector_id) ?? {
      label: model.connector_label,
      options: [],
    }
    held.options.push({
      id: model.id,
      name: model.name,
      quant: null,
      size: model.params_b ? `${trimZero(model.params_b)}B` : null,
      loaded: false,
      pooled: model.pooled,
      note: model.pooled ? `${model.pool_size} free models` : undefined,
      cost: model.free ? 'free' : 'paid',
      // `general` says nothing the row does not already say, and putting it on
      // every untagged model turns a signal into wallpaper.
      specialities: model.specialities.filter((speciality) => speciality !== 'general'),
      unavailable: model.unavailable ?? undefined,
      fixUrl: model.fix_url ?? undefined,
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
    // A pooled row is the one most people want and the one nobody would find by
    // scrolling, so it leads its provider whatever its name sorts as.
    options: [...options].sort((left, right) => Number(right.pooled) - Number(left.pooled)),
  }))
  remoteGroups.sort((left, right) => Number(right.free) - Number(left.free))
  groups.push(...remoteGroups)

  return groups
}

function compareByKey(a: [string, Option[]], b: [string, Option[]]): number {
  return a[0].localeCompare(b[0])
}

/** `70` rather than `70.0`, and `6.7` kept as it is. */
function trimZero(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1)
}
