import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import {
  api,
  isRemoteModel,
  type Effort,
  type InstalledModel,
  type RemoteModel,
} from '../lib/api'
import { useUi } from '../store/ui'
import { ModelPicker } from './ModelPicker'
import {
  buildCommands,
  commandQuery,
  matchCommands,
  SlashMenu,
  useSlashKeys,
  type SlashCommand,
} from './SlashCommands'
import { ThinkingPicker } from './ThinkingPicker'
import {
  FileIcon,
  FolderIcon,
  GlobeIcon,
  ImageIcon,
  MicIcon,
  PaperclipIcon,
  PlugIcon,
  PlusIcon,
  SendIcon,
  StopIcon,
  ToolIcon,
  VideoIcon,
} from './icons'

/** Tallest the input grows before it starts scrolling. Matches `max-height`
 *  on `.composer-input`; the two have to agree or the box scrolls early. */
const MAX_INPUT_HEIGHT = 320

/** Extensions the text path can read. Anything else needs a capable model. */
const TEXT_ACCEPT =
  '.txt,.md,.markdown,.json,.jsonl,.csv,.tsv,.log,.xml,.yml,.yaml,.toml,.ini,.env,' +
  '.ts,.tsx,.js,.jsx,.py,.rs,.go,.java,.kt,.swift,.c,.h,.cpp,.hpp,.cs,.rb,.php,.sh,' +
  '.sql,.css,.scss,.html,.vue,.svelte'

/** A switch shown to the left of the model picker. */
export interface ComposerToggle {
  id: string
  label: string
  icon: React.ReactNode
  on: boolean
  title: string
  onChange: (on: boolean) => void
  /**
   * What to call this in the `/` palette, when the id would collide with a
   * page of the same name.
   *
   * The projects toggle needs one. Its id is `projects` and so is the Projects
   * page, and the two are genuinely different things — one is "may the model
   * read my folders this turn", the other is "show me my projects" — so
   * dropping either would lose something a person will look for.
   */
  command?: string
}

interface ComposerProps {
  models: InstalledModel[]
  remote: RemoteModel[]
  /**
   * `skills` are the ones named with `/` on this message. Separate from the
   * text because they are an instruction to Kuro rather than something the
   * model should read: pasting `/rust` into the prompt would ask the model to
   * interpret a command instead of applying the guidance behind it.
   */
  onSend: (content: string, skills: string[]) => void
  onStop: () => void
  isStreaming: boolean
  /** Centred on an empty chat, docked once the conversation starts. */
  centred?: boolean

  selectedModel: string | null
  onSelectModel: (model: string) => void
  effort: Effort
  onEffortChange: (effort: Effort) => void
  /** What raising the effort buys on this surface. */
  effortNote?: string
  /** Unlocks the coding-only effort level and its wording. */
  coding?: boolean

  /** Web, memory and so on. The Code page passes none of these. */
  toggles?: ComposerToggle[]
  /** Rendered before the toggles — the Code page's mode switch goes here. */
  leading?: React.ReactNode
  /** Rendered between the toggles and the model picker. */
  trailing?: React.ReactNode

  /**
   * Where this composer's half-written message is kept.
   *
   * One key per conversation or workspace, so switching between two chats does
   * not carry one's draft into the other.
   */
  draftKey: string

  placeholder?: string
  hint?: React.ReactNode
  /** Set when sending is impossible, with the reason. Shown on the button. */
  disabledReason?: string | null
}

/**
 * The message box, shared by chat and by a coding workspace.
 *
 * It was two components, and the Code page's copy was a bare textarea with a
 * send button — no attachments, no tools menu, no effort control, and a model
 * picker bolted onto the header several inches away from it. Somebody who had
 * learned the chat composer arrived at the Code page and found that half of what
 * they knew was missing, for no reason they could see.
 *
 * What actually differs between the two surfaces is which switches sit on the
 * left, so that is the prop. Everything else — growing with the content,
 * attachments, the model picker, how hard to think — is the same control in both
 * places because it is the same decision in both places.
 */
