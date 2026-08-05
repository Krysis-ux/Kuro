import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../lib/api'
import { CodeBlock } from './CodeBlock'
import { CloseIcon } from './icons'

/**
 * What a change actually changed.
 *
 * The changes list said "edited src/app/page.tsx, +9 −2" and stopped there,
 * which is the shape of a change rather than the change. Nine lines is either
 * fine or a disaster and the number is the same either way, so the only way to
 * find out was to open the file and try to remember what had been in it.
 *
 * Both sides have been stored since the first version of undo — undo could not
 * work without them — so this is the same data the undo button already relies
 * on, shown instead of only acted upon.
 */

/** One line of the rendered diff. */
interface DiffRow {
  kind: 'same' | 'added' | 'removed'
  /** Line number on the left, when there is one. */
  before: number | null
  /** Line number on the right, when there is one. */
  after: number | null
  text: string
}

/** How many unchanged lines to keep either side of a change. */
const CONTEXT = 3

/**
 * A line-by-line diff of two texts.
 *
 * Longest common subsequence, computed over lines. The table is O(n·m), which
 * is the reason for the size guard below rather than a cleverer algorithm: a
 * real source file is a few thousand lines and this is instant on that, while a
 * minified bundle is one line and a lockfile is not something anybody reads a
 * diff of.
 */
function diffLines(before: string, after: string): DiffRow[] {
  const left = before.split('\n')
  const right = after.split('\n')

  // Common prefix and suffix are stripped first. Almost every edit touches a
  // handful of lines in the middle of a file, so this is what keeps the table
  // small enough to be worth computing at all.
  let head = 0
  while (head < left.length && head < right.length && left[head] === right[head]) head += 1

  let tail = 0
  while (
    tail < left.length - head &&
    tail < right.length - head &&
    left[left.length - 1 - tail] === right[right.length - 1 - tail]
  ) {
    tail += 1
  }

  const midLeft = left.slice(head, left.length - tail)
  const midRight = right.slice(head, right.length - tail)

  const rows: DiffRow[] = []
  const push = (kind: DiffRow['kind'], text: string, b: number | null, a: number | null) =>
    rows.push({ kind, text, before: b, after: a })

  for (let index = 0; index < head; index += 1) {
    push('same', left[index] as string, index + 1, index + 1)
  }

  // Past this the table would be tens of millions of cells for a file nobody is
  // reading line by line anyway. Reported as a whole-block replacement, which
  // is honest about what it is rather than pretending to a line-level answer.
  const TOO_BIG = 2000
  if (midLeft.length > TOO_BIG || midRight.length > TOO_BIG) {
    midLeft.forEach((text, index) => push('removed', text, head + index + 1, null))
    midRight.forEach((text, index) => push('added', text, null, head + index + 1))
  } else {
    const table: number[][] = Array.from({ length: midLeft.length + 1 }, () =>
      new Array<number>(midRight.length + 1).fill(0),
    )
    for (let i = midLeft.length - 1; i >= 0; i -= 1) {
      for (let j = midRight.length - 1; j >= 0; j -= 1) {
        table[i]![j] =
          midLeft[i] === midRight[j]
            ? (table[i + 1]![j + 1] as number) + 1
            : Math.max(table[i + 1]![j] as number, table[i]![j + 1] as number)
      }
    }

    let i = 0
    let j = 0
    while (i < midLeft.length && j < midRight.length) {
      if (midLeft[i] === midRight[j]) {
        push('same', midLeft[i] as string, head + i + 1, head + j + 1)
        i += 1
        j += 1
      } else if ((table[i + 1]![j] as number) >= (table[i]![j + 1] as number)) {
        push('removed', midLeft[i] as string, head + i + 1, null)
        i += 1
      } else {
        push('added', midRight[j] as string, null, head + j + 1)
        j += 1
      }
    }
    while (i < midLeft.length) {
      push('removed', midLeft[i] as string, head + i + 1, null)
      i += 1
    }
    while (j < midRight.length) {
      push('added', midRight[j] as string, null, head + j + 1)
      j += 1
    }
  }

  for (let index = 0; index < tail; index += 1) {
    push(
      'same',
      left[left.length - tail + index] as string,
      left.length - tail + index + 1,
      right.length - tail + index + 1,
    )
  }

  return rows
}

