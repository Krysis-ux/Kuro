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
  commandTokenAt,
  matchCommands,
  skillsNamedIn,
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

const MAX_INPUT_HEIGHT = 320

const TEXT_ACCEPT =
  '.txt,.md,.markdown,.json,.jsonl,.csv,.tsv,.log,.xml,.yml,.yaml,.toml,.ini,.env,' +
  '.ts,.tsx,.js,.jsx,.py,.rs,.go,.java,.kt,.swift,.c,.h,.cpp,.hpp,.cs,.rb,.php,.sh,' +
  '.sql,.css,.scss,.html,.vue,.svelte'

export interface ComposerToggle {
  id: string
  label: string
  icon: React.ReactNode
  on: boolean
  title: string
  onChange: (on: boolean) => void
  command?: string
}

interface ComposerProps {
  models: InstalledModel[]
  remote: RemoteModel[]
  onSend: (content: string, skills: string[]) => void
  onStop: () => void
  isStreaming: boolean
  centred?: boolean

  selectedModel: string | null
  onSelectModel: (model: string) => void
  effort: Effort
  onEffortChange: (effort: Effort) => void
  effortNote?: string
  coding?: boolean

  toggles?: ComposerToggle[]
  leading?: React.ReactNode
  trailing?: React.ReactNode

  draftKey: string

  placeholder?: string
  hint?: React.ReactNode
  disabledReason?: string | null
}

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
  const text = useUi((state) => state.drafts[draftKey] ?? '')
  const setDraftText = useUi((state) => state.setDraft)
  const setText = useCallback(
    (value: string) => setDraftText(draftKey, value),
    [draftKey, setDraftText],
  )
  const [menuOpen, setMenuOpen] = useState(false)
  const [caret, setCaret] = useState(0)
  const [dismissed, setDismissed] = useState<string | null>(null)
  const [attachments, setAttachments] = useState<{ name: string; content: string }[]>([])
  const [attachError, setAttachError] = useState<string | null>(null)
  /*
   * There is no separate list of attached skills any more.
   *
   * There was, and it was the wrong model: picking `/rust` deleted the word and
   * put a chip above the box, so the message that reached the model said
   * nothing about Rust and the skill arrived as an unexplained block of
   * instructions with no anchor in the question. "Use /brainstorming on the
   * second half" lost the half it was about.
   *
   * The text is the record. A `/name` written anywhere in the message attaches
   * that skill and stays where it was written, so the model reads the request
   * and the reason for the guidance in the same sentence — and typing one by
   * hand works as well as picking one, which it did not before.
   */
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const fitToContent = useCallback(() => {
    const node = textareaRef.current
    if (!node || node.clientWidth === 0) return
    node.style.height = 'auto'
    node.style.height = `${Math.min(node.scrollHeight, MAX_INPUT_HEIGHT)}px`
  }, [])

  useEffect(fitToContent, [text, fitToContent])

  useEffect(() => {
    const node = textareaRef.current
    if (!node) return

    let lastWidth = node.clientWidth
    const observer = new ResizeObserver(() => {
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

    const preamble = attachments
      .map((file) => `--- ${file.name} ---\n${file.content}`)
      .join('\n\n')

    onSend(preamble ? `${preamble}\n\n${trimmed}` : trimmed, namedSkills)
    setText('')
    setAttachments([])
    setAttachError(null)
  }

  const token = commandTokenAt(text, caret)
  const query = token?.query ?? null

  const catalogue = useQuery({
    queryKey: ['tools'],
    queryFn: api.tools.overview,
    staleTime: 60_000,
  })
  const skillList = useMemo(
    () => catalogue.data?.skills.catalogue ?? [],
    [catalogue.data],
  )

  const namedSkills = useMemo(() => skillsNamedIn(text, skillList), [text, skillList])

  const commands = useMemo(
    () =>
      buildCommands({
        toggles,
        skills: skillList,
        attached: namedSkills,
      }),
    [toggles, skillList, namedSkills],
  )
  const matches = useMemo(
    () => (query === null ? [] : matchCommands(commands, query)),
    [commands, query],
  )
  const slashOpen = query !== null && matches.length > 0 && dismissed !== query
  const { index, setIndex } = useSlashKeys(slashOpen, matches.length)

  const replaceToken = (replacement: string) => {
    if (!token) return

    const before = text.slice(0, token.start)
    let after = text.slice(token.end)
    if (replacement === '' && /\s$/.test(before)) after = after.replace(/^[^\S\n]/, '')

    const next = before + replacement + after
    const position = before.length + replacement.length
    setText(next)
    setCaret(position)

    requestAnimationFrame(() => {
      const node = textareaRef.current
      if (!node) return
      node.focus()
      node.setSelectionRange(position, position)
    })
  }

  const removeSkillWord = (slug: string) => {
    const next = text
      .replace(new RegExp(`(^|[\\s([{])/${slug}\\b[^\\S\\n]?`, 'gi'), '$1')
      .replace(/[^\S\n]{2,}/g, ' ')
    setText(next)
    setCaret(Math.min(caret, next.length))
  }

  const runCommand = (command: SlashCommand) => {
    if (command.kind === 'skill') {
      replaceToken(`/${command.name} `)
    } else {
      replaceToken('')
      command.run?.()
    }
    setDismissed(null)
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
      const highlighted = matches[index]
      if (event.key === 'Tab' && highlighted) {
        event.preventDefault()
        replaceToken(`/${highlighted.name}`)
        return
      }
      if (event.key === 'Enter' && !event.shiftKey && highlighted) {
        event.preventDefault()
        runCommand(highlighted)
        return
      }
      if (event.key === 'Escape') {
        event.preventDefault()
        setDismissed(query)
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
        {namedSkills.length > 0 && (
          <div className="composer-attachments">
            {namedSkills.map((slug) => (
              <span key={slug} className="tag tag-live">
                /{slug}
                <button
                  className="attachment-remove"
                  aria-label={`Remove /${slug} from the message`}
                  onClick={() => removeSkillWord(slug)}
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
          onChange={(event) => {
            setText(event.target.value)
            setCaret(event.target.selectionStart ?? event.target.value.length)
          }}
          onKeyDown={handleKeyDown}
          onSelect={(event) => setCaret(event.currentTarget.selectionStart ?? 0)}
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

  const leaving = { state: { from: location.pathname + location.search } }

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

function looksBinary(content: string): boolean {
  const sample = content.slice(0, 4000)
  if (sample.includes("\u0000")) return true

  const replacements = (sample.match(/\uFFFD/g) ?? []).length
  return replacements > sample.length * 0.02
}
