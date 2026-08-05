import { useSyncExternalStore } from 'react'
import {
  api,
  streamEditMessage,
  streamMessage,
  OPTIMISTIC_ID_PREFIX,
  type Effort,
  type Message,
  type ToolGroup,
} from './api'
import { queryClient } from './queryClient'

/** One tool call, as the transcript watches it happen. */
export interface StreamingTool {
  name: string
  arguments: Record<string, unknown>
  state: 'running' | 'done' | 'failed'
  preview?: string
  /** When the call started, so a slow one can say how long it has been. */
  startedAt: number
}

export interface StreamingState {
  content: string
  reasoning: string
  tools: StreamingTool[]
  /** When the last event arrived. Drives the "still working" pulse. */
  touchedAt: number
}

/** Everything one conversation's turn shows on screen. */
export interface TurnState {
  /** Non-null exactly while a turn is in flight. */
  stream: StreamingState | null
  error: string | null
  notices: string[]
}

const EMPTY: TurnState = { stream: null, error: null, notices: [] }

export interface TurnRequest {
  model?: string
  effort?: Effort
  tools?: ToolGroup[]
  web_search?: boolean
  /**
   * Skills named with `/` on this message.
   *
   * Distinct from the enabled set in settings: that says what Kuro may use,
   * this says what it will, for this turn, and the orchestrator may not trim
   * one away to fit its budget.
   */
  skills?: string[]
  /**
   * The workspace this turn belongs to, when it has one.
   *
   * Only used to know which panels went stale when it finishes. It is passed
   * rather than closed over because the callback that used to do this job died
   * with the component, which is the whole bug this module exists to fix.
   */
  workspace?: string
}

/**
 * Turns in flight, and the state each is showing — outside React entirely.
 *
 * This used to be `useState` inside the page, with a `useEffect` cleanup that
 * aborted the stream on unmount. A turn is work the *server* is doing, and
 * React unmounts a component for reasons that have nothing to do with whether
 * that work should continue — opening Settings mid-answer, clicking another
 * conversation, switching from Code to Chat.
 *
 * What that cost is worth being exact about, because it is not what it looks
 * like. The server carries on generating after the client hangs up and still
 * stores the reply; measured, not assumed. What died was the *view* of it. The
 * page came back with no streaming row and no spinner — indistinguishable from
 * a turn that had stopped — and the finished reply did not appear, because the
 * one thing that would have refetched it was the cleanup that had already run
 * at abort time, seconds too early. On the Code page it was worse: the callback
 * that refreshes the file tree and the changes list died with the component, so
 * a turn finishing off-page left both showing the project as it was before it
 * ran.
 *
 * Keyed by conversation, so several can run at once and each page picks up
 * whichever belongs to what it is showing. Nothing here is persisted: a reload
 * genuinely does end the stream, because the `fetch` reading it is gone.
 */
const turns = new Map<string, TurnState>()

/** Live requests, so `stop` can reach one the component no longer holds. */
const controllers = new Map<string, AbortController>()

const listeners = new Set<() => void>()

