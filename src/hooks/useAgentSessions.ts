import { useEffect, useMemo } from 'react'
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import {
  commands,
  type AgentSession,
  type AgentSessionEvent,
  type SessionListFilterInput,
} from '@/lib/bindings'
import type { SessionIntent } from '@/lib/bindings'
import { keys, unwrap } from '@/lib/queryKeys'
import { log } from '@/lib/log'
import { liveOverlayIsRedundant, mergeSessionMessages, useAgentSessionStore } from '@/stores/agentSessionStore'

const AGENT_SESSION_EVENT = 'agent-session-event'
const SESSION_PAGE_SIZE = 100

const DEFAULT_FILTER: SessionListFilterInput = {
  repoId: null,
  projectPath: null,
  states: [],
  sourceKinds: [],
  hasChangedFiles: null,
  archived: null,
  titleContains: null,
}

/** Stable, order-independent cache key for a filter shape. */
function filterKey(filter: SessionListFilterInput): string {
  return JSON.stringify({
    repoId: filter.repoId,
    projectPath: filter.projectPath,
    states: [...filter.states].sort(),
    sourceKinds: [...filter.sourceKinds].sort(),
    hasChangedFiles: filter.hasChangedFiles,
    archived: filter.archived,
    titleContains: filter.titleContains,
  })
}

/**
 * Paged Agent Desk session headers, newest-first, matching the ordering the
 * backend's index keeps (see `architecture.md` section 2, `sort_headers`).
 *
 * Cursor-paged rather than offset-paged because the backend command takes a
 * `cursor: string | null`: each page hands back the cursor for the next one,
 * so paging stays correct even as sessions are created between fetches.
 */
export function useAgentSessions(filter: SessionListFilterInput = DEFAULT_FILTER) {
  const key = filterKey(filter)
  return useInfiniteQuery({
    queryKey: keys.agentSessions(key),
    initialPageParam: null as string | null,
    queryFn: async ({ pageParam }) =>
      unwrap(await commands.agentSessionList(filter, pageParam, SESSION_PAGE_SIZE)),
    getNextPageParam: (lastPage) => lastPage.nextCursor,
  })
}

/** Flattened headers across every loaded page, for list rendering. */
export function useAgentSessionHeaders(filter: SessionListFilterInput = DEFAULT_FILTER) {
  const query = useAgentSessions(filter)
  const headers = useMemo(
    () => query.data?.pages.flatMap((page) => page.headers) ?? [],
    [query.data]
  )
  const diagnostics = useMemo(
    () => query.data?.pages.flatMap((page) => page.diagnostics) ?? [],
    [query.data]
  )
  return { ...query, headers, diagnostics }
}

/**
 * One session's persisted content plus its live-event overlay.
 *
 * The query is the source of truth on load and after any refetch; the store
 * (`agentSessionStore`) only ever adds messages the query has not seen yet.
 * `mergeSessionMessages` reconciles the two by `messageId` so a message that
 * arrived live and then came back in a refetch is not shown twice -- the
 * scenario task 5.4 calls out.
 */
export function useAgentSession(sessionId: string | null) {
  const query = useQuery({
    queryKey: keys.agentSession(sessionId ?? 'none'),
    enabled: sessionId != null,
    queryFn: async () => unwrap(await commands.agentSessionGet(sessionId!)),
  })

  const liveEntry = useAgentSessionStore((s) => (sessionId ? s.bySession[sessionId] : undefined))

  const session: AgentSession | null =
    query.data?.kind === 'found' ? query.data.session : null

  const messages = useMemo(
    () => mergeSessionMessages(session, liveEntry?.messages ?? []),
    [session, liveEntry?.messages]
  )

  // Drop the live overlay once the query has caught up with it. Without this
  // every streamed message stayed in two places for the life of the window
  // and `mergeSessionMessages` re-walked both arrays on every render -- the
  // exact growth `clearSession` was written to prevent and never called to.
  useEffect(() => {
    if (!sessionId) return
    if (liveOverlayIsRedundant(session, liveEntry?.messages ?? [])) {
      useAgentSessionStore.getState().clearSession(sessionId)
    }
  }, [sessionId, session, liveEntry?.messages])

  // Whether any event never arrived for this chat. Surfaced, not just
  // recorded: the backend leaves gap detection to the client precisely so it
  // can be said out loud, and a transcript that quietly renders as complete
  // while turns are missing is the thing this product promises not to do.
  const hasMissingEvents = (liveEntry?.gappedExecutionIds?.length ?? 0) > 0

  const state = liveEntry?.state ?? session?.header.state ?? null

  return { ...query, session, messages, state, hasMissingEvents }
}

let listenerRefCount = 0
let unlistenFn: (() => void) | null = null

/**
 * Subscribes to `agent-session-event` exactly once for the whole window and
 * routes every event into the live-event store, invalidating the affected
 * session's (and its list's) queries so a background tab or a fresh mount
 * picks up the change on its next read.
 *
 * Guarded with a module-level ref count rather than a plain "have I run"
 * flag: React StrictMode mounts effects twice in development, and two
 * mounted call sites (e.g. the main window and a future secondary pane) must
 * still end up with exactly one underlying `listen()` call, matching how
 * `useAiRunListener` is mounted once at the app root.
 */
export function useAgentSessionListener() {
  const qc = useQueryClient()

  useEffect(() => {
    listenerRefCount += 1

    let cancelled = false
    if (!unlistenFn) {
      void listen<AgentSessionEvent>(AGENT_SESSION_EVENT, (event) => {
        useAgentSessionStore.getState().applyEvent(event.payload)
        qc.invalidateQueries({ queryKey: keys.agentSession(event.payload.sessionId) })
        qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      })
        .then((stop) => {
          if (cancelled) {
            // Registration finished after every caller had already
            // unmounted; tear it straight back down instead of leaking it.
            stop()
            return
          }
          unlistenFn = stop
        })
        .catch((e) => log.error(`agent session listener: could not subscribe: ${String(e)}`))
    }

    return () => {
      cancelled = true
      listenerRefCount -= 1
      if (listenerRefCount <= 0 && unlistenFn) {
        unlistenFn()
        unlistenFn = null
        listenerRefCount = 0
      }
    }
    // applyEvent is a stable zustand action reference; qc is the app-wide
    // client. Neither should retrigger the subscription.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
}

/**
 * Whether a chat's purpose allows changing files at all.
 *
 * The backend owns this: `policy.rs` decides from the intent, and mode "can
 * never widen what the intent allows". The composer used to offer Plan and
 * Auto on every chat, including ones the engine launches with the write and
 * shell tools denied -- so a person could pick Auto on "Explain this issue"
 * and watch nothing happen, with no reason given.
 *
 * The answer depends only on the intent, so it never goes stale within a
 * session.
 */
export function useIntentPolicy(intent: SessionIntent | null) {
  return useQuery({
    queryKey: keys.agentIntentPolicy(intent ?? 'none'),
    enabled: intent != null,
    staleTime: Infinity,
    queryFn: async () => await commands.agentIntentPolicy(intent!),
  })
}
