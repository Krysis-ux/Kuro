import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { api, isRemoteModel, type Effort, type InstalledModel, type RemoteModel } from '../lib/api'
import { useUi } from '../store/ui'
import { ModelPicker } from './ModelPicker'
import {
  BrainIcon,
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

const EFFORTS: Effort[] = ['low', 'balanced', 'high', 'max']

/** Tallest the input grows before it starts scrolling. Matches `max-height`
 *  on `.composer-input`; the two have to agree or the box scrolls early. */
const MAX_INPUT_HEIGHT = 320

const EFFORT_HINT: Record<Effort, string> = {
  low: 'Short answers, least compute',
  balanced: 'The default',
  high: 'Longer, more thorough answers',
  max: 'Maximum thinking and output',
}

/** Extensions the text path can read. Anything else needs a capable model. */
const TEXT_ACCEPT =
  '.txt,.md,.markdown,.json,.jsonl,.csv,.tsv,.log,.xml,.yml,.yaml,.toml,.ini,.env,' +
  '.ts,.tsx,.js,.jsx,.py,.rs,.go,.java,.kt,.swift,.c,.h,.cpp,.hpp,.cs,.rb,.php,.sh,' +
  '.sql,.css,.scss,.html,.vue,.svelte'

interface ComposerProps {
  models: InstalledModel[]
  remote: RemoteModel[]
  onSend: (content: string) => void
  onStop: () => void
  isStreaming: boolean
  /** Centred on an empty chat, docked once the conversation starts. */
  centred: boolean
}

export function Composer({
  models,
  remote,
  onSend,
  onStop,
  isStreaming,
  centred,
}: ComposerProps) {
  const [text, setText] = useState('')
  const [menuOpen, setMenuOpen] = useState(false)
  const [attachments, setAttachments] = useState<{ name: string; content: string }[]>([])
  const [attachError, setAttachError] = useState<string | null>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const {
    effort,
    setEffort,
    webSearch,
    setWebSearch,
    memory,
    setMemory,
    files,
    setFiles,
    selectedModel,
    setSelectedModel,
  } = useUi()

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
    if (!trimmed || isStreaming) return

    // Attached text is prepended so the model sees the file before the question.
    const preamble = attachments
      .map((file) => `--- ${file.name} ---\n${file.content}`)
      .join('\n\n')

    onSend(preamble ? `${preamble}\n\n${trimmed}` : trimmed)
    setText('')
    setAttachments([])
    setAttachError(null)
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
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

  return (
    <div className={`composer-shell ${centred ? 'is-centred' : ''}`}>
      <div className="composer">
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
          placeholder={active.isRemote ? 'Message — this one goes to your provider…' : 'Message Kuro…'}
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

            <button
              className={`btn btn-ghost composer-toggle ${webSearch ? 'is-on' : ''}`}
              onClick={() => setWebSearch(!webSearch)}
              title={
                webSearch
                  ? 'On: your question is searched for before the model answers. Queries leave this machine.'
                  : 'Off: search the web before answering. Queries leave this machine.'
              }
              aria-pressed={webSearch}
            >
              <GlobeIcon />
              Web
            </button>

            <button
              className={`btn btn-ghost composer-toggle ${memory ? 'is-on' : ''}`}
              onClick={() => setMemory(!memory)}
              title={
                memory
                  ? 'On: the model can read and save durable facts about you.'
                  : 'Off: memory is not read or written this turn.'
              }
              aria-pressed={memory}
            >
              <BrainIcon />
              Memory
            </button>

            <button
              className={`btn btn-ghost composer-toggle ${files ? 'is-on' : ''}`}
              onClick={() => setFiles(!files)}
              title={
                files
                  ? 'On: the model can use the folders granted in Tools → Files.'
                  : 'Off: the model cannot read or write any file. Grant folders in Tools → Files.'
              }
              aria-pressed={files}
            >
              <FolderIcon />
              Files
            </button>

            <EffortPicker value={effort} onChange={setEffort} />
          </div>

          <div className="composer-right">
            <ModelPicker
              installed={models}
              remote={remote}
              selected={selectedModel}
              onSelect={setSelectedModel}
            />
            {isStreaming ? (
              <button className="btn btn-solid btn-icon" onClick={onStop} aria-label="Stop">
                <StopIcon />
              </button>
            ) : (
              <button
                className="btn btn-solid btn-icon"
                onClick={submit}
                disabled={!text.trim()}
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
        {active.isRemote
          ? 'Enter to send, Shift+Enter for a new line. This model runs on your provider, not here.'
          : 'Enter to send, Shift+Enter for a new line. Models run entirely on this machine.'}
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

  // A provider's models are not described by the local capability list, so assume
  // the common case rather than disabling everything.
  const has = (capability: string) => isRemote || capabilities.includes(capability)

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
        enabled={has('vision')}
        hint="Needs a vision model"
      />
      <MenuItem
        icon={<MicIcon />}
        label="Audio"
        enabled={has('audio')}
        hint="Needs an audio model"
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
        onClick={() => navigate('/tools')}
      />

      {connected.slice(0, 4).map((server) => (
        <MenuItem
          key={server.id}
          icon={<PlugIcon />}
          label={server.name}
          enabled
          hint={`${server.tool_count ?? server.tools.length} tools from this server`}
          onClick={() => navigate('/tools')}
        />
      ))}

      <MenuItem
        icon={<FolderIcon />}
        label="A folder"
        enabled={false}
        hint="Add the Filesystem MCP server to give the model a folder"
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

function EffortPicker({ value, onChange }: { value: Effort; onChange: (effort: Effort) => void }) {
  return (
    <div className="effort" role="group" aria-label="Effort">
      {EFFORTS.map((option) => (
        <button
          key={option}
          className={`effort-step ${value === option ? 'is-on' : ''}`}
          onClick={() => onChange(option)}
          title={EFFORT_HINT[option]}
          aria-pressed={value === option}
        >
          {option}
        </button>
      ))}
    </div>
  )
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
