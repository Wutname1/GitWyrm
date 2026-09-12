import { useEffect, useMemo } from 'react'
import { useInfiniteQuery, useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
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

  // Something the backend could not write to the session file. Kept apart from
  // `hasMissingEvents` because the two need opposite advice: a missing event is
  // in the file and reopening the chat loads it, while an unsaved one is only
  // on screen and reopening loses it.
  const hasUnsavedEvents = (liveEntry?.unsavedExecutionIds?.length ?? 0) > 0

  const state = liveEntry?.state ?? session?.header.state ?? null

  return { ...query, session, messages, state, hasMissingEvents, hasUnsavedEvents }
}

let listenerRefCount = 0
let unlistenFn: (() => void) | null = null
/**
 * Set as soon as `listen()` is CALLED, not when it resolves.
 *
 * The ref count guarded against re-subscribing, but the guard read
 * `unlistenFn`, which is only assigned in `.then()`. A second caller mounting
 * while the first registration was still in flight therefore saw `null` and
 * started its own subscription -- and since both assign `unlistenFn`, the
 * first was overwritten and leaked, applying every event twice for the life
 * of the window. React StrictMode's double-mount is exactly that shape, so
 * this fired in development on every load.
 */
let listenerStarting = false

/**
 * Fired when a quiet re-check of the installed AI tools found something
 * different from what was already on screen. Matches
 * `commands::agent_providers::TOOLS_CHANGED_EVENT`.
 */
const TOOLS_CHANGED_EVENT = 'agent-tools-changed'

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

    if (!unlistenFn && !listenerStarting) {
      listenerStarting = true
      // The tool list is checked again quietly after being answered from what
      // GitWyrm wrote down last run, and this fires only when that check found
      // something different -- a tool installed, updated, or offering
      // different models. Every screen that lists tools shares the
      // `agentProviders` key prefix, so one invalidation reaches all of them
      // and each refetches only if it is actually mounted.
      //
      // Rides the same subscription rather than opening a second one: this
      // listener already runs exactly once per window, with the refcount
      // bookkeeping that makes that true under StrictMode, and a parallel
      // singleton would be a second copy of that same delicate thing.
      void listen(TOOLS_CHANGED_EVENT, () => {
        void qc.invalidateQueries({ queryKey: ['agentProviders'] })
      })

      void listen<AgentSessionEvent>(AGENT_SESSION_EVENT, (event) => {
        useAgentSessionStore.getState().applyEvent(event.payload)
        qc.invalidateQueries({ queryKey: keys.agentSession(event.payload.sessionId) })
        qc.invalidateQueries({ queryKey: keys.agentSessionsAll })
      })
        .then((stop) => {
          listenerStarting = false
          // Every caller unmounted while registration was in flight -- tear it
          // straight back down instead of leaking it. `listenerRefCount` is
          // the check, not a per-effect flag: one effect's cleanup running is
          // not the same as nobody being left, and a per-effect flag would
          // tear down a subscription a later mount still relies on.
          if (listenerRefCount <= 0) {
            stop()
            return
          }
          unlistenFn = stop
        })
        .catch((e) => {
          listenerStarting = false
          log.error(`agent session listener: could not subscribe: ${String(e)}`)
          // Say so. Without live events the desk keeps rendering whatever it
          // last read: a running agent looks like a finished, empty chat, and
          // nothing on screen suggests anything is wrong. A log line is not a
          // user-visible response, which house rule 1 requires.
          toast.error('GitWyrm is not receiving live updates from your agents.', {
            description: 'What you see may be out of date. Reopening the window usually fixes it.',
          })
        })
    }

    return () => {
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

/**
 * Whether one session still exists, asked of the backend rather than inferred.
 *
 * The pane-recovery check used to test membership of the session list, which is
 * filtered (unarchived only) and paged (100 at a time). Both filters produced
 * the same bug from different directions: a chat that was archived, or simply
 * older than the newest hundred, read as deleted and its pane was silently
 * swapped for a different chat.
 *
 * `agentSessionGet` answers the real question. Only `notFound` means gone --
 * `damaged` and `unavailable` describe a file that cannot be read right now,
 * which is not the same thing and must not evict a pane.
 */
export function useAgentSessionExistence(sessionId: string | null) {
  const query = useQuery({
    queryKey: keys.agentSession(sessionId ?? 'none'),
    enabled: sessionId != null,
    queryFn: async () => unwrap(await commands.agentSessionGet(sessionId!)),
  })
  return query.data
}

/**
 * Whether this chat may change files, as the engine's own tool gate decides.
 *
 * There were three answers to this question. `ProviderControl` asks the
 * backend, and its comment records an earlier version being re-derived in the
 * UI and disagreeing with the engine. `SessionSourceBanner` had a third rule --
 * "a pull request, commit or diff is read-only" -- which is not the question
 * the engine asks at all: the backend decides from the chat's *intent* and
 * whether a plan has started.
 *
 * They agree today only because of which kickoffs happen to exist. Issue
 * kickoffs already accept `fix`, so a writable chat with a read-only-looking
 * source is a caller away, and the banner is a safety label.
 *
 * Shares `ProviderControl`'s query key, so opening the picker and reading the
 * banner cannot give different answers, and the two share one fetch.
 */
export function useSessionReadOnly(sessionId: string | null) {
  const query = useQuery({
    queryKey: keys.agentProviders(sessionId ?? 'none'),
    enabled: sessionId != null,
    queryFn: async () => unwrap(await commands.agentProvidersList(sessionId!)),
  })
  return query.data?.readOnly ?? null
}
