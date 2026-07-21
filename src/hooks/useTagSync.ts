import { useMemo } from 'react'
import { useRemotes, useRemoteTags, useTags } from '@/hooks/useGitQueries'
import { detectProvider, providerLabel } from '@/lib/remoteProvider'

/**
 * Whether a tag exists on the remote. `unknown` means we have not managed to
 * look yet -- offline, still loading, or the lookup failed. It is deliberately
 * distinct from `local`: a tag is only ever labelled "not sent" once we have
 * actually checked, never by assumption.
 */
export type TagSyncState = 'synced' | 'local' | 'unknown'

/**
 * Remote-sync status for every tag in the repo, plus the name to call the host
 * in UI copy ("GitHub", "GitLab", or a plain "the remote").
 *
 * The remote lookup is a network call, so it is opt-in via `enabled` -- pass
 * false from surfaces that are not visible.
 */
export function useTagSync(repoId: string | null, enabled = true) {
  const tags = useTags(repoId)
  const remotes = useRemotes(repoId)

  const hasRemote = (remotes.data?.length ?? 0) > 0
  const remoteTags = useRemoteTags(repoId, '', enabled && hasRemote)

  // The remote a tag operation would target, matching the backend's default:
  // `origin` when present, otherwise the only remote.
  const defaultRemote = useMemo(() => {
    const list = remotes.data ?? []
    return list.find((r) => r.name === 'origin') ?? list[0] ?? null
  }, [remotes.data])

  const hostLabel = providerLabel(detectProvider(defaultRemote?.url)) ?? 'the remote'

  const onRemote = useMemo(() => {
    if (!remoteTags.data) return null
    return new Set(remoteTags.data.map((t) => t.name))
  }, [remoteTags.data])

  const stateOf = useMemo(() => {
    return (name: string): TagSyncState => {
      if (!hasRemote || !onRemote) return 'unknown'
      return onRemote.has(name) ? 'synced' : 'local'
    }
  }, [hasRemote, onRemote])

  /** Tags we know are not on the remote. Empty while the status is unknown. */
  const localOnly = useMemo(() => {
    if (!onRemote) return []
    return (tags.data ?? []).filter((t) => !onRemote.has(t.name)).map((t) => t.name)
  }, [tags.data, onRemote])

  return {
    stateOf,
    localOnly,
    hostLabel,
    hasRemote,
    defaultRemoteName: defaultRemote?.name ?? null,
    /** True once a remote lookup has succeeded, so statuses are meaningful. */
    checked: onRemote != null,
    isChecking: remoteTags.isFetching,
  }
}
