import { useState } from 'react'
import { NavLink, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime } from '../lib/api'
import { useUi } from '../store/ui'
import {
  GiftIcon,
  BracesIcon,
  ChatIcon,
  CloudIcon,
  CubeIcon,
  FolderIcon,
  KeyIcon,
  PanelIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  ToolIcon,
  TrashIcon,
} from './icons'
import { Logo } from './Logo'

/**
 * The navigation, as data.
 *
 * One list, used once. It was written as data with a comment saying two
 * hand-maintained copies would drift — and then the open sidebar was written out
 * by hand anyway, so there were three lists: this one, the markup, and the CSS
 * rule for an `is-icon-only` class nothing ever set. That is why the collapsed
 * rail rendered full-width labels inside 52 pixels.
 */
const NAV: { to: string; label: string; icon: React.ReactNode; end?: boolean }[] = [
  { to: '/', label: 'Chat', icon: <ChatIcon size={15} />, end: true },
  { to: '/code', label: 'Code', icon: <BracesIcon size={15} /> },
  { to: '/projects', label: 'Projects', icon: <FolderIcon size={15} /> },
  { to: '/models', label: 'Models', icon: <CubeIcon size={15} /> },
  { to: '/free', label: 'Free models', icon: <GiftIcon size={15} /> },
  { to: '/tools', label: 'Tools', icon: <ToolIcon size={15} /> },
  { to: '/providers', label: 'Providers', icon: <KeyIcon size={15} /> },
  { to: '/cloud', label: 'Cloud', icon: <CloudIcon size={15} /> },
  { to: '/settings', label: 'Settings', icon: <SettingsIcon size={15} /> },
]

export function Sidebar() {
  const navigate = useNavigate()
  const params = useParams<{ id?: string }>()
  const queryClient = useQueryClient()
  const { sidebarOpen, toggleSidebar } = useUi()
  const [search, setSearch] = useState('')

  const conversations = useQuery({
    queryKey: ['conversations', search],
    queryFn: () => api.conversations.list(search || undefined),
  })

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

  /**
   * The one control that is never inside the thing it controls.
   *
   * Rendered in both states and positioned by CSS: in the sidebar's own header
   * when it is open, and floating over the top-left corner of the page when it
   * is not. Putting it anywhere inside the collapsed sidebar is what forced the
   * old rail to exist — a sidebar that fully collapses has nowhere left to hold
   * its own reopen button, so the button has to leave first.
   */
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

  // Collapsed means collapsed: no rail, no icon strip, nothing but the page and
  // the button that brings this back. The rail was a compromise made because
  // there was nowhere to put that button, and now there is.
  if (!sidebarOpen) {
    return <div className="sidebar-rail">{toggle}</div>
  }

  return (
    <aside className="sidebar" id="sidebar">
      <div className="sidebar-head">
        <NavLink to="/" className="brand">
          <Logo size={19} />
          <span>Kuro</span>
        </NavLink>
        {toggle}
      </div>

      <button
        className="btn new-chat"
        onClick={() => createChat.mutate()}
        disabled={createChat.isPending}
      >
        <PlusIcon size={15} />
        New chat
      </button>

      <nav className="sidebar-nav">
        {NAV.map((entry) => (
          <NavLink key={entry.to} to={entry.to} end={entry.end} className="nav-item">
            {entry.icon}
            {entry.label}
          </NavLink>
        ))}
      </nav>

      <div className="sidebar-search">
        <SearchIcon size={14} className="search-icon" />
        <input
          className="input search-input"
          placeholder="Search chats"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </div>

      <div className="chat-list">
        {conversations.data?.conversations.length === 0 && (
          <p className="faint chat-list-empty">
            {search ? 'Nothing matched.' : 'No conversations yet.'}
          </p>
        )}

        {conversations.data?.conversations.map((conversation) => (
          <NavLink
            key={conversation.id}
            to={`/chat/${conversation.id}`}
            className="chat-item"
            title={conversation.title}
          >
            <span className="chat-item-title">{conversation.title}</span>
            <span className="chat-item-time faint">{relativeTime(conversation.updated_at)}</span>
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
    </aside>
  )
}
