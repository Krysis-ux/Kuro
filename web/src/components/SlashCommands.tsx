import { useEffect, useState } from 'react'

/**
 * The `/` palette.
 *
 * It began as a shortcut to other pages, and that was the wrong idea. A command
 * that navigates away abandons the half-written message it was typed into —
 * which is the opposite of useful, since the reason to reach for `/` mid-sentence
 * is almost always that the sentence told you what you needed.
 *
 * So nothing here leaves the page. A command either switches something on for
 * this message or attaches expertise to it: `/rust` puts the Rust guidance in
 * front of the model for this turn, `/security` does the same for security
 * review, `/web` turns on search. The message stays where it is and gains
 * something.
 *
 * That also makes the palette the answer to "what can this thing actually do" —
 * every skill and every tool group in one list, in the place where you would
 * use them, rather than on a settings screen you have to go and find.
 */
export interface SlashCommand {
  /** Typed after the slash. Lowercase, no spaces. */
  name: string
  /** One line, shown beside the name. */
  hint: string
  /** Other spellings that should find this. */
  aliases?: string[]
  /** Shown on the right — a state, or what kind of thing this is. */
  state?: string
  kind: 'toggle' | 'skill'
  /** For a skill, the slug to pin on the message. */
  slug?: string
  /** Toggles act immediately; skills attach and are removed on send. */
  run?: () => void
}

/** A `/command` being typed, and where in the text it sits. */
export interface CommandToken {
  /** What follows the slash, lowercased. */
  query: string
  /** Index of the `/`. */
  start: number
  /** Index just past the last character of the word. */
  end: number
}

/**
 * The command the caret is inside, if it is inside one.
 *
 * This used to only look at the very first character of the message, which made
 * `/` a thing you could use to *start* a message and nothing else. That is not
 * how anyone writes: the moment you realise you want the Rust guidance is
 * usually three sentences in, and by then the only way to ask for it was to
 * delete what you had written, type `/rust`, and start again.
 *
 * A command is a word that begins with a slash, so the check is on the start of
 * the word the caret is in rather than on the character before it. That is what
 * keeps it clear of ordinary prose containing slashes: in `https://example.com`
 * and `and/or` the slash is mid-word, so neither opens anything.
 */
export function commandTokenAt(text: string, caret: number): CommandToken | null {
  const position = Math.max(0, Math.min(caret, text.length))

  let start = position
  while (start > 0 && !/\s/.test(text[start - 1] as string)) start -= 1

  if (text[start] !== '/') return null

  let end = position
  while (end < text.length && !/\s/.test(text[end] as string)) end += 1

  const query = text.slice(start + 1, end)
  // No command name contains a slash, and a path is the thing most likely to be
  // pasted into a coding message. Without this, `/usr/local/bin` would open the
  // palette on every keystroke of a filename.
  if (query.includes('/')) return null

  return { query: query.toLowerCase(), start, end }
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

/** A skill as the tools API describes it. */
export interface PaletteSkill {
  slug: string
  name: string
  blurb: string
}

/**
 * The commands, built from what this surface can actually do.
 *
 * Assembled by the caller rather than declared as a constant, because all of it
 * reflects live state — which switches this surface has, which are on, and which
 * skills the build knows about. A command whose label lies about its state is
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
  skills: PaletteSkill[]
  /** Slugs already attached to this message, so the row can say so. */
  attached: string[]
  onAttach: (slug: string) => void
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
    state: parts.attached.includes(skill.slug) ? 'added' : undefined,
    kind: 'skill',
    slug: skill.slug,
    run: () => parts.onAttach(skill.slug),
  }))

  // Switches first: there are two or three of them, they change what this
  // message *does* rather than how it is answered, and burying them under forty
  // skills would make the common case the hard one.
  const all = [...fromToggles, ...fromSkills]

  // Two commands of one name is one command that does the wrong thing half the
  // time. The toggles are built first and win.
  const seen = new Set<string>()
  return all.filter((command) => {
    if (seen.has(command.name)) return false
    seen.add(command.name)
    return true
  })
}
