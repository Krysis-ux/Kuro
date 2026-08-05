import { useEffect, useRef, useState } from 'react'
import { CheckIcon, CopyIcon } from './icons'

/** How long the button stays confirmed after a copy. */
const CONFIRM_MS = 1600

/**
 * Copy to clipboard, with the outcome shown on the button.
 *
 * `navigator.clipboard` needs a secure context, which localhost counts as — but
 * a browser that refuses still has to say so somewhere, and a button that
 * silently does nothing is the worst version of this. So a refusal flips the
 * label to "Press ⌘C", which is at least true.
 */
function useCopy(): [state: 'idle' | 'done' | 'failed', copy: (text: string) => void] {
  const [state, setState] = useState<'idle' | 'done' | 'failed'>('idle')
  const timer = useRef<number | null>(null)

  useEffect(() => () => { if (timer.current) window.clearTimeout(timer.current) }, [])

  const copy = (text: string) => {
    const settle = (next: 'done' | 'failed') => {
      setState(next)
      if (timer.current) window.clearTimeout(timer.current)
      timer.current = window.setTimeout(() => setState('idle'), CONFIRM_MS)
    }

    navigator.clipboard
      ?.writeText(text)
      .then(() => settle('done'))
      .catch(() => settle('failed')) ?? settle('failed')
  }

  return [state, copy]
}

/**
 * A block of code with a copy button in its corner.
 *
 * Every block, everywhere: a fenced block in a reply, a file in the viewer, a
 * process's output. Code arrives here in order to be used somewhere else, and
 * selecting several hundred lines of it by dragging — in a pane that scrolls
 * under the cursor while you do — is the kind of small tax that makes people
 * stop using a feature without ever filing a complaint about it.
 *
 * The button sits over the block rather than in a header bar, so a block gains
 * no height for having it, and fades in on hover so a page of code is not also a
 * page of buttons. It stays reachable by keyboard regardless of hover, because
 * fading a control out is a visual convenience and must not be an access rule.
 */
export function CodeBlock({
  text,
  children,
  className = '',
  label = 'Copy',
}: {
  /** What the button puts on the clipboard. */
  text: string
  /** What is drawn. Defaults to the text itself. */
  children?: React.ReactNode
  className?: string
  /** Named when there is more than one on screen, for screen readers. */
  label?: string
}) {
  return (
    <div className={`code-surface ${className}`}>
      <CopyButton text={text} label={label} />
      {children ?? <pre className="code-surface-body mono">{text}</pre>}
    </div>
  )
}

export function CopyButton({ text, label = 'Copy' }: { text: string; label?: string }) {
  const [state, copy] = useCopy()

  return (
    <button
      type="button"
      className={`copy-button ${state === 'done' ? 'is-done' : ''}`}
      onClick={() => copy(text)}
      aria-label={label}
      title={state === 'failed' ? 'Could not reach the clipboard' : label}
    >
      {state === 'done' ? <CheckIcon size={12} /> : <CopyIcon size={12} />}
      <span className="copy-button-label">
        {state === 'done' ? 'Copied' : state === 'failed' ? 'Press ⌘C' : 'Copy'}
      </span>
    </button>
  )
}

/**
 * The `pre` renderer handed to react-markdown.
 *
 * Markdown gives us a `<pre>` wrapping a `<code>`, and the text to copy is the
 * code element's children — read out of the React tree rather than out of the
 * DOM, because reading the DOM would also pick up whatever syntax highlighting
 * put there.
 */
export function MarkdownPre({ children }: { children?: React.ReactNode }) {
  const text = textOf(children)
  const language = languageOf(children)

  return (
    <div className="code-surface is-markdown">
      <div className="code-surface-bar">
        {language && <span className="faint code-surface-lang">{language}</span>}
        <CopyButton text={text} label={`Copy this ${language || 'code'} block`} />
      </div>
      <pre>{children}</pre>
    </div>
  )
}

/** Flatten a React subtree to the text it renders. */
function textOf(node: React.ReactNode): string {
  if (node === null || node === undefined || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')

  const element = node as { props?: { children?: React.ReactNode } }
  return element.props ? textOf(element.props.children) : ''
}

/** The language off the `language-x` class react-markdown puts on the code. */
function languageOf(node: React.ReactNode): string {
  const element = Array.isArray(node) ? node[0] : node
  const className = (element as { props?: { className?: string } })?.props?.className ?? ''
  return /language-([\w+-]+)/.exec(className)?.[1] ?? ''
}
