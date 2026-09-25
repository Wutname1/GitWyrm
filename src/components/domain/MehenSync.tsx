import { useEffect, useRef } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { useBranches } from '@/hooks/useGitQueries'
import { openInMehen, useMehenOverview } from '@/hooks/useMehen'
import { commands } from '@/lib/bindings'
import { allFixKeys, newFixes } from '@/lib/mehen'
import { samePath, pathName } from '@/lib/paths'
import { keys } from '@/lib/queryKeys'
import { useActiveRepo, useWorkspaceStore } from '@/stores/workspaceStore'

/**
 * Fixes already announced, kept in this browser profile only: losing it costs
 * one repeated note, so it does not belong in settings.
 */
const SEEN_FIXES_KEY = 'gitwyrm.mehen.seenFixes'

function readSeen(): Set<string> | null {
  try {
    const raw = window.localStorage.getItem(SEEN_FIXES_KEY)
    return raw ? new Set(JSON.parse(raw) as string[]) : null
  } catch {
    return null
  }
}

function writeSeen(seen: string[]) {
  try {
    window.localStorage.setItem(SEEN_FIXES_KEY, JSON.stringify(seen))
  } catch {
    // Private storage off: the worst case is a repeated note.
  }
}

/**
 * Keeps Mehen's answer fresh while the developer lives in GitWyrm, and brings
 * news from it here. Renders nothing.
 *
 * - When GitWyrm starts or comes back into focus, Mehen runs a full check in
 *   the background if its last one is more than 12 hours old.
 * - When the open repository's commit moves and that changed its dependency
 *   files (a pull, a merge, a branch switch), Mehen checks just that repository.
 * - When a check finds a security fix that was not there before, in a
 *   repository that is open here, a note says so once, with a way to fix it.
 *
 * The backend decides whether each check is needed; these calls are cheap.
 */
export function MehenSync() {
  const qc = useQueryClient()
  const repo = useActiveRepo()
  const head = useBranches(repo?.id ?? null).data?.local.find((b) => b.is_head)
  const openRepos = useWorkspaceStore((s) => s.openRepos)
  const overview = useMehenOverview().data
  const openReposRef = useRef(openRepos)
  openReposRef.current = openRepos

  useEffect(() => {
    const refresh = () => void commands.mehenRefreshIfStale()
    refresh()
    window.addEventListener('focus', refresh)
    return () => window.removeEventListener('focus', refresh)
  }, [])

  useEffect(() => {
    const unlisten = listen('mehen-status-changed', () => {
      void qc.invalidateQueries({ queryKey: keys.mehenOverview() })
      void qc.invalidateQueries({ queryKey: keys.mehenPushNoteEvery() })
    })
    return () => {
      void unlisten.then((fn) => fn())
    }
  }, [qc])

  const repoId = repo?.id
  const tip = head?.tip
  useEffect(() => {
    if (repoId && tip) void commands.mehenRepoChanged(repoId)
  }, [repoId, tip])

  useEffect(() => {
    if (!overview) return
    const current = allFixKeys(overview.repos)
    const seen = readSeen()
    writeSeen(current)
    // The first time, everything is already on screen; announcing it all at
    // once would be noise.
    if (!seen) return
    const open = openReposRef.current
    for (const { repo: found, count } of newFixes(overview.repos, open.map((r) => r.path), seen)) {
      const tab = open.find((r) => samePath(r.path, found.path))
      if (!tab) continue
      toast.info(count === 1 ? `Mehen found a security fix for ${pathName(found.path)}` : `Mehen found ${count} security fixes for ${pathName(found.path)}`, {
        description: 'Each one has a fixed version you can move to.',
        duration: 10000,
        action: overview.can_open ? { label: 'Fix in Mehen', onClick: () => void openInMehen(tab.id, true) } : undefined,
      })
    }
  }, [overview])

  return null
}
