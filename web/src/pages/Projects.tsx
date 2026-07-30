import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useNavigate, useParams } from 'react-router-dom'
import { api, relativeTime, type Project } from '../lib/api'
import { ModelPicker } from '../components/ModelPicker'
import {
  ChatIcon,
  FolderIcon,
  PlusIcon,
  SparkIcon,
  TrashIcon,
} from '../components/icons'

/**
 * Projects.
 *
 * The substance is the standing instructions, not the folder. "This is a Rust
 * codebase, assume the 2021 edition, never suggest adding a dependency" said once
 * and applied to every conversation in the project — which is the thing people
 * actually retype at the top of every chat.
 *
 * The grouping matters mainly because it makes those instructions findable again a
 * week later, and because it gives a set of related chats somewhere to live.
 */
export function ProjectsPage() {
  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')

  const projects = useQuery({ queryKey: ['projects'], queryFn: api.projects.list })

  const create = useMutation({
    mutationFn: () => api.projects.create({ name: name.trim() }),
    onSuccess: (result) => {
      setName('')
      setCreating(false)
      void queryClient.invalidateQueries({ queryKey: ['projects'] })
      navigate(`/projects/${result.project.id}`)
    },
  })

  const list = projects.data?.projects ?? []

  return (
    <div className="page">
      <header className="page-head">
        <h1>Projects</h1>
        <p className="muted">
          Standing instructions plus a place for related chats. What you write here is added to the
          model's brief for every conversation in the project, so you only say it once.
        </p>
      </header>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <FolderIcon size={15} />
            Your projects
          </h2>
          {!creating && (
            <button className="btn btn-solid btn-sm" onClick={() => setCreating(true)}>
              <PlusIcon size={14} />
              New project
            </button>
          )}
        </div>

        {creating && (
          <form
            className="pull-form"
            onSubmit={(event) => {
              event.preventDefault()
              if (name.trim()) create.mutate()
            }}
          >
            <input
              className="input"
              autoFocus
              placeholder="Kuro, thesis, side project…"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
            <button className="btn btn-solid" disabled={!name.trim() || create.isPending}>
              {create.isPending ? <span className="spinner" /> : <PlusIcon size={14} />}
              Create
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                setCreating(false)
                setName('')
              }}
            >
              Cancel
            </button>
          </form>
        )}

        {list.length === 0 && !creating && (
          <p className="faint panel-note">
            None yet. A project is worth making as soon as you notice yourself retyping the same
            context at the top of every chat.
          </p>
        )}

        <div className="project-grid">
          {list.map((project) => (
            <button
              key={project.id}
              className="project-card"
              onClick={() => navigate(`/projects/${project.id}`)}
            >
              <div className="project-card-head">
                <FolderIcon size={14} />
                <span className="project-card-name">{project.name}</span>
              </div>
              {project.description && (
                <p className="muted project-card-blurb">{project.description}</p>
              )}
              <div className="project-card-foot faint">
                <span>
                  {project.conversation_count}{' '}
                  {project.conversation_count === 1 ? 'chat' : 'chats'}
                </span>
                {project.instructions && (
                  <span className="tag">
                    <SparkIcon size={10} /> instructions
                  </span>
                )}
                <span>{relativeTime(project.updated_at)}</span>
              </div>
            </button>
          ))}
        </div>
      </section>
    </div>
  )
}