export function Composer({
  models,
  remote,
  onSend,
  onStop,
  isStreaming,
  centred = false,
  selectedModel,
  onSelectModel,
  effort,
  onEffortChange,
  effortNote,
  coding = false,
  toggles = [],
  leading,
  trailing,
  draftKey,
  placeholder,
  hint,
  disabledReason,
}: ComposerProps) {
  // Read from the store rather than from local state, so that unmounting this
  // component — which navigating anywhere does — no longer discards what was
  // being typed.
  const text = useUi((state) => state.drafts[draftKey] ?? '')
  const setDraftText = useUi((state) => state.setDraft)
  const setText = useCallback(
    (value: string) => setDraftText(draftKey, value),
    [draftKey, setDraftText],
  )
  const [menuOpen, setMenuOpen] = useState(false)
  const [attachments, setAttachments] = useState<{ name: string; content: string }[]>([])
  const [attachError, setAttachError] = useState<string | null>(null)
  /**
   * Skills named with `/` for this message.
   *
   * Held here rather than in settings because that is the distinction: the
   * store says what Kuro *may* use, and this says what it *will*, once, for
   * this message. Cleared on send along with the text.
   */
  const [attachedSkills, setAttachedSkills] = useState<string[]>([])
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  // Grow with the content instead of scrolling inside a fixed box.
  //
  // The width check is load-bearing. A textarea asked for its `scrollHeight`
  // before layout has given it a width answers with a number that has nothing
  // to do with its contents — several hundred pixels for an empty box. Because
  // this only re-ran when the text changed, and the text of a fresh composer
  // never does, that number used to be latched in for good: an empty input
  // half the height of the window, sitting over the conversation it had
  // squeezed out of the way.
  const fitToContent = useCallback(() => {
    const node = textareaRef.current
    if (!node || node.clientWidth === 0) return
    node.style.height = 'auto'
    node.style.height = `${Math.min(node.scrollHeight, MAX_INPUT_HEIGHT)}px`
  }, [])

  useEffect(fitToContent, [text, fitToContent])

  // Re-measure when the input's width changes. That covers the window being
  // resized, and — the case that matters — a first render that happened before
  // the element had been laid out, which is what makes the measurement above
  // safe to skip rather than guess at.
  useEffect(() => {
    const node = textareaRef.current
    if (!node) return

    let lastWidth = node.clientWidth
    const observer = new ResizeObserver(() => {
      // Height changes are this effect's own doing; reacting to them would
      // loop. Only a width change can invalidate the measurement.
      if (node.clientWidth === lastWidth) return
      lastWidth = node.clientWidth
      fitToContent()
    })

    observer.observe(node)
    return () => observer.disconnect()
  }, [fitToContent])

  useEffect(() => {
    if (!menuOpen) return
    const close = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false)
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [menuOpen])

  const active = useMemo(() => {
    const ready = models.filter((entry) => entry.model.status === 'ready')
    const id = selectedModel ?? ready[0]?.model.id ?? remote[0]?.id ?? null
    return {
      id,
      capabilities: ready.find((entry) => entry.model.id === id)?.model.capabilities ?? [],
      isRemote: id ? isRemoteModel(id) : false,
    }
  }, [models, remote, selectedModel])

  const submit = () => {
    const trimmed = text.trim()
    if (!trimmed || isStreaming || disabledReason) return

    // Attached text is prepended so the model sees the file before the question.
    const preamble = attachments
      .map((file) => `--- ${file.name} ---\n${file.content}`)
      .join('\n\n')

    onSend(preamble ? `${preamble}\n\n${trimmed}` : trimmed, attachedSkills)
    setText('')
    setAttachments([])
    setAttachedSkills([])
    setAttachError(null)
  }

  // `/` commands. Open while the message is a single slash-prefixed word, which
  // is what keeps them clear of prose that merely contains a slash.
  const query = commandQuery(text)

  // Every skill this build knows about, so `/rust` finds Rust without the user
  // having gone to the store first.
  const catalogue = useQuery({
    queryKey: ['tools'],
    queryFn: api.tools.overview,
    staleTime: 60_000,
  })
  const skillList = useMemo(
    () => catalogue.data?.skills.catalogue ?? [],
    [catalogue.data],
  )

  const commands = useMemo(
    () =>
      buildCommands({
        toggles,
        skills: skillList,
        attached: attachedSkills,
        onAttach: (slug) =>
          setAttachedSkills((held) =>
            held.includes(slug) ? held.filter((have) => have !== slug) : [...held, slug],
          ),
      }),
    [toggles, skillList, attachedSkills],
  )
  const matches = useMemo(
    () => (query === null ? [] : matchCommands(commands, query)),
    [commands, query],
  )
  const slashOpen = query !== null && matches.length > 0
  const { index, setIndex } = useSlashKeys(slashOpen, matches.length)

  const runCommand = (command: SlashCommand) => {
    // Cleared first, so `/web` never ends up sent to the model as a message.
    setText('')
    command.run?.()
    textareaRef.current?.focus()
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (slashOpen) {
      if (event.key === 'ArrowDown') {
        event.preventDefault()
        setIndex((current) => (current + 1) % matches.length)
        return
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        setIndex((current) => (current - 1 + matches.length) % matches.length)
        return
      }
      // Tab completes rather than running, so a half-typed name can be checked
      // before it does anything.
      const highlighted = matches[index]
      if (event.key === 'Tab' && highlighted) {
        event.preventDefault()
        setText(`/${highlighted.name}`)
        return
      }
      if (event.key === 'Enter' && !event.shiftKey && highlighted) {
        event.preventDefault()
        runCommand(highlighted)
        return
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        setText('')
        return
      }
    }

    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      submit()
    }
  }

  const readFiles = async (files: FileList | null) => {
    if (!files) return
    setAttachError(null)

    const read: { name: string; content: string }[] = []
    const rejected: string[] = []

    for (const file of Array.from(files)) {
      const content = await file.text()
      // A file that decodes to control characters is binary, and pasting it into
      // the prompt would waste the whole context window on noise.
      if (looksBinary(content)) {
        rejected.push(file.name)
        continue
      }
      read.push({ name: file.name, content })
    }

    if (read.length > 0) setAttachments((existing) => [...existing, ...read])
    if (rejected.length > 0) {
      setAttachError(
        `${rejected.join(', ')} ${rejected.length === 1 ? 'is not' : 'are not'} readable as text.`,
      )
    }
    setMenuOpen(false)
  }

  const defaultPlaceholder = active.isRemote
    ? 'Message — this one goes to your provider…'
    : 'Message Kuro…'

  return (
    <div className={`composer-shell ${centred ? 'is-centred' : ''}`}>
      {slashOpen && (
        <SlashMenu
          commands={matches}
          query={query ?? ''}
          index={index}
          onHover={setIndex}
          onRun={runCommand}
        />
      )}

      <div className="composer">
        {attachedSkills.length > 0 && (
          // Shown as chips rather than left invisible: a `/rust` that vanished
          // into thin air would give no way to tell whether it took, and no way
          // to take it back.
          <div className="composer-attachments">
            {attachedSkills.map((slug) => (
              <span key={slug} className="tag tag-live">
                /{slug}
                <button
                  className="attachment-remove"
                  aria-label={`Remove ${slug}`}
                  onClick={() =>
                    setAttachedSkills((held) => held.filter((have) => have !== slug))
                  }
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}

        {attachments.length > 0 && (
          <div className="composer-attachments">
            {attachments.map((file, index) => (
              <span key={`${file.name}-${index}`} className="tag">
                {file.name}
                <button
                  className="attachment-remove"
                  aria-label={`Remove ${file.name}`}
                  onClick={() =>
                    setAttachments((existing) => existing.filter((_, i) => i !== index))
                  }
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}

        {attachError && <p className="composer-warning">{attachError}</p>}

        <textarea
          ref={textareaRef}
          className="composer-input"
          placeholder={placeholder ?? defaultPlaceholder}
          rows={1}
          value={text}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={handleKeyDown}
        />

        <div className="composer-actions">
          <div className="composer-left">
            <div className="menu-anchor" ref={menuRef}>
              <button
                className="btn btn-ghost btn-icon"
                aria-label="Add to this message"
                aria-expanded={menuOpen}
                onClick={() => setMenuOpen((open) => !open)}
              >
                <PlusIcon />
              </button>
              {menuOpen && (
                <AddMenu
                  capabilities={active.capabilities}
                  isRemote={active.isRemote}
                  onAttach={() => fileInputRef.current?.click()}
                />
              )}
            </div>

            {leading}

            {toggles.map((toggle) => (
              <button
                key={toggle.id}
                className={`btn btn-ghost composer-toggle ${toggle.on ? 'is-on' : ''}`}
                onClick={() => toggle.onChange(!toggle.on)}
                title={toggle.title}
                aria-pressed={toggle.on}
              >
                {toggle.icon}
                {/* Wrapped so the composer can collapse it to the icon when
                    the row runs out of room. */}
                <span className="composer-toggle-label">{toggle.label}</span>
              </button>
            ))}

            <ThinkingPicker
              value={effort}
              onChange={onEffortChange}
              note={effortNote}
              coding={coding}
            />
          </div>

          <div className="composer-right">
            {trailing}
            <ModelPicker
              installed={models}
              remote={remote}
              selected={selectedModel}
              onSelect={onSelectModel}
            />
            {isStreaming ? (
              <button className="btn btn-solid btn-icon" onClick={onStop} aria-label="Stop">
                <StopIcon />
              </button>
            ) : (
              <button
                className="btn btn-solid btn-icon"
                onClick={submit}
                disabled={!text.trim() || Boolean(disabledReason)}
                title={disabledReason ?? undefined}
                aria-label="Send"
              >
                <SendIcon />
              </button>
            )}
          </div>
        </div>

        <input
          ref={fileInputRef}
          type="file"
          multiple
          accept={TEXT_ACCEPT}
          hidden
          onChange={(event) => {
            void readFiles(event.target.files)
            event.target.value = ''
          }}
        />
      </div>

      <p className="composer-hint faint">
        {disabledReason ??
          hint ??
          (active.isRemote
            ? 'Enter to send, Shift+Enter for a new line. This model runs on your provider, not here.'
            : 'Enter to send, Shift+Enter for a new line. Models run entirely on this machine.')}
      </p>
    </div>
  )
}

/**
 * The `+` menu.
 *
 * Every entry stays visible whether or not it is usable, and an unusable one is
 * dimmed rather than hidden — a menu that changes shape between models makes a
 * capability look like a bug. The *reason* it is unusable lives in the tooltip,
 * not on the row: spelling it out inline turned this menu into a wall of
 * apologetic grey text that wrapped over three lines each.
 *
 * ## Every enabled row does something
 *
 * That is a rule now because it was broken. Images and Audio were enabled
 * whenever the chosen model was a provider model — `isRemote` alone was taken
 * as proof of the capability — and neither had a click handler at all. So on
 * any provider model they lit up, invited a click, and did nothing: no picker,
 * no error, no attachment. The bug reads as the whole application being broken,
 * because the part of it the user touched was.
 *
 * Both are off until Kuro can actually send them, and the tooltip says which
 * side the missing piece is on. A message's content is a string end to end —
 * in storage, in the history sent upstream, and on the wire — so an image has
 * nowhere to go yet regardless of which model is chosen. Saying "needs a vision
 * model" would have pointed at the wrong thing entirely: it is not the model
 * that is missing.
 */
function AddMenu({
  capabilities,
  isRemote,
  onAttach,
}: {
  capabilities: string[]
  isRemote: boolean
  onAttach: () => void
}) {
  const navigate = useNavigate()
  const location = useLocation()

  // Where this menu was opened from, carried along so the destination can
  // offer a way back. Without it, following "Tools" out of a half-written
  // message is a one-way trip through the sidebar.
  const leaving = { state: { from: location.pathname + location.search } }

  // Kept for the tooltips: whether the *model* could take an image is still
  // worth saying, even while the answer does not change whether the row works.
  const modelHandles = (capability: string) => isRemote || capabilities.includes(capability)

  const servers = useQuery({
    queryKey: ['mcp', 'servers'],
    queryFn: () => api.mcp.servers(false),
    staleTime: 30_000,
  })

  const connected = (servers.data?.servers ?? []).filter(
    (server) => server.enabled && server.status === 'connected',
  )
  const toolCount = connected.reduce(
    (total, server) => total + (server.tool_count ?? server.tools.length),
    0,
  )

  return (
    <div className="menu fade-in" role="menu">
      <div className="menu-label">Attach</div>

      <MenuItem
        icon={<FileIcon />}
        label="Text and code"
        enabled
        hint="Read into the prompt as text"
        onClick={onAttach}
      />
      <MenuItem
        icon={<ImageIcon />}
        label="Images"
        enabled={false}
        hint={
          modelHandles('vision')
            ? 'This model can read images, but Kuro cannot send them yet — a message is text from the composer to the provider.'
            : 'Kuro cannot send images yet, and this model could not read one.'
        }
      />
      <MenuItem
        icon={<MicIcon />}
        label="Audio"
        enabled={false}
        hint="Kuro cannot send audio yet — a message is text from the composer to the provider."
      />
      <MenuItem
        icon={<VideoIcon />}
        label="Video"
        enabled={false}
        hint="No local engine supports video yet"
      />
      <MenuItem
        icon={<PaperclipIcon />}
        label="PDF"
        enabled={false}
        hint="Text extraction is not built yet"
      />

      <div className="menu-separator" />
      <div className="menu-label">Tools</div>

      <MenuItem
        icon={<ToolIcon />}
        label="Tools and MCP"
        enabled
        hint="Manage built-in tools, skills and MCP servers"
        badge={toolCount > 0 ? String(toolCount) : undefined}
        onClick={() => navigate('/tools', leaving)}
      />

      {connected.slice(0, 4).map((server) => (
        <MenuItem
          key={server.id}
          icon={<PlugIcon />}
          label={server.name}
          enabled
          hint={`${server.tool_count ?? server.tools.length} tools from this server`}
          onClick={() => navigate('/tools', leaving)}
        />
      ))}

      <MenuItem
        icon={<FolderIcon />}
        label="A folder"
        enabled
        hint="Open a workspace on the Code page. Chat can read one, but only the Code page can change files."
        onClick={() => navigate('/code', leaving)}
      />
    </div>
  )
}

/**
 * A menu row.
 *
 * One line, always. The hint is a tooltip so a long explanation cannot reflow the
 * menu, and the badge is for counts, which are short enough to show inline.
 */
function MenuItem({
  icon,
  label,
  enabled,
  hint,
  badge,
  onClick,
}: {
  icon: React.ReactNode
  label: string
  enabled: boolean
  hint: string
  badge?: string
  onClick?: () => void
}) {
  return (
    <button
      className="menu-item"
      disabled={!enabled}
      role="menuitem"
      title={hint}
      onClick={onClick}
    >
      {icon}
      <span className="menu-item-label">{label}</span>
      {badge && <span className="menu-badge">{badge}</span>}
    </button>
  )
}

/**
 * The switches a chat shows.
 *
 * Two, and memory is deliberately not one of them. It is on, it stays on, and it
 * only ever touches things the user asked to be saved — so a switch for it was
 * a control nobody used occupying space under every message. It lives in
 * Settings now, next to the box for writing what you want models to know about
 * you, which is where somebody goes when they actually have an opinion about it.
 */
export function chatToggles(state: {
  webSearch: boolean
  setWebSearch: (on: boolean) => void
  projects: boolean
  setProjects: (on: boolean) => void
}): ComposerToggle[] {
  return [
    {
      id: 'web',
      label: 'Web',
      icon: <GlobeIcon />,
      on: state.webSearch,
      title: state.webSearch
        ? 'On: your question is searched for before the model answers. Queries leave this machine.'
        : 'Off: search the web before answering. Queries leave this machine.',
      onChange: state.setWebSearch,
    },
    {
      id: 'projects',
      // `/folders` rather than `/projects`, which is the page. This switch is
      // about whether the model may read the folders opened on the Code page,
      // and "folders" is both what it does and what nothing else is called.
      command: 'folders',
      label: 'Projects',
      icon: <FolderIcon />,
      on: state.projects,
      title: state.projects
        ? 'On: the model can read the folders you opened on the Code page. Reading only — chat can never change a file.'
        : 'Off: the model cannot see your coding workspaces this turn.',
      onChange: state.setProjects,
    },
  ]
}

/**
 * Whether decoded text is really binary.
 *
 * `File.text()` will happily decode a PNG into replacement characters, so the
 * check is for those and for NUL — both of which are absent from any real
 * document.
 */
function looksBinary(content: string): boolean {
  const sample = content.slice(0, 4000)
  // A NUL byte never appears in text and always appears in a binary header.
  if (sample.includes("\u0000")) return true

  // Decoding a binary file as UTF-8 leaves a field of replacement characters. A
  // couple can legitimately appear in badly encoded text, so this is a ratio
  // rather than a flat rejection.
  const replacements = (sample.match(/\uFFFD/g) ?? []).length
  return replacements > sample.length * 0.02
}
