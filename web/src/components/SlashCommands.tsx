import { useEffect, useState } from 'react'

/**
 * The `/` palette.
 *
 * Everything here is reachable another way — through the sidebar, a toggle, or
 * the model picker — which is the point rather than an objection. The controls
 * are spread across the window and the keyboard is where the hands already are
 * halfway through typing a message; a palette is the shortest path between
 * "I want the web on for this one" and having it on, without the round trip
 * through the mouse and back to the caret.
 *
 * It deliberately does not run anything the composer could not already do.
 * A `/` command that sent a message, deleted a conversation, or spent an
 * allowance would be a destructive action one keystroke away from an ordinary
 * typo, and `/` is a character people begin real sentences with.
 */
export interface SlashCommand {
  /** Typed after the slash. Lowercase, no spaces. */
  name: string
  /** One line, shown beside the name. */
  hint: string
  /** Other spellings that should find this. */
  aliases?: string[]
  /** Shown on the right when the command reflects a state. */
  state?: string
  run: () => void
}

/**
 * Whether what has been typed is a command being written.
 *
 * A single leading slash and no whitespace yet. The whitespace rule is what
 * keeps the palette out of the way of ordinary prose: "/usr/local/bin is on my
 * PATH" opens it on the first character and closes it on the space, and
 * "and/or" never opens it at all because the slash is not at the start.
 */
export function commandQuery(text: string): string | null {
  if (!text.startsWith('/')) return null
  const rest = text.slice(1)
  if (/\s/.test(rest)) return null
  return rest.toLowerCase()
}

/** Commands matching what has been typed so far, best first. */
export function matchCommands(commands: SlashCommand[], query: string): SlashCommand[] {
  if (!query) return commands

  const scored = commands
    .map((command) => {
      const names = [command.name, ...(command.aliases ?? [])]
      // A prefix match is what someone typing expects to be offered first; a
      // match in the middle of the word is a rescue, not a suggestion.
      const prefix = names.some((name) => name.startsWith(query))
      const contains = prefix || names.some((name) => name.includes(query))
      return { command, rank: prefix ? 0 : 1, hit: contains }
    })
    .filter((entry) => entry.hit)

  scored.sort((left, right) => left.rank - right.rank)
  return scored.map((entry) => entry.command)
}

interface SlashMenuProps {
  commands: SlashCommand[]
  query: string
  /** Which row Enter would run. Owned by the composer, which sees the keys. */
  index: number
  onHover: (index: number) => void
  onRun: (command: SlashCommand) => void
}

/**
 * The list itself.
 *
 * Keyboard state lives in the composer rather than here, because the keys
 * arrive in the textarea: a menu that owned its own selection would need to
 * steal focus, and stealing focus mid-sentence is how a palette becomes
 * something people turn off.
 */
export function SlashMenu({ commands, query, index, onHover, onRun }: SlashMenuProps) {
  if (commands.length === 0) {
    return (
      <div className="slash-menu fade-in">
        <p className="faint slash-empty">No command called “/{query}”.</p>
      </div>
    )
  }

  return (
    <div className="slash-menu fade-in" role="listbox" aria-label="Commands">
      {commands.map((command, position) => (
        <button
          key={command.name}
          className={`slash-item ${position === index ? 'is-on' : ''}`}
          role="option"
          aria-selected={position === index}
          onMouseEnter={() => onHover(position)}
          // `onMouseDown` rather than `onClick`: the textarea loses focus first
          // on a click, and the blur closes the menu before the click lands.
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

/**
 * Keyboard handling for the palette, kept with the rest of it.
 *
 * Returns `true` when the key was the palette's, so the composer knows not to
 * also treat it as typing — Enter in particular, which would otherwise send
 * `/web` to the model as a message.
 */
export function useSlashKeys(open: boolean, count: number) {
  const [index, setIndex] = useState(0)

  // A shrinking list must not leave the selection past the end of it, which is
  // what happens when someone types another character and the matches narrow.
  useEffect(() => {
    setIndex((current) => (current >= count ? Math.max(0, count - 1) : current))
  }, [count])

  useEffect(() => {
    if (!open) setIndex(0)
  }, [open])

  return { index, setIndex }
}

/**
 * The commands themselves, built from what the surface can actually do.
 *
 * Assembled by the caller rather than declared as a constant, because half of
 * them reflect state the composer holds — whether web search is on, whether
 * there is a workspace — and a command whose label lies about its state is
 * worse than no command.
 */
export function buildCommands(parts: {
  toggles: {
    id: string
    label: string
    on: boolean
    onChange: (on: boolean) => void
    /** What to call this in the palette, when the toggle's id would collide. */
    command?: string
  }[]
  go: (path: string) => void
  onClear: () => void
  onHelp: () => void
}): SlashCommand[] {
  const fromToggles: SlashCommand[] = parts.toggles.map((toggle) => ({
    name: toggle.command ?? toggle.id,
    hint: `Turn ${toggle.label.toLowerCase()} ${toggle.on ? 'off' : 'on'} for this message`,
    state: toggle.on ? 'on' : 'off',
    run: () => toggle.onChange(!toggle.on),
  }))

  const all: SlashCommand[] = [
    ...fromToggles,
    {
      name: 'models',
      hint: 'Manage and download models',
      aliases: ['model'],
      run: () => parts.go('/models'),
    },
    {
      name: 'free',
      hint: 'Free provider keys and what they have cost',
      run: () => parts.go('/free'),
    },
    {
      name: 'tools',
      hint: 'Built-in tools, skills and MCP servers',
      aliases: ['skills', 'mcp'],
      run: () => parts.go('/tools'),
    },
    {
      name: 'code',
      hint: 'Open a folder to work in',
      aliases: ['workspace'],
      run: () => parts.go('/code'),
    },
    {
      name: 'projects',
      hint: 'Projects and their standing instructions',
      run: () => parts.go('/projects'),
    },
    {
      name: 'settings',
      hint: 'Settings',
      aliases: ['config', 'prefs'],
      run: () => parts.go('/settings'),
    },
    {
      name: 'clear',
      hint: 'Empty this message box',
      run: parts.onClear,
    },
    {
      name: 'help',
      hint: 'What Kuro can do',
      aliases: ['?'],
      run: parts.onHelp,
    },
  ]

  // Two commands of one name is one command that does the wrong thing half the
  // time. The toggles are built first and win, because a surface only offers a
  // toggle it actually has, whereas the navigation entries are the same list
  // everywhere.
  const seen = new Set<string>()
  return all.filter((command) => {
    if (seen.has(command.name)) return false
    seen.add(command.name)
    return true
  })
}
