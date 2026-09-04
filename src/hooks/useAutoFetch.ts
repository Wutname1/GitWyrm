import { useEffect } from 'react'
import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { commands } from '@/lib/bindings'
import { keys } from '@/lib/queryKeys'
import { log } from '@/lib/log'
import { useWorkspaceStore } from '@/stores/workspaceStore'
import {
  BACKGROUND_FETCH_INTERVAL_MS,
  FOREGROUND_FETCH_INTERVAL_MS,
  isAuthFailure,
  staggerFor,
} from '@/lib/autoFetchPolicy'

export {
  FOREGROUND_FETCH_INTERVAL_MS,
  BACKGROUND_FETCH_INTERVAL_MS,
  BACKGROUND_FETCH_STAGGER_MS,
  staggerFor,
  isAuthFailure,
} from '@/lib/autoFetchPolicy'

/**
 * Remotes whose last background fetch failed on authentication.
 *
 * A repo in here is skipped by the sweep until the user does something about
 * it. See `isAuthFailure` for why retrying is actively harmful rather than
 * merely wasteful.
 *
 * Cleared by `noteManualFetch`, so any deliberate fetch, pull or push gives a
 * repo another chance: that is the moment the user has plausibly just fixed
 * their access, and it means the pause can never become permanent.
 */
const authBlocked = new Set<string>()

/** Repos currently skipped by the sweep, for tests and diagnostics. */
export function authBlockedCount(): number {
  return authBlocked.size
}

/**
 * Delay before a newly-opened or newly-focused repo is fetched. Opening a repo
 * already triggers a burst of local git work to draw the graph; a fetch in the
 * same moment competes with it and makes the tab feel slow to open.
 */
export const FETCH_ON_OPEN_DELAY_MS = 1_500

/**
 * Repos with a fetch in flight, and when each last finished one.
 *
 * Module-scoped rather than component state on purpose. These survive the hook
 * remounting (StrictMode, hot reload) and are shared with the rest of the app,
 * so a remount cannot double-fetch and a background sweep cannot pile onto a
 * fetch the user started by hand.
 */
const inFlight = new Set<string>()
const lastFetchedAt = new Map<string, number>()
/** Repos with a first fetch already queued, so a re-render cannot queue it twice. */
const scheduled = new Set<string>()

/** Records a fetch the user started, so the timers here do not repeat it. */
export function noteManualFetch(repoId: string) {
  lastFetchedAt.set(repoId, Date.now())
  // A deliberate fetch, pull or push is the moment the user has plausibly just
  // sorted out their access, so give the sweep another chance at this repo.
  // Without this the pause would outlive the problem for the whole session.
  authBlocked.delete(repoId)
}

/** Forgets a closed repo so its ids do not accumulate for the session. */
function forget(repoId: string) {
  authBlocked.delete(repoId)
  inFlight.delete(repoId)
  lastFetchedAt.delete(repoId)
  scheduled.delete(repoId)
}

/**
 * Fetch one repo unless it is busy or was fetched recently.
 *
 * `minAgeMs` is what makes the two timers cooperate: the background sweep asks
 * for a 15-minute age, so it skips the repo you are looking at, which the
 * foreground timer is already keeping fresh every 3 minutes.
 */
async function fetchIfDue(qc: QueryClient, repoId: string, minAgeMs: number) {
  if (inFlight.has(repoId)) return
  // Auth failed here last time and nothing has changed since. Retrying would
  // run the credential helper again for a credential that cannot work.
  if (authBlocked.has(repoId)) return

  const last = lastFetchedAt.get(repoId)
  if (last !== undefined && Date.now() - last < minAgeMs) return

  inFlight.add(repoId)
  try {
    // background: true -- a credential helper must not open a login window
    // over whatever the user is doing. An unauthenticated sweep fails silently.
    const res = await commands.gitFetch(repoId, true)
    if (res.status === 'error') {
      // Expected and common: no remote, offline, or credentials not set up.
      // A background fetch the user did not ask for must stay silent -- it is
      // logged for diagnosis and otherwise ignored.
      if (isAuthFailure(res.error)) {
        authBlocked.add(repoId)
        log.info(`auto-fetch paused for ${repoId} until you fetch by hand: ${res.error}`)
      } else {
        log.info(`auto-fetch skipped for ${repoId}: ${res.error}`)
      }
      return
    }
    // A success clears any earlier block: access evidently works again.
    authBlocked.delete(repoId)

    // Remote-tracking refs only. A background fetch must not call
    // trimLogToFirstPage: that would snap a scrolled graph back to the top
    // under the user's cursor, with nothing on screen explaining why.
    qc.invalidateQueries({ queryKey: keys.branches(repoId) })
    qc.invalidateQueries({ queryKey: keys.remotes(repoId) })
    qc.invalidateQueries({ queryKey: keys.tags(repoId) })
    qc.invalidateQueries({ queryKey: keys.logAll(repoId) })
  } catch (err) {
    log.info(`auto-fetch failed for ${repoId}: ${String(err)}`)
  } finally {
    inFlight.delete(repoId)
    lastFetchedAt.set(repoId, Date.now())
  }
}

