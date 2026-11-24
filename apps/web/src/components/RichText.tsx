import { Fragment, type ReactNode } from 'react'


const PATTERN = /(https?:\/\/[^\s<>]+|www\.[^\s<>]+)|(\/[A-Za-z][\w-]*)/g

function trimUrl(raw: string): { url: string; trailing: string } {
  const match = /[.,;:!?)\]}'"]+$/.exec(raw)
  if (!match) return { url: raw, trailing: '' }
  return { url: raw.slice(0, match.index), trailing: raw.slice(match.index) }
}

export function RichText({ text }: { text: string }) {
  const parts: ReactNode[] = []
  let cursor = 0

  for (const match of text.matchAll(PATTERN)) {
    const at = match.index
    if (at === undefined) continue

    const [whole, url, command] = match

    if (command !== undefined) {
      const before = at === 0 ? '' : text[at - 1]
      if (before !== '' && !/\s|[([{]/.test(before as string)) continue

      if (text[at + whole.length] === '/') continue
    }

    if (at > cursor) parts.push(text.slice(cursor, at))

    if (url !== undefined) {
      const { url: href, trailing } = trimUrl(url)
      parts.push(
        <a
          key={`${at}-link`}
          className="rich-link"
          href={href.startsWith('www.') ? `https://${href}` : href}
          target="_blank"
          rel="noopener noreferrer"
        >
          {href}
        </a>,
      )
      if (trailing) parts.push(trailing)
    } else {
      parts.push(
        <span key={`${at}-cmd`} className="rich-command">
          {command}
        </span>,
      )
    }

    cursor = at + whole.length
  }

  if (cursor < text.length) parts.push(text.slice(cursor))

  return (
    <>
      {parts.map((part, index) => (
        <Fragment key={index}>{part}</Fragment>
      ))}
    </>
  )
}