function emit() {
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

function read(key: string | null): TurnState {
  if (key === null) return EMPTY
  return turns.get(key) ?? EMPTY
}

/** Replace one conversation's state and tell every subscriber. */
function update(key: string, change: (state: TurnState) => TurnState) {
  turns.set(key, change(turns.get(key) ?? EMPTY))
  emit()
}

/** Change the live stream, leaving errors and notices alone. */
function patchStream(key: string, change: (stream: StreamingState) => StreamingState) {
  update(key, (state) => ({
    ...state,
    stream: change(state.stream ?? emptyStream()),
  }))
}

function emptyStream(): StreamingState {
  return { content: '', reasoning: '', tools: [], touchedAt: Date.now() }
}

/** Whether this conversation has a turn running right now. */
export function isRunning(key: string | null): boolean {
  return key !== null && controllers.has(key)
}

/** How many turns are running anywhere. Drives the global activity dot. */
export function runningCount(): number {
  return controllers.size
}

/** Stop one turn. Safe to call when nothing is running. */
export function stopTurn(key: string | null) {
  if (key === null) return
  controllers.get(key)?.abort()
}

/** Forget a finished turn's error and notices, once they have been read. */
export function clearTurnError(key: string | null) {
  if (key === null) return
  update(key, (state) => ({ ...state, error: null, notices: [] }))
}

/**
 * Run one turn of a conversation.
 *
 * Deliberately a plain function rather than a hook. It is started from a
 * component and then has nothing more to do with one: it writes into the map
 * above, and whichever page happens to be mounted reads it.
 */
export async function runTurn(
  key: string,
  content: string,
  request: TurnRequest,
  editing?: string,
): Promise<void> {
  // One turn per conversation. A second send while the first is streaming would
  // otherwise interleave two replies into one bubble.
  if (controllers.has(key)) return

  update(key, () => ({ stream: emptyStream(), error: null, notices: [] }))

  queryClient.setQueryData<{ messages: Message[] }>(['messages', key], (existing) => {
    const held = existing?.messages ?? []
    const cut = editing ? held.findIndex((message) => message.id === editing) : -1
    // A cut of -1 means the row is not in the cache. Keeping everything is the
    // safe reading: the refetch that follows corrects it either way, and
    // `slice(0, -1)` would quietly drop the wrong message.
    const kept = cut === -1 ? held : held.slice(0, cut)
    return { messages: [...kept, optimisticUserMessage(key, content)] }
  })

  const controller = new AbortController()
  controllers.set(key, controller)
  emit()

  const { workspace, ...body } = request

  try {
    const events = editing
      ? streamEditMessage(key, editing, { content, ...body }, controller.signal)
      : streamMessage(key, { content, ...body }, controller.signal)

    for await (const event of events) {
      const now = Date.now()

      if (event.type === 'token') {
        patchStream(key, (stream) => ({
          ...stream,
          content: stream.content + event.content,
          touchedAt: now,
        }))
      } else if (event.type === 'reasoning') {
        patchStream(key, (stream) => ({
          ...stream,
          reasoning: stream.reasoning + event.content,
          touchedAt: now,
        }))
      } else if (event.type === 'tool_call') {
        // Shown while it runs, so a long search or a large file does not look
        // like a hang.
        patchStream(key, (stream) => ({
          ...stream,
          tools: [
            ...stream.tools,
            {
              name: event.name,
              arguments: event.arguments,
              state: 'running',
              startedAt: now,
            },
          ],
          touchedAt: now,
        }))
      } else if (event.type === 'tool_result') {
        patchStream(key, (stream) => ({
          ...stream,
          tools: resolveLast(stream.tools, event.name, event.ok, event.preview),
          touchedAt: now,
        }))
      } else if (event.type === 'notice') {
        update(key, (state) => ({ ...state, notices: [...state.notices, event.message] }))
      } else if (event.type === 'error') {
        update(key, (state) => ({ ...state, error: event.message }))
      }
    }
  } catch (caught) {
    // An abort is the user pressing stop, not a failure worth reporting.
    if (!controller.signal.aborted) {
      const message = caught instanceof Error ? caught.message : 'Something went wrong.'
      update(key, (state) => ({ ...state, error: message }))
    }
  } finally {
    controllers.delete(key)
    update(key, (state) => ({ ...state, stream: null }))

    // Replace the optimistic view with what was stored, which also brings in
    // the usage, timing and tool numbers.
    void queryClient.invalidateQueries({ queryKey: ['messages', key] })
    void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    // And the model list, because a turn is the main thing that changes it. A
    // provider that refuses is set aside on the server and the picker renders
    // that as a greyed row saying why — which was never visible, because the
    // list was fetched once and then went stale.
    void queryClient.invalidateQueries({ queryKey: ['models'] })

    if (workspace) {
      // A coding turn may have touched the project or started a server, so all
      // three panels are stale.
      void queryClient.invalidateQueries({ queryKey: ['workspace', workspace] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-tree', workspace] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-changes', workspace] })
      void queryClient.invalidateQueries({ queryKey: ['workspace-processes', workspace] })
    }
  }
}

/** Branch a conversation at a message. The original is left untouched. */
export async function forkTurn(key: string, messageId: string) {
  try {
    const branch = await api.conversations.fork(key, messageId)
    void queryClient.invalidateQueries({ queryKey: ['conversations'] })
    return branch
  } catch (caught) {
    const message =
      caught instanceof Error ? caught.message : 'Could not fork this conversation.'
    update(key, (state) => ({ ...state, error: message }))
    return null
  }
}

/**
 * Watch one conversation's turn.
 *
 * `useSyncExternalStore` rather than a `useState` mirror, because the store is
 * genuinely external now and this is the hook that exists for that: it
 * subscribes on mount, unsubscribes on unmount, and — the part that matters —
 * unmounting does nothing else at all.
 */
export function useTurn(key: string | null): TurnState {
  return useSyncExternalStore(
    subscribe,
    () => read(key),
    () => EMPTY,
  )
}

/**
 * Whether anything anywhere is still working.
 *
 * Read by the sidebar, so leaving a page does not mean losing track of the turn
 * you left behind on it.
 */
export function useAnyTurnRunning(): number {
  return useSyncExternalStore(
    subscribe,
    () => controllers.size,
    () => 0,
  )
}

/**
 * Mark the most recent matching call as finished.
 *
 * Matched from the end because parallel calls to the same tool are legal, and
 * the result that just arrived belongs to the newest one still running.
 */
function resolveLast(
  tools: StreamingTool[],
  name: string,
  ok: boolean,
  preview: string,
): StreamingTool[] {
  const index = [...tools]
    .reverse()
    .findIndex((tool) => tool.name === name && tool.state === 'running')

  if (index === -1) return tools

  const target = tools.length - 1 - index
  return tools.map((tool, position) =>
    position === target ? { ...tool, state: ok ? 'done' : 'failed', preview } : tool,
  )
}

/**
 * Stand-in for the message the server is about to store.
 *
 * Only the fields the transcript reads are filled; the real row replaces this
 * as soon as the turn finishes.
 */
function optimisticUserMessage(conversationId: string, content: string): Message {
  return {
    id: `${OPTIMISTIC_ID_PREFIX}${Date.now()}`,
    conversation_id: conversationId,
    role: 'user',
    content,
    reasoning_content: null,
    tool_calls: null,
    used_web_search: false,
    web_sources: null,
    model_id: null,
    usage_prompt_tokens: null,
    usage_completion_tokens: null,
    timing_ttft_ms: null,
    timing_total_ms: null,
    timing_tokens_per_sec: null,
    finish_reason: null,
    created_at: new Date().toISOString(),
  }
}
