import { useCallback, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type GithubRepoRef } from '@/lib/bindings'
import { isTauri } from '@/lib/env'
import { unwrap } from '@/lib/queryKeys'
import { classifyError } from '@/lib/errorClass'
import { log } from '@/lib/log'

export const githubKeys = {
  auth: ['github-auth'] as const,
  repositories: ['github-repositories'] as const,
  slug: (repoId: string) => ['github-slug', repoId] as const,
  prs: (owner: string, repo: string) => ['github-prs', owner, repo] as const,
  issues: (owner: string, repo: string) => ['github-issues', owner, repo] as const,
  pr: (owner: string, repo: string, number: number) => ['github-pr', owner, repo, number] as const,
  issue: (owner: string, repo: string, number: number) =>
    ['github-issue', owner, repo, number] as const,
}

/** The signed-in GitHub login, or null when not connected. */
export function useGithubAuth() {
  return useQuery({
    queryKey: githubKeys.auth,
    enabled: isTauri,
    staleTime: 5 * 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.githubAuthStatus()),
  })
}

/** Repositories available to the signed-in account, with starred entries marked. */
export function useGithubRepositories(connected: boolean) {
  return useQuery({
    queryKey: githubKeys.repositories,
    enabled: isTauri && connected,
    staleTime: 5 * 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.githubListRepositories()),
  })
}

/** The owner/repo behind origin, or null when this repo is not on github.com. */
export function useGithubSlug(repoId: string | null) {
  return useQuery({
    queryKey: githubKeys.slug(repoId ?? ''),
    enabled: isTauri && repoId != null,
    staleTime: 5 * 60 * 1000,
    queryFn: async () => unwrap(await commands.githubRepoSlug(repoId!)),
  })
}

export function useGithubPrs(slug: GithubRepoRef | null | undefined, connected: boolean) {
  return useQuery({
    queryKey: githubKeys.prs(slug?.owner ?? '', slug?.repo ?? ''),
    enabled: isTauri && slug != null && connected,
    staleTime: 60 * 1000,
    queryFn: async () => unwrap(await commands.githubListPrs(slug!.owner, slug!.repo)),
  })
}

export function useGithubIssues(slug: GithubRepoRef | null | undefined, connected: boolean) {
  return useQuery({
    queryKey: githubKeys.issues(slug?.owner ?? '', slug?.repo ?? ''),
    enabled: isTauri && slug != null && connected,
    staleTime: 60 * 1000,
    queryFn: async () => unwrap(await commands.githubListIssues(slug!.owner, slug!.repo)),
  })
}

export function useGithubPrDetail(slug: GithubRepoRef | null | undefined, number: number | null) {
  return useQuery({
    queryKey: githubKeys.pr(slug?.owner ?? '', slug?.repo ?? '', number ?? 0),
    enabled: isTauri && slug != null && number != null,
    staleTime: 30 * 1000,
    queryFn: async () => unwrap(await commands.githubPrDetail(slug!.owner, slug!.repo, number!)),
  })
}

export function useGithubIssueDetail(
  slug: GithubRepoRef | null | undefined,
  number: number | null
) {
  return useQuery({
    queryKey: githubKeys.issue(slug?.owner ?? '', slug?.repo ?? '', number ?? 0),
    enabled: isTauri && slug != null && number != null,
    staleTime: 30 * 1000,
    queryFn: async () => unwrap(await commands.githubIssueDetail(slug!.owner, slug!.repo, number!)),
  })
}

const onError = (e: Error) => {
  const { severity, message, raw } = classifyError(e)
  log[severity === 'info' ? 'info' : severity === 'warning' ? 'warn' : 'error'](
    `github mutation failed [${severity}]: ${raw}`
  )
  if (severity === 'info') toast.info(message)
  else if (severity === 'warning') toast.warning(message)
  else toast.error(message)
}

export function useGithubMutations(slug: GithubRepoRef | null | undefined) {
  const qc = useQueryClient()
  const owner = slug?.owner ?? ''
  const repo = slug?.repo ?? ''

  const refreshItem = (kind: 'pr' | 'issue', number: number) => {
    qc.invalidateQueries({
      queryKey: kind === 'pr' ? githubKeys.pr(owner, repo, number) : githubKeys.issue(owner, repo, number),
    })
    qc.invalidateQueries({
      queryKey: kind === 'pr' ? githubKeys.prs(owner, repo) : githubKeys.issues(owner, repo),
    })
  }

  const comment = useMutation({
    mutationFn: async (v: { kind: 'pr' | 'issue'; number: number; body: string }) => {
      await unwrap(await commands.githubComment(owner, repo, v.number, v.body))
      return v
    },
    onSuccess: (v) => {
      refreshItem(v.kind, v.number)
      toast('Reply posted on GitHub')
    },
    onError,
  })

  const approvePr = useMutation({
    mutationFn: async (number: number) => {
      await unwrap(await commands.githubApprovePr(owner, repo, number))
      return number
    },
    onSuccess: (number) => {
      refreshItem('pr', number)
      toast('Approved. The author can see your review.')
    },
    onError,
  })

  const mergePr = useMutation({
    mutationFn: async (v: { number: number; method: 'merge' | 'squash' | 'rebase' }) => {
      await unwrap(await commands.githubMergePr(owner, repo, v.number, v.method))
      return v
    },
    onSuccess: (v) => {
      refreshItem('pr', v.number)
      toast(`Pull request #${v.number} was added to the project`)
    },
    onError,
  })

  const closeIssue = useMutation({
    mutationFn: async (number: number) => {
      await unwrap(await commands.githubCloseIssue(owner, repo, number))
      return number
    },
    onSuccess: (number) => {
      refreshItem('issue', number)
      toast(`Issue #${number} closed`)
    },
    onError,
  })

  const signOut = useMutation({
    mutationFn: async () => unwrap(await commands.githubSignOut()),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: githubKeys.auth })
      toast('Disconnected from GitHub')
    },
    onError,
  })

  return { comment, approvePr, mergePr, closeIssue, signOut }
}

export type GithubSignIn =
  | { state: 'idle' }
  | { state: 'starting' }
  | { state: 'waiting'; userCode: string; verificationUri: string }
  | { state: 'error'; message: string }

/** Drives the GitHub device-code sign-in: start, open browser, poll. */
export function useGithubSignIn(onComplete?: () => void) {
  const qc = useQueryClient()
  const [status, setStatus] = useState<GithubSignIn>({ state: 'idle' })
  const cancelled = useRef(false)

  const cancel = useCallback(() => {
    cancelled.current = true
    setStatus({ state: 'idle' })
  }, [])

  const start = useCallback(async () => {
    cancelled.current = false
    setStatus({ state: 'starting' })
    try {
      const info = unwrap(await commands.githubDeviceStart())
      setStatus({
        state: 'waiting',
        userCode: info.user_code,
        verificationUri: info.verification_uri,
      })
      let interval = Math.max(info.interval, 5)
      for (;;) {
        await new Promise((r) => setTimeout(r, interval * 1000))
        if (cancelled.current) return
        const poll = unwrap(await commands.githubDevicePoll(info.device_code, interval))
        if (poll.status === 'complete') {
          setStatus({ state: 'idle' })
          qc.invalidateQueries({ queryKey: githubKeys.auth })
          toast('GitHub connected')
          onComplete?.()
          return
        }
        interval = Math.max(poll.interval, 5)
      }
    } catch (e) {
      if (!cancelled.current) setStatus({ state: 'error', message: String(e) })
    }
  }, [qc, onComplete])

  return { status, start, cancel }
}
