import type { ReactNode } from 'react'
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown'
import rehypeRaw from 'rehype-raw'
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize'
import remarkGfm from 'remark-gfm'

/**
 * A Hugging Face model card, rendered the way the Hub renders it.
 *
 * Cards mix markdown with raw HTML — centred logos, badge rows, benchmark
 * tables, demo videos — so HTML is parsed rather than escaped, and then passed
 * through the same allowlist GitHub uses, widened only for `<video>`. Nothing a
 * card author writes can run script here: no `style`, no event handlers, no
 * `javascript:` URLs.
 */

const SCHEMA = {
  ...defaultSchema,
  tagNames: [...(defaultSchema.tagNames ?? []), 'video', 'source', 'center', 'figure', 'figcaption'],
  attributes: {
    ...defaultSchema.attributes,
    '*': [...(defaultSchema.attributes?.['*'] ?? []), 'align'],
    img: [...(defaultSchema.attributes?.img ?? []), 'width', 'height'],
    video: ['src', 'poster', 'controls', 'loop', 'muted', 'playsInline', 'width', 'height'],
    source: ['src', 'type'],
  },
}

const VIDEO = /\.(mp4|webm|mov)(\?|$)/i

export function HubMarkdown({ repo, markdown }: { repo: string; markdown: string }) {
  return (
    <div className="prose hub-card">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, SCHEMA]]}
        urlTransform={(url, key) => resolveUrl(repo, url, key)}
        components={{
          a: ({ href, children }) => <CardLink href={href}>{children}</CardLink>,
          img: ({ src, alt, width, height }) =>
            // Cards embed demos as images of an .mp4 as often as with <video>.
            src && VIDEO.test(src) ? (
              <video src={src} controls playsInline preload="metadata" />
            ) : (
              <img src={src} alt={alt ?? ''} width={width} height={height} loading="lazy" />
            ),
          video: ({ src, poster, children }) => (
            <video src={src} poster={poster} controls playsInline preload="metadata">
              {children}
            </video>
          ),
        }}
      >
        {markdown}
      </ReactMarkdown>
    </div>
  )
}

function CardLink({ href, children }: { href?: string; children?: ReactNode }) {
  if (href?.startsWith('#')) return <a href={href}>{children}</a>
  return (
    <a className="rich-link" href={href} target="_blank" rel="noopener noreferrer">
      {children}
    </a>
  )
}

/**
 * Point the card's relative paths at the repository they came from.
 *
 * `figures/benchmark.png` in a card means a file in that repo, and resolving it
 * against Kuro's own address would request it from the daemon. Links go to the
 * file's page on the Hub; images and video to the raw file.
 */
function resolveUrl(repo: string, url: string, key: string): string {
  const safe = defaultUrlTransform(url)
  if (!safe || safe.startsWith('#') || /^([a-z][a-z\d+.-]*:|\/\/)/i.test(safe)) return safe
  if (safe.startsWith('/')) return `https://huggingface.co${safe}`
  const kind = key === 'href' ? 'blob' : 'resolve'
  return `https://huggingface.co/${repo}/${kind}/main/${safe.replace(/^\.\//, '')}`
}
