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

const MENU_HEIGHT = 460
const VIEWPORT_MARGIN = 12
const MIN_MENU_HEIGHT = 200

const MODELS_PER_PROVIDER = 40

interface Placement {
  side: 'above' | 'below'
  align: 'left' | 'right'
  maxHeight: number
}

interface ModelPickerProps {
  installed: InstalledModel[]
  remote: RemoteModel[]
  selected: string | null
  onSelect: (id: string) => void
}

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

  useEffect(() => {
    if (open) searchRef.current?.focus()
  }, [open])

  const groups = useMemo(() => buildGroups(ready, remote, query), [ready, remote, query])
  const total = ready.length + remote.length

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

  const searching = query.trim().length > 0

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
                sectionHead={
                  index === 0 && !group.remote
                    ? 'On this machine'
                    : group.remote && !groups[index - 1]?.remote
                      ? 'API providers'
                      : null
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
  const [limit, setLimit] = useState(MODELS_PER_PROVIDER)

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

function usePlacement(anchor: React.RefObject<HTMLElement | null>, open: boolean): Placement {
  const [placement, setPlacement] = useState<Placement>({
    side: 'above',
    align: 'right',
    maxHeight: MENU_HEIGHT,
  })

  useLayoutEffect(() => {
    if (!open) return

    const measure = () => {
      const trigger = anchor.current?.getBoundingClientRect()
      if (!trigger) return

      const below = window.innerHeight - trigger.bottom - VIEWPORT_MARGIN
      const above = trigger.top - VIEWPORT_MARGIN

      const side = below >= MENU_HEIGHT || below >= above ? 'below' : 'above'

      const menuWidth = Math.min(360, window.innerWidth - VIEWPORT_MARGIN * 2)
      const align = trigger.right - menuWidth < VIEWPORT_MARGIN ? 'left' : 'right'

      const room = side === 'below' ? below : above
      setPlacement((current) => {
        const next: Placement = {
          side,
          align,
          maxHeight: Math.max(MIN_MENU_HEIGHT, Math.min(MENU_HEIGHT, room)),
        }
        return current.side === next.side &&
          current.align === next.align &&
          current.maxHeight === next.maxHeight
          ? current
          : next
      })
    }

    measure()

    const frame = requestAnimationFrame(measure)

    const observer = new ResizeObserver(measure)
    if (anchor.current) observer.observe(anchor.current)
    observer.observe(document.documentElement)

    window.addEventListener('resize', measure)
    window.addEventListener('scroll', measure, true)

    return () => {
      cancelAnimationFrame(frame)
      observer.disconnect()
      window.removeEventListener('resize', measure)
      window.removeEventListener('scroll', measure, true)
    }
  }, [anchor, open])

  return placement
}

interface Option {
  id: string
  name: string
  quant: string | null
  size: string | null
  loaded: boolean
  pooled?: boolean
  note?: string
  cost?: 'free' | 'paid'
  specialities: string[]
  unavailable?: string
  fixUrl?: string
}

interface Group {
  key: string
  label: string
  remote: boolean
  free: boolean
  options: Option[]
}

function brandOf(id: string, family: string | null | undefined): string {
  const publisher = publisherOf(id)
  if (publisher) return publisher

  const leading = friendlyModelName(id).split(/[\/:]/)[0] ?? id
  const word = leading.split('-')[0] ?? leading
  const stripped = word.replace(/[\d.]+$/, '')

  return stripped.length >= 2 ? stripped : (family ?? word ?? 'Local')
}

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

    const publisher = brandOf(id, entry.model.family)
    const name = friendlyModelName(id)

    const option: Option = {
      id,
      name: publisher && name.startsWith(`${publisher}/`)
        ? name.slice(publisher.length + 1)
        : name,
      quant: quantOf(id) ?? entry.model.quant?.toUpperCase() ?? null,
      size: entry.model.file_size_bytes ? formatBytes(entry.model.file_size_bytes) : null,
      loaded: entry.loaded,
      specialities: entry.kind && entry.kind !== 'chat' ? [entry.kind] : [],
      unavailable: entry.chat === false ? `This is an ${entry.kind} model, not a chat model` : undefined,
    }

    const held = local.get(publisher) ?? []
    held.push(option)
    local.set(publisher, held)
  }

  const byProvider = new Map<string, { label: string; options: Option[] }>()

  for (const model of remote) {
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

  const remoteGroups: Group[] = [...byProvider].map(([id, { label, options }]) => ({
    key: `remote:${id}`,
    label,
    remote: true,
    free: id === 'kuro-free',
    options: [...options].sort((left, right) => Number(right.pooled) - Number(left.pooled)),
  }))
  remoteGroups.sort((left, right) => Number(right.free) - Number(left.free))
  groups.push(...remoteGroups)

  return groups
}

function compareByKey(a: [string, Option[]], b: [string, Option[]]): number {
  return a[0].localeCompare(b[0])
}

function trimZero(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1)
}