/** One project: its instructions, its defaults, and the chats inside it. */
export function ProjectPage() {
  const params = useParams<{ id: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const projectId = params.id as string

  const detail = useQuery({
    queryKey: ['projects', projectId],
    queryFn: () => api.projects.get(projectId),
  })
  const models = useQuery({ queryKey: ['models'], queryFn: api.models.list })

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['projects'] })
    void queryClient.invalidateQueries({ queryKey: ['conversations'] })
  }

  const update = useMutation({
    mutationFn: (patch: Parameters<typeof api.projects.update>[1]) =>
      api.projects.update(projectId, patch),
    onSuccess: refresh,
  })

  const remove = useMutation({
    mutationFn: () => api.projects.remove(projectId),
    onSuccess: () => {
      refresh()
      navigate('/projects')
    },
  })

  const startChat = useMutation({
    mutationFn: async () => {
      const project = detail.data?.project
      const conversation = await api.conversations.create(project?.model_id ?? undefined)
      await api.projects.moveConversation(conversation.id, projectId)
      return conversation
    },
    onSuccess: (conversation) => {
      refresh()
      navigate(`/chat/${conversation.id}`)
    },
  })

  const project = detail.data?.project
  const conversations = detail.data?.conversations ?? []

  if (!project) {
    return (
      <div className="page">
        <p className="faint">{detail.isError ? 'That project does not exist.' : 'Loading…'}</p>
      </div>
    )
  }

  return (
    <div className="page">
      <header className="page-head">
        <button className="link-button faint" onClick={() => navigate('/projects')}>
          ← All projects
        </button>
        <h1>{project.name}</h1>
      </header>

      <InstructionsPanel
        project={project}
        onSave={(instructions) => update.mutate({ instructions })}
        saving={update.isPending}
      />

      <section className="panel">
        <h2 className="panel-title">Defaults for new chats here</h2>
        <p className="faint panel-note">
          A project can be "the one that always uses the big model". Leave a field alone to inherit
          whatever the composer is set to.
        </p>

        <div className="field">
          <div className="field-label">
            <span>Model</span>
            <span className="faint field-hint">
              {project.model_id ?? 'Inherits the composer’s choice'}
            </span>
          </div>
          <div className="field-control project-model-picker">
            <ModelPicker
              installed={models.data?.models ?? []}
              remote={models.data?.remote ?? []}
              selected={project.model_id}
              onSelect={(modelId) => update.mutate({ modelId })}
            />
            {project.model_id && (
              <button className="link-button faint" onClick={() => update.mutate({ modelId: null })}>
                Clear
              </button>
            )}
          </div>
        </div>
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2 className="panel-title">
            <ChatIcon size={15} />
            Chats
          </h2>
          <button
            className="btn btn-solid btn-sm"
            onClick={() => startChat.mutate()}
            disabled={startChat.isPending}
          >
            {startChat.isPending ? <span className="spinner" /> : <PlusIcon size={14} />}
            New chat here
          </button>
        </div>

        {conversations.length === 0 ? (
          <p className="faint panel-note">
            No chats yet. One started here inherits the instructions above.
          </p>
        ) : (
          <div className="server-rows">
            {conversations.map((conversation) => (
              <div key={conversation.id} className="server-row">
                <button
                  className="server-row-main project-chat"
                  onClick={() => navigate(`/chat/${conversation.id}`)}
                >
                  <span className="server-row-name">{conversation.title}</span>
                  <span className="faint">{relativeTime(conversation.updated_at)}</span>
                </button>
                <div className="server-row-actions">
                  <button
                    className="link-button faint"
                    title="Move this chat out of the project"
                    onClick={async () => {
                      await api.projects.moveConversation(conversation.id, null)
                      void queryClient.invalidateQueries({ queryKey: ['projects', projectId] })
                      refresh()
                    }}
                  >
                    Remove
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <h2 className="panel-title">Delete</h2>
        <p className="faint panel-note">
          Deleting a project keeps its chats — they move back to the top level. The project is only a
          grouping.
        </p>
        <button className="btn btn-danger btn-sm" onClick={() => remove.mutate()}>
          <TrashIcon size={13} />
          Delete project
        </button>
      </section>
    </div>
  )
}

/**
 * The instructions editor.
 *
 * Saved explicitly rather than on every keystroke: these words change how every
 * conversation in the project behaves, and a half-typed sentence taking effect
 * would be a strange thing to happen silently.
 */
function InstructionsPanel({
  project,
  onSave,
  saving,
}: {
  project: Project
  onSave: (instructions: string) => void
  saving: boolean
}) {
  const [draft, setDraft] = useState(project.instructions)

  // Adopt an externally changed value, without clobbering an in-progress edit.
  useEffect(() => setDraft(project.instructions), [project.instructions])

  const dirty = draft !== project.instructions

  return (
    <section className="panel">
      <div className="panel-head">
        <h2 className="panel-title">
          <SparkIcon size={15} />
          Instructions
        </h2>
        {dirty && (
          <button className="btn btn-solid btn-sm" onClick={() => onSave(draft)} disabled={saving}>
            {saving ? <span className="spinner" /> : null}
            Save
          </button>
        )}
      </div>

      <p className="faint panel-note">
        Added to the model's brief for every chat in this project, and given precedence over Kuro's
        own guidance. Concrete beats aspirational: "assume PostgreSQL 16" earns its place, "be
        helpful" does not.
      </p>

      <textarea
        className="input project-instructions"
        rows={8}
        placeholder={
          'This is a Rust workspace on the 2021 edition.\n' +
          'Never suggest adding a dependency without saying what it replaces.\n' +
          'Prefer the standard library.'
        }
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
      />

      <div className="slider-foot">
        <span className="faint">
          {draft.trim().length > 0
            ? `${draft.trim().split(/\s+/).length} words`
            : 'Empty — the project is just a folder until you write something here.'}
        </span>
        {dirty && <span className="faint">Unsaved</span>}
      </div>
    </section>
  )
}
