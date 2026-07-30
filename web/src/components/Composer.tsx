import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import type { Effort, InstalledModel } from '../lib/api'
import { friendlyModelName, quantOf } from '../lib/api'
import { useUi } from '../store/ui'
import {
  ChevronIcon,
  CloudIcon,
  FolderIcon,
  GitHubIcon,
  GlobeIcon,
  PaperclipIcon,
  PlugIcon,
  PlusIcon,
  SendIcon,
  SparkIcon,
  StopIcon,
} from './icons'

const EFFORTS: Effort[] = ['low', 'balanced', 'high', 'max']

const EFFORT_HINT: Record<Effort, string> = {
  low: 'Short answers, least compute',
  balanced: 'The default',
  high: 'Longer, more thorough answers',
  max: 'Maximum thinking and output',
}

interface ComposerProps {
  models: InstalledModel[]
  onSend: (content: string) => void
  onStop: () => void
  isStreaming: boolean
  /** Centred on an empty chat, docked once the conversation starts. */
  centred: boolean
}

export function Composer({ models, onSend, onStop, isStreaming, centred }: ComposerProps) {
  const [text, setText] = useState('')
  const [menuOpen, setMenuOpen] = useState(false)
  const [attachments, setAttachments] = useState<{ name: string; content: string }[]>([])
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const { effort, setEffort, webSearch, setWebSearch } = useUi()

  // Grow with the content instead of scrolling inside a fixed box.
  useEffect(() => {
    const node = textareaRef.current
    if (!node) return
    node.style.height = 'auto'
    node.style.height = `${Math.min(node.scrollHeight, 320)}px`
  }, [text])

  useEffect(() => {
    if (!menuOpen) return
    const close = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenuOpen(false)
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [menuOpen])

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
  }

  const handleKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      submit()
    }
  }

  const readFiles = async (files: FileList | null) => {
    if (!files) return
    const read = await Promise.all(
      Array.from(files).map(async (file) => ({
        name: file.name,
        content: await file.text(),
      })),
    )
    setAttachments((existing) => [...existing, ...read])
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

        <textarea
          ref={textareaRef}
          className="composer-input"
          placeholder="Message Kuro…"
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
              {menuOpen && <AddMenu onAttach={() => fileInputRef.current?.click()} />}
            </div>

            <button
              className={`btn btn-ghost composer-toggle ${webSearch ? 'is-on' : ''}`}
              onClick={() => setWebSearch(!webSearch)}
              title="Search the web before answering. Add a search API key in Settings to enable."
              disabled
            >
              <GlobeIcon />
              Web
            </button>

            <EffortPicker value={effort} onChange={setEffort} />
          </div>

          <div className="composer-right">
            <ModelPicker models={models} />
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
          accept=".txt,.md,.json,.csv,.log,.ts,.tsx,.js,.py,.rs,.go,.java,.c,.cpp,.h,.css,.html,.yml,.yaml,.toml"
          hidden
          onChange={(event) => {
            void readFiles(event.target.files)
            event.target.value = ''
          }}
        />
      </div>

      <p className="composer-hint faint">
        Enter to send, Shift+Enter for a new line. Models run entirely on this machine.
      </p>
    </div>
  )
}

/** The `+` menu. Items that are not built yet say so rather than failing quietly. */
function AddMenu({ onAttach }: { onAttach: () => void }) {
  const navigate = useNavigate()

  return (
    <div className="menu fade-in" role="menu">
      <button className="menu-item" onClick={onAttach} role="menuitem">
        <PaperclipIcon />
        <span>Attach files</span>
      </button>

      <button className="menu-item" onClick={() => navigate('/mcp')} role="menuitem">
        <PlugIcon />
        <span>MCP tools</span>
      </button>

      <button className="menu-item" onClick={() => navigate('/mcp')} role="menuitem">
        <GitHubIcon />
        <span>GitHub</span>
        <span className="menu-note">via MCP</span>
      </button>

      <div className="menu-separator" />

      <button className="menu-item" disabled role="menuitem">
        <FolderIcon />
        <span>Add a folder</span>
        <span className="menu-note">Soon</span>
      </button>

      <button className="menu-item" disabled role="menuitem">
        <SparkIcon />
        <span>Prompt template</span>
        <span className="menu-note">Soon</span>
      </button>

      <button className="menu-item" disabled role="menuitem">
        <CloudIcon />
        <span>Run in cloud</span>
        <span className="menu-note">Soon</span>
      </button>
    </div>
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

function ModelPicker({ models }: { models: InstalledModel[] }) {
  const { selectedModel, setSelectedModel } = useUi()
  const ready = models.filter((entry) => entry.model.status === 'ready')

  // Fall back to the only installed model so a first-time user never has to
  // choose before sending anything.
  const active = selectedModel ?? ready[0]?.model.id ?? null

  if (ready.length === 0) {
    return <span className="faint model-picker-empty">No models installed</span>
  }

  return (
    <div className="model-picker">
      <select
        value={active ?? ''}
        onChange={(event) => setSelectedModel(event.target.value)}
        aria-label="Model"
      >
        {ready.map((entry) => (
          <option key={entry.model.id} value={entry.model.id}>
            {friendlyModelName(entry.model.id)}
            {quantOf(entry.model.id) ? ` · ${quantOf(entry.model.id)}` : ''}
          </option>
        ))}
      </select>
      <ChevronIcon size={13} />
    </div>
  )
}
