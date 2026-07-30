import { useState } from 'react'
import { NavLink, useNavigate, useParams } from 'react-router-dom'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api, relativeTime } from '../lib/api'
import {
  ChatIcon,
  CloudIcon,
  CubeIcon,
  FolderIcon,
  KeyIcon,
  PlusIcon,
  SearchIcon,
  SettingsIcon,
  ToolIcon,
  TrashIcon,
} from './icons'
import { Logo } from './Logo'

export function Sidebar() {
  const navigate = useNavigate()
  const params = useParams<{ id?: string }>()
  const queryClient = useQueryClient()
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

  return (
    <aside className="sidebar">
      <div className="sidebar-head">
        <NavLink to="/" className="brand">
          <Logo size={19} />
          <span>Kuro</span>
        </NavLink>
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
        <NavLink to="/" end className="nav-item">
          <ChatIcon size={15} />
          Chat
        </NavLink>
        <NavLink to="/projects" className="nav-item">
          <FolderIcon size={15} />
          Projects
        </NavLink>
        <NavLink to="/models" className="nav-item">
          <CubeIcon size={15} />
          Models
        </NavLink>
        <NavLink to="/tools" className="nav-item">
          <ToolIcon size={15} />
          Tools
        </NavLink>
        <NavLink to="/providers" className="nav-item">
          <KeyIcon size={15} />
          Providers
        </NavLink>
        <NavLink to="/cloud" className="nav-item">
          <CloudIcon size={15} />
          Cloud
        </NavLink>
        <NavLink to="/settings" className="nav-item">
          <SettingsIcon size={15} />
          Settings
        </NavLink>
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
