import { useEffect, useRef, useState } from 'react'
import { CheckIcon, CopyIcon } from './icons'

const CONFIRM_MS = 1600

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

export function CodeBlock({
  text,
  children,
  className = '',
  label = 'Copy',
}: {
  text: string
  children?: React.ReactNode
  className?: string
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

function textOf(node: React.ReactNode): string {
  if (node === null || node === undefined || typeof node === 'boolean') return ''
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')

  const element = node as { props?: { children?: React.ReactNode } }
  return element.props ? textOf(element.props.children) : ''
}

function languageOf(node: React.ReactNode): string {
  const element = Array.isArray(node) ? node[0] : node
  const className = (element as { props?: { className?: string } })?.props?.className ?? ''
  return /language-([\w+-]+)/.exec(className)?.[1] ?? ''
}
