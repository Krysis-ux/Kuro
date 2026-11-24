import { useEffect, useState } from 'react'
import { Link, NavLink, useLocation, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime } from '../lib/api'
import { useUi } from '../store/ui'
import {
  BracesIcon,
  CloseIcon,
  ComposeIcon,
  CubeIcon,
  FolderIcon,
  PanelIcon,
  SearchIcon,
  SettingsIcon,
  TrashIcon,
} from './icons'
import { Logo } from './Logo'

/** The places to go. Free, Providers, Cloud and Tools live in Settings now. */
const NAV: { to: string; label: string; icon: React.ReactNode }[] = [
  { to: '/code', label: 'Code', icon: <BracesIcon size={15} /> },
  { to: '/projects', label: 'Projects', icon: <FolderIcon size={15} /> },
  { to: '/models', label: 'Models', icon: <CubeIcon size={15} /> },
]

export function Sidebar() {
  const navigate = useNavigate()
  const location = useLocation()
  const params = useParams<{ id?: string }>()
  const queryClient = useQueryClient()
  const { sidebarOpen, toggleSidebar } = useUi()
  const [searching, setSearching] = useState(false)
  const [search, setSearch] = useState('')

  const conversations = useQuery({
    queryKey: ['conversations', search],
    queryFn: () => api.conversations.list(search || undefined),
  })
  const status = useQuery({ queryKey: ['status'], queryFn: api.status, refetchInterval: 10_000 })

  const createChat = useMutation({
    mutationFn: () => api.conversations.create(),
    onSuccess: (conversation) => {
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      navigate(`/chat/${conversation.id}`)
    },
  })

  const deleteChat = useMutation({
    mutationFn: (id: string) => api.conversations.remove(id),
    onSuccess: (_result, id) => {
      void queryClient.invalidateQueries({ queryKey: ['conversations'] })
      if (params.id === id) navigate('/')
    },
  })

  // ⌘N / Ctrl+N, where the browser lets the page have it (most keep it for a new window).
  const newChat = createChat.mutate
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && !event.shiftKey && event.key.toLowerCase() === 'n') {
        event.preventDefault()
        newChat()
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [newChat])

  // The one control that is never inside the thing it controls: in the sidebar's
  // head when open, floating over the page's corner when not.
  const toggle = (
    <button
      className="sidebar-toggle"
      onClick={toggleSidebar}
      title={sidebarOpen ? 'Hide the sidebar' : 'Show the sidebar'}
      aria-label={sidebarOpen ? 'Hide the sidebar' : 'Show the sidebar'}
      aria-expanded={sidebarOpen}
      aria-controls="sidebar"
    >
      <PanelIcon size={15} />
    </button>
  )

  if (!sidebarOpen) {
    return <div className="sidebar-rail">{toggle}</div>
  }

  const closeSearch = () => {
    setSearching(false)
    setSearch('')
  }

  return (
    <aside className="sidebar" id="sidebar">
      <div className="sidebar-head">{toggle}</div>

      <nav className="side-nav" aria-label="Main">
        <button
          className={`side-row ${location.pathname === '/' ? 'active' : ''}`}
          onClick={() => createChat.mutate()}
          disabled={createChat.isPending}
        >
          <ComposeIcon size={15} />
          <span className="side-row-label">New chat</span>
          <kbd className="side-row-trail">⌘N</kbd>
        </button>
        {NAV.map((entry) => (
          <NavLink key={entry.to} to={entry.to} className="side-row">
            {entry.icon}
            <span className="side-row-label">{entry.label}</span>
          </NavLink>
        ))}
      </nav>

      <div className="side-heading">
        <span>Recents</span>
        <button
          className="side-heading-action"
          onClick={() => (searching ? closeSearch() : setSearching(true))}
          title={searching ? 'Close search' : 'Search chats'}
          aria-label={searching ? 'Close search' : 'Search chats'}
        >
          {searching ? <CloseIcon size={12} /> : <SearchIcon size={12} />}
        </button>
      </div>

      {searching && (
        <input
          className="input side-search"
          placeholder="Search chats"
          autoFocus
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          onKeyDown={(event) => event.key === 'Escape' && closeSearch()}
        />
      )}

      <div className="chat-list">
        {conversations.data?.conversations.length === 0 && (
          <p className="faint chat-list-empty">{search ? 'Nothing matched' : 'No chats yet'}</p>
        )}

        {conversations.data?.conversations.map((conversation) => (
          <NavLink
            key={conversation.id}
            to={`/chat/${conversation.id}`}
            className="chat-item"
            title={conversation.title}
          >
            <span className="chat-item-title">{conversation.title}</span>
            <span className="chat-item-time faint">{shortTime(conversation.updated_at)}</span>
            <button
              className="chat-item-delete"
              aria-label={`Delete ${conversation.title}`}
              onClick={(event) => {
                event.preventDefault()
                event.stopPropagation()
                deleteChat.mutate(conversation.id)
              }}
            >
              <TrashIcon size={13} />
            </button>
          </NavLink>
        ))}
      </div>

      <div className="side-foot">
        <span className="side-mark">
          <Logo size={14} />
        </span>
        <span className="side-foot-name">Kuro</span>
        <span
          className={`status-dot ${status.data ? 'status-connected' : ''}`}
          title={status.data ? 'Server running' : 'Server not running'}
        />
        <Link
          to="/settings/general"
          className="side-foot-gear"
          title="Settings"
          aria-label="Settings"
        >
          <SettingsIcon size={15} />
        </Link>
      </div>
    </aside>
  )
}

/** "2h", not "2h ago": the sidebar has no room for the rest. */
function shortTime(iso: string): string {
  return relativeTime(iso).replace(/ ago$/, '').replace('just now', 'now')
}