/**
 * Keeps open repositories current against their remotes without the user
 * asking, so ahead/behind counts and remote branches are already right when
 * they look at a tab.
 *
 * Three triggers:
 *  - a repo becoming active (opening it, or switching to its tab),
 *  - a frequent timer on the active tab,
 *  - an infrequent, staggered sweep of every other open tab.
 */
export function useAutoFetch() {
  const qc = useQueryClient()
  const autoFetch = useWorkspaceStore((s) => s.autoFetch)
  const activeRepoId = useWorkspaceStore((s) => s.activeRepoId)

  // Drop state for repos that are no longer open. Runs regardless of the
  // setting so nothing is retained while auto-fetch is off.
  const openRepoIds = useWorkspaceStore((s) => s.openRepos.map((r) => r.id).join(' '))
  useEffect(() => {
    const open = new Set(openRepoIds ? openRepoIds.split(' ') : [])
    for (const id of [...inFlight, ...lastFetchedAt.keys(), ...scheduled]) {
      if (!open.has(id)) forget(id)
    }
  }, [openRepoIds])

  // A tab opened mid-session is fetched now rather than waiting for the next
  // sweep, which can be a quarter of an hour away. Opening several at once (a
  // tab group, or a restore) is staggered the same way a sweep is.
  //
  // This deliberately does not live in the sweep effect below: keying that on
  // the open tabs would restart its long interval every time a tab opened or
  // closed, so a busy session would never reach a sweep at all.
  useEffect(() => {
    if (!autoFetch) return

    const ids = openRepoIds ? openRepoIds.split(' ') : []
    const fresh = ids.filter((id) => !scheduled.has(id) && !lastFetchedAt.has(id))
    if (fresh.length === 0) return

    // Claim these ids before the timers are armed, and deliberately do not
    // release them in cleanup. Opening a second tab re-runs this effect, and
    // releasing would let the first tab be picked up again as "fresh" -- so a
    // burst of tab opening would keep cancelling and re-queueing the same
    // fetches and never actually run one.
    for (const id of fresh) scheduled.add(id)

    const gap = staggerFor(fresh.length)
    for (const [index, id] of fresh.entries()) {
      window.setTimeout(
        () => {
          scheduled.delete(id)
          const state = useWorkspaceStore.getState()
          if (!state.autoFetch) return
          // Closed while waiting, or now the active tab -- which has its own,
          // sooner trigger, so skipping here avoids racing it for one repo.
          if (!state.openRepos.some((r) => r.id === id)) return
          if (id === state.activeRepoId) return
          void fetchIfDue(qc, id, BACKGROUND_FETCH_INTERVAL_MS)
        },
        FETCH_ON_OPEN_DELAY_MS + index * gap
      )
    }
  }, [qc, autoFetch, openRepoIds])

  // Trigger 1 + 2: the active tab, on focus and then on a short interval.
  useEffect(() => {
    if (!autoFetch || !activeRepoId) return

    // Deferred rather than immediate so it does not compete with the local git
    // work of drawing a freshly-opened repo.
    const onOpen = window.setTimeout(() => {
      void fetchIfDue(qc, activeRepoId, FOREGROUND_FETCH_INTERVAL_MS)
    }, FETCH_ON_OPEN_DELAY_MS)

    const timer = window.setInterval(() => {
      void fetchIfDue(qc, activeRepoId, FOREGROUND_FETCH_INTERVAL_MS)
    }, FOREGROUND_FETCH_INTERVAL_MS)

    return () => {
      window.clearTimeout(onOpen)
      window.clearInterval(timer)
    }
  }, [qc, autoFetch, activeRepoId])

  // Trigger 3: every other open tab, rarely, spread out across the sweep.
  useEffect(() => {
    if (!autoFetch) return

    let pending: number[] = []

    const sweep = () => {
      // Handles from the previous sweep have all fired by now; dropping them
      // keeps this from growing for the life of the session.
      pending = []

      // Read from the store rather than closing over a snapshot: tabs open and
      // close between sweeps, and the active tab changes.
      const { openRepos, activeRepoId: active } = useWorkspaceStore.getState()
      const background = openRepos.filter((repo) => repo.id !== active)
      const gap = staggerFor(background.length)

      background.forEach((repo, index) => {
        const handle = window.setTimeout(() => {
          // Re-check on arrival: this fires up to minutes after the sweep began,
          // by which point the tab may be closed or now be the active one.
          const state = useWorkspaceStore.getState()
          if (!state.autoFetch) return
          if (repo.id === state.activeRepoId) return
          if (!state.openRepos.some((r) => r.id === repo.id)) return
          void fetchIfDue(qc, repo.id, BACKGROUND_FETCH_INTERVAL_MS)
        }, index * gap)
        pending.push(handle)
      })
    }

    // No opening sweep: tabs present at launch have never been fetched, so the
    // catch-up effect above already covers them. This interval is only for
    // keeping them fresh afterwards.
    const timer = window.setInterval(sweep, BACKGROUND_FETCH_INTERVAL_MS)
    return () => {
      window.clearInterval(timer)
      for (const handle of pending) window.clearTimeout(handle)
    }
  }, [qc, autoFetch])
}
