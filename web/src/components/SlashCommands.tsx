import { useEffect, useState } from 'react'

export interface SlashCommand {
  name: string
  hint: string
  aliases?: string[]
  state?: string
  kind: 'toggle' | 'skill'
  slug?: string
  run?: () => void
}

export interface CommandToken {
  query: string
  start: number
  end: number
}

export function commandTokenAt(text: string, caret: number): CommandToken | null {
  const position = Math.max(0, Math.min(caret, text.length))

  let start = position
  while (start > 0 && !/\s/.test(text[start - 1] as string)) start -= 1

  if (text[start] !== '/') return null

  let end = position
  while (end < text.length && !/\s/.test(text[end] as string)) end += 1

  const query = text.slice(start + 1, end)
  if (query.includes('/')) return null

  return { query: query.toLowerCase(), start, end }
}

export function matchCommands(commands: SlashCommand[], query: string): SlashCommand[] {
  if (!query) return commands

  const scored = commands
    .map((command) => {
      const names = [command.name, ...(command.aliases ?? [])]
      const prefix = names.some((name) => name.startsWith(query))
      const contains = prefix || names.some((name) => name.includes(query))
      return { command, rank: prefix ? 0 : 1, hit: contains }
    })
    .filter((entry) => entry.hit)

  scored.sort((left, right) => left.rank - right.rank)
  return scored.map((entry) => entry.command)
}

export function skillsNamedIn(text: string, known: PaletteSkill[]): string[] {
  const found: string[] = []

  for (const match of text.matchAll(/(^|[\s([{])\/([A-Za-z][\w-]*)/g)) {
    const slug = (match[2] ?? '').toLowerCase()
    if (!known.some((skill) => skill.slug === slug)) continue
    if (!found.includes(slug)) found.push(slug)
  }

  return found
}

interface SlashMenuProps {
  commands: SlashCommand[]
  query: string
  index: number
  onHover: (index: number) => void
  onRun: (command: SlashCommand) => void
}

export function SlashMenu({ commands, query, index, onHover, onRun }: SlashMenuProps) {
  if (commands.length === 0) {
    return (
      <div className="slash-menu fade-in">
        <p className="faint slash-empty">No skill or switch called “/{query}”.</p>
      </div>
    )
  }

  return (
    <div className="slash-menu fade-in" role="listbox" aria-label="Skills and switches">
      {commands.map((command, position) => (
        <button
          key={`${command.kind}:${command.name}`}
          className={`slash-item ${position === index ? 'is-on' : ''}`}
          role="option"
          aria-selected={position === index}
          onMouseEnter={() => onHover(position)}
          onMouseDown={(event) => {
            event.preventDefault()
            onRun(command)
          }}
        >
          <span className="slash-name">/{command.name}</span>
          <span className="slash-hint faint">{command.hint}</span>
          {command.state && <span className="tag">{command.state}</span>}
        </button>
      ))}
    </div>
  )
}

export function useSlashKeys(open: boolean, count: number) {
  const [index, setIndex] = useState(0)

  useEffect(() => {
    setIndex((current) => (current >= count ? Math.max(0, count - 1) : current))
  }, [count])

  useEffect(() => {
    if (!open) setIndex(0)
  }, [open])

  return { index, setIndex }
}

export interface PaletteSkill {
  slug: string
  name: string
  blurb: string
}

export function buildCommands(parts: {
  toggles: {
    id: string
    label: string
    on: boolean
    onChange: (on: boolean) => void
    command?: string
  }[]
  skills: PaletteSkill[]
  attached: string[]
}): SlashCommand[] {
  const fromToggles: SlashCommand[] = parts.toggles.map((toggle) => ({
    name: toggle.command ?? toggle.id,
    hint: `Turn ${toggle.label.toLowerCase()} ${toggle.on ? 'off' : 'on'} for this message`,
    state: toggle.on ? 'on' : 'off',
    kind: 'toggle',
    run: () => toggle.onChange(!toggle.on),
  }))

  const fromSkills: SlashCommand[] = parts.skills.map((skill) => ({
    name: skill.slug,
    hint: skill.blurb,
    aliases: [skill.name.toLowerCase()],
    state: parts.attached.includes(skill.slug) ? 'in message' : undefined,
    kind: 'skill',
    slug: skill.slug,
  }))

  const all = [...fromToggles, ...fromSkills]

  const seen = new Set<string>()
  return all.filter((command) => {
    if (seen.has(command.name)) return false
    seen.add(command.name)
    return true
  })
}
