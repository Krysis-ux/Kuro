import { Fragment, type ReactNode } from 'react'

/**
 * `/commands` and URLs, in plain text that is not markdown.
 *
 * A user's own message is stored and shown verbatim — it is not rendered as
 * markdown, because someone typing `*` or `#` in a question means the character
 * rather than the formatting. That left two things invisible that ought not to
 * be: a `/rust` that Kuro had recognised looked exactly like one it had not,
 * and a URL that had been read looked exactly like one that had only been
 * mentioned.
 *
 * So this marks both and changes nothing else. It is not a markdown renderer
 * and must not become one.
 */

/**
 * One pass over the text, matching a URL or a slash-word.
 *
 * Written as one alternation rather than two passes so the matches cannot
 * overlap — `https://example.com/rust` is a URL, and the `/rust` inside it is
 * part of that URL rather than a command sitting in one.
 */
const PATTERN = /(https?:\/\/[^\s<>]+|www\.[^\s<>]+)|(\/[A-Za-z][\w-]*)/g

/** Trailing punctuation belongs to the sentence, not to the URL in it. */
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

    // A command is a *word* starting with a slash. Checked here rather than
    // with a lookbehind in the pattern, which Safari only learned recently.
    if (command !== undefined) {
      const before = at === 0 ? '' : text[at - 1]
      if (before !== '' && !/\s|[([{]/.test(before as string)) continue

      // `/tmp/cache` is a path, and a path is the single most likely thing to
      // appear in a message about code. A command never has a second slash, so
      // one following the word is enough to tell them apart.
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
