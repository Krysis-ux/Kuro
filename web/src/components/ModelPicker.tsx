import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import {
  formatBytes,
  friendlyModelName,
  isRemoteModel,
  publisherOf,
  quantOf,
  type InstalledModel,
  type RemoteModel,
} from '../lib/api'
import { CheckIcon, ChevronIcon, CloudIcon, CubeIcon, SearchIcon } from './icons'

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
        {activeIsRemote ? <CloudIcon size={13} /> : <CubeIcon size={13} />}
        <span className="model-trigger-name">
          {active ? friendlyModelName(active) : 'Choose a model'}
        </span>
        {activeQuant && <span className="quant">{activeQuant}</span>}
        <ChevronIcon size={12} />
      </button>

      {open && (
        <div className="model-menu fade-in" role="listbox">
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
                  {group.remote ? <CloudIcon size={11} /> : <CubeIcon size={11} />}
                  <span>{group.label}</span>
                  {group.remote && <span className="tag tag-warn">leaves this machine</span>}
                </div>

                {group.options.map((option) => (
                  <button
                    key={option.id}
                    className={`model-option ${option.id === active ? 'is-on' : ''}`}
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

                    <span className="model-option-name">{option.name}</span>

                    <span className="model-option-tags">
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

interface Option {
  id: string
  name: string
  quant: string | null
  size: string | null
  loaded: boolean
}

interface Group {
  key: string
  label: string
  remote: boolean
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
    })
    byProvider.set(model.connector_id, held)
  }

  const groups: Group[] = []

  for (const [publisher, options] of [...local].sort(compareByKey)) {
    groups.push({ key: `local:${publisher}`, label: publisher, remote: false, options })
  }
  for (const [id, { label, options }] of byProvider) {
    groups.push({ key: `remote:${id}`, label, remote: true, options })
  }

  return groups
}

function compareByKey(a: [string, Option[]], b: [string, Option[]]): number {
  return a[0].localeCompare(b[0])
}
