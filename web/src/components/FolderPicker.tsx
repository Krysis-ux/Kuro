import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { api } from '../lib/api'
import { CheckIcon, ChevronIcon, FolderIcon } from './icons'

/**
 * Choosing a folder by clicking, not by typing a path.
 *
 * A browser cannot open a native folder dialog that returns a path.
 * `<input webkitdirectory>` hands over the files themselves, uploaded, with
 * their real location stripped — exactly backwards for a workspace, where the
 * path is the only thing wanted.
 *
 * So the picker is built rather than borrowed: the daemon walks its own
 * filesystem and this renders the result. The daemon is on the same machine as
 * the folders, which is what makes that honest rather than a workaround.
 *
 * Typing is still there, because somebody who knows the path should not have to
 * click through six levels to reach it — but it is the fallback rather than the
 * only option, which is the way round it was.
 */
interface FolderPickerProps {
  /** The currently chosen path, or empty. */
  value: string
  onChange: (path: string) => void
  onClose: () => void
  title?: string
}

export function FolderPicker({ value, onChange, onClose, title }: FolderPickerProps) {
  const [at, setAt] = useState<string | undefined>(value || undefined)
  const [showHidden, setShowHidden] = useState(false)

  const listing = useQuery({
    queryKey: ['fs', at ?? '~', showHidden],
    queryFn: () => api.fs.browse(at, showHidden),
    // A folder that cannot be read is a dead end, not a reason to retry.
    retry: false,
  })

  const here = listing.data

  return (
    <div className="dialog-backdrop" onMouseDown={onClose}>
      <div
        className="dialog folder-picker"
        role="dialog"
        aria-modal="true"
        aria-label={title ?? 'Choose a folder'}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="dialog-head">
          <h2>{title ?? 'Choose a folder'}</h2>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>
            Cancel
          </button>
        </header>

        <div className="folder-picker-body">
          <aside className="folder-shortcuts">
            {here?.shortcuts.map((shortcut) => (
              <button
                key={shortcut.path}
                className={`folder-shortcut ${here.path === shortcut.path ? 'is-on' : ''}`}
                onClick={() => setAt(shortcut.path)}
              >
                <FolderIcon size={13} />
                {shortcut.label}
              </button>
            ))}
          </aside>

          <div className="folder-list-side">
            <div className="folder-crumbs">
              <button
                className="btn btn-ghost btn-sm"
                disabled={!here?.parent}
                onClick={() => here?.parent && setAt(here.parent)}
                title="Up one level"
              >
                <ChevronIcon size={12} className="rotate-up" />
                Up
              </button>
              <span className="mono faint folder-crumb-path">{here?.path ?? '…'}</span>
            </div>

            {listing.isError && (
              <p className="form-error">
                That folder could not be opened. It may have been moved, or it may not be
                readable by this account.
              </p>
            )}

            {listing.isLoading && <p className="faint code-panel-note">Reading…</p>}

            {here && here.entries.length === 0 && (
              <p className="faint code-panel-note">
                Nothing inside this folder. You can still choose it.
              </p>
            )}

            <ul className="folder-list">
              {here?.entries.map((entry) => (
                <li key={entry.path}>
                  <button
                    className="folder-row"
                    onClick={() => setAt(entry.path)}
                    disabled={!entry.has_children}
                    title={
                      entry.has_children ? entry.path : `${entry.path} — nothing inside it`
                    }
                  >
                    <FolderIcon size={14} />
                    <span className="folder-row-name">{entry.name}</span>
                    {entry.has_children && <ChevronIcon size={12} />}
                  </button>
                </li>
              ))}
            </ul>

            {here?.truncated && (
              <p className="faint code-panel-note">
                Only the first few hundred folders are shown. Type the path below if the one
                you want is not here.
              </p>
            )}
          </div>
        </div>

        <footer className="folder-picker-foot">
          <label className="folder-hidden-toggle faint">
            <input
              type="checkbox"
              checked={showHidden}
              onChange={(event) => setShowHidden(event.target.checked)}
            />
            Show hidden folders
          </label>

          <div className="inline-form folder-picker-choose">
            <input
              className="input mono"
              value={here?.path ?? ''}
              placeholder="Or type a path"
              onChange={(event) => setAt(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' && here?.path) {
                  onChange(here.path)
                  onClose()
                }
              }}
            />
            <button
              className="btn btn-solid"
              disabled={!here?.path}
              onClick={() => {
                if (!here?.path) return
                onChange(here.path)
                onClose()
              }}
            >
              <CheckIcon size={14} />
              Use this folder
            </button>
          </div>
        </footer>
      </div>
    </div>
  )
}

/**
 * A path field with a Choose button beside it.
 *
 * The pairing matters: the field shows what was chosen so it can be checked or
 * corrected, and the button means nobody has to know the path in the first
 * place.
 */
export function FolderField({
  value,
  onChange,
  placeholder,
  title,
}: {
  value: string
  onChange: (path: string) => void
  placeholder?: string
  title?: string
}) {
  const [picking, setPicking] = useState(false)
  const [opening, setOpening] = useState(false)
  const [note, setNote] = useState<string | null>(null)

  /**
   * Ask the operating system first.
   *
   * The built-in list works and is nobody's idea of a folder picker — people
   * expect Finder, with their own sidebar and `⌘⇧G`. The daemon runs on this
   * machine, so it can open the real dialog; the list stays as the fallback for
   * a platform that has none, or for a dialog that opens behind the browser and
   * gets dismissed.
   */
  const choose = async () => {
    setNote(null)
    setOpening(true)
    try {
      const result = await api.fs.choose()
      if (result.path) {
        onChange(result.path)
        return
      }
      if (!result.available) {
        setPicking(true)
        return
      }
      // Cancelled. Saying nothing would look like a broken button, and opening
      // the fallback list would override a decision that was just made.
      setNote(result.reason ?? 'Nothing was chosen.')
    } catch {
      // The endpoint is missing or the daemon is older than this interface.
      setPicking(true)
    } finally {
      setOpening(false)
    }
  }

  return (
    <>
      <div className="inline-form folder-field">
        <input
          className="input mono"
          placeholder={placeholder ?? 'No folder chosen'}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
        <button className="btn btn-solid" onClick={() => void choose()} disabled={opening}>
          {opening ? <span className="spinner" /> : <FolderIcon size={14} />}
          {opening ? 'Choosing…' : 'Choose…'}
        </button>
        <button
          className="btn btn-ghost"
          onClick={() => setPicking(true)}
          title="Browse without the system dialog"
        >
          Browse
        </button>
      </div>

      {note && <p className="faint hint">{note}</p>}
      {opening && (
        <p className="faint hint">
          The folder dialog has opened — it may be behind this window.
        </p>
      )}

      {picking && (
        <FolderPicker
          value={value}
          title={title}
          onChange={onChange}
          onClose={() => setPicking(false)}
        />
      )}
    </>
  )
}