/** Drop the runs of unchanged lines nobody is reading. */
function collapse(rows: DiffRow[]): (DiffRow | { kind: 'gap'; count: number })[] {
  const keep = new Set<number>()
  rows.forEach((row, index) => {
    if (row.kind === 'same') return
    for (let at = index - CONTEXT; at <= index + CONTEXT; at += 1) {
      if (at >= 0 && at < rows.length) keep.add(at)
    }
  })

  const out: (DiffRow | { kind: 'gap'; count: number })[] = []
  let skipped = 0
  rows.forEach((row, index) => {
    if (keep.has(index)) {
      if (skipped > 0) {
        out.push({ kind: 'gap', count: skipped })
        skipped = 0
      }
      out.push(row)
    } else {
      skipped += 1
    }
  })
  if (skipped > 0) out.push({ kind: 'gap', count: skipped })
  return out
}

export function DiffView({
  workspaceId,
  changeId,
  path,
  onClose,
}: {
  workspaceId: string
  changeId: string
  path: string
  onClose: () => void
}) {
  const detail = useQuery({
    queryKey: ['workspace-change', workspaceId, changeId],
    queryFn: () => api.workspaces.change(workspaceId, changeId),
  })

  const change = detail.data

  const rows = useMemo(() => {
    if (!change) return null
    // A missing snapshot is not an empty file. Both are possible and only one of
    // them can be shown as a diff.
    if (change.after === null) return null
    return collapse(diffLines(change.before ?? '', change.after))
  }, [change])

  const added = rows?.filter((row) => row.kind === 'added').length ?? 0
  const removed = rows?.filter((row) => row.kind === 'removed').length ?? 0

  return (
    <div className="diff-view">
      <div className="diff-head">
        <span className="mono diff-path">{path}</span>
        {rows && (
          <span className="diff-counts mono">
            <span className="diff-added">+{added}</span>
            <span className="diff-removed">−{removed}</span>
          </span>
        )}
        <button className="btn btn-ghost btn-icon" onClick={onClose} aria-label="Close the diff">
          <CloseIcon size={14} />
        </button>
      </div>

      {detail.isLoading && <p className="faint code-panel-note">Loading…</p>}

      {detail.isError && (
        <p className="form-error">That change could not be read back.</p>
      )}

      {change && !rows && (
        <p className="faint code-panel-note">
          This change was too large to keep a copy of, so there is nothing to compare. Undo is
          unavailable for the same reason.
        </p>
      )}

      {rows && (
        <CodeBlock
          text={rows
            .map((row) =>
              row.kind === 'gap'
                ? '…'
                : `${row.kind === 'added' ? '+' : row.kind === 'removed' ? '-' : ' '}${row.text}`,
            )
            .join('\n')}
          className="is-filled"
          label={`Copy the diff of ${path}`}
        >
          <div className="diff-body mono">
            {rows.map((row, index) =>
              row.kind === 'gap' ? (
                <div key={index} className="diff-row is-gap">
                  <span className="diff-gutter" />
                  <span className="diff-text faint">
                    {row.count} unchanged {row.count === 1 ? 'line' : 'lines'}
                  </span>
                </div>
              ) : (
                <div key={index} className={`diff-row is-${row.kind}`}>
                  <span className="diff-gutter">{row.before ?? ''}</span>
                  <span className="diff-gutter">{row.after ?? ''}</span>
                  <span className="diff-sign">
                    {row.kind === 'added' ? '+' : row.kind === 'removed' ? '−' : ' '}
                  </span>
                  <span className="diff-text">{row.text || ' '}</span>
                </div>
              ),
            )}
          </div>
        </CodeBlock>
      )}
    </div>
  )
}
