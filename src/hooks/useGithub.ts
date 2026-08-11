import { useCallback, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { commands, type GithubRepoRef, type PrSummary } from '@/lib/bindings'
import { isTauri } from '@/lib/env'
import { unwrap } from '@/lib/queryKeys'
import { classifyError } from '@/lib/errorClass'
import { log } from '@/lib/log'
import { useWorkspaceStore } from '@/stores/workspaceStore'

export const githubKeys = {
  auth: ['github-auth'] as const,
  repositories: ['github-repositories'] as const,
  slug: (repoId: string) => ['github-slug', repoId] as const,
  prs: (owner: string, repo: string) => ['github-prs', owner, repo] as const,
  issues: (owner: string, repo: string) => ['github-issues', owner, repo] as const,
  pr: (owner: string, repo: string, number: number) => ['github-pr', owner, repo, number] as const,
  issue: (owner: string, repo: string, number: number) =>
    ['github-issue', owner, repo, number] as const,
  sshKeys: ['github-ssh-keys'] as const,
}

/** Query keys for the host-agnostic provider list. */
export const hostingKeys = {
  providers: ['hosting-providers'] as const,
  repoProvider: (repoId: string) => ['hosting-repo-provider', repoId] as const,
}

/**
 * Every code host GitWyrm knows about, with each one's connection state.
 *
 * The backend owns this list, so a host added there shows up here without a
 * frontend change. Sharing `githubKeys.auth` in the invalidation path is
 * deliberate: connecting or disconnecting GitHub changes a row in this list, and
 * the two must not disagree about who is signed in.
 */
export function useHostingProviders() {
  return useQuery({
    queryKey: hostingKeys.providers,
    enabled: isTauri,
    staleTime: 5 * 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.hostingProviders()),
  })
}

/** Which host a repository's origin is on, or null when unknown. */
export function useRepoHostProvider(repoId: string | null) {
  return useQuery({
    queryKey: hostingKeys.repoProvider(repoId ?? ''),
    enabled: isTauri && repoId != null,
    staleTime: 5 * 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.repoHostProvider(repoId!)),
  })
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

/**
 * Local SSH keys paired against the ones on the signed-in GitHub account.
 *
 * Needs no extra sign-in: the `read:user` scope GitHub is already connected
 * with implicitly covers reading public keys.
 */
export function useGithubSshKeys(connected: boolean) {
  return useQuery({
    queryKey: githubKeys.sshKeys,
    enabled: isTauri && connected,
    staleTime: 5 * 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.githubSshKeyPairings()),
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

/**
 * Open pull requests for a repo.
 *
 * Deliberately not gated on being signed in: the backend reads public repos
 * anonymously, so PR links work before anyone connects GitHub. A private repo
 * comes back empty in that case, which is the honest answer -- we cannot see it.
 * The `connected` argument is kept so callers re-fetch on sign-in, when the same
 * repo may start returning results.
 */
export function useGithubPrs(slug: GithubRepoRef | null | undefined, connected: boolean) {
  return useQuery({
    queryKey: [...githubKeys.prs(slug?.owner ?? '', slug?.repo ?? ''), connected],
    enabled: isTauri && slug != null,
    staleTime: 60 * 1000,
    queryFn: async () => unwrap(await commands.githubListPrs(slug!.owner, slug!.repo)),
  })
}

/**
 * The open pull request whose source branch is `branch`, or null.
 *
 * Reuses the same PR list the GitHub panel already loads, so right-clicking a
 * branch costs no extra request. Branch names are compared as GitHub reports
 * them on `head_ref` -- no remote prefix -- so pass a plain local branch name.
 *
 * Works signed out on public repos; see [`useGithubPrs`].
 */
export function useGithubPrForBranch(
  repoId: string | null,
  branch: string | null | undefined
): PrSummary | null {
  const auth = useGithubAuth()
  const slug = useGithubSlug(repoId)
  const prs = useGithubPrs(slug.data, auth.data != null)
  if (!branch) return null
  return prs.data?.find((pr) => pr.head_ref === branch) ?? null
}

export function useGithubIssues(slug: GithubRepoRef | null | undefined, connected: boolean) {
  return useQuery({
    queryKey: githubKeys.issues(slug?.owner ?? '', slug?.repo ?? ''),
    enabled: isTauri && slug != null && connected,
    staleTime: 60 * 1000,
    queryFn: async () => unwrap(await commands.githubListIssues(slug!.owner, slug!.repo)),
  })
}

/** Open pull request and issue counts for one repository's tab badge. */
export interface RepoGithubCounts {
  prs: number
  issues: number
}

/**
 * The GitHub counts a repository tab shows, or zeroes.
 *
 * Both halves are opt-in and independently gated, because each one costs a
 * request per open repository: someone who wants pull request counts should not
 * also pay for issues. When a toggle is off the query never runs, so a user who
 * leaves both off -- the default -- makes no GitHub requests from the tab strip
 * at all.
 *
 * Issues additionally need a connected account; the API does not serve them
 * anonymously. Pull requests do come back for public repositories signed out,
 * matching [`useGithubPrs`].
 *
 * Counts are what the list endpoints return, which is capped at 50. A repo with
 * more than that shows 50, which reads as "a lot" rather than a wrong number.
 */
export function useRepoGithubCounts(repoId: string | null): RepoGithubCounts {
  const showPrs = useWorkspaceStore((s) => s.showTabPrCount)
  const showIssues = useWorkspaceStore((s) => s.showTabIssueCount)
  const auth = useGithubAuth()
  const connected = auth.data != null

  // The slug query is shared with the GitHub panel and cached for five minutes,
  // so enabling a badge does not add a second origin lookup per repo.
  const slug = useGithubSlug(showPrs || showIssues ? repoId : null)
  const owner = slug.data?.owner ?? ''
  const repo = slug.data?.repo ?? ''

  const prs = useQuery({
    queryKey: [...githubKeys.prs(owner, repo), connected],
    enabled: isTauri && showPrs && slug.data != null,
    staleTime: 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.githubListPrs(owner, repo)),
  })

  const issues = useQuery({
    queryKey: githubKeys.issues(owner, repo),
    enabled: isTauri && showIssues && connected && slug.data != null,
    staleTime: 60 * 1000,
    retry: false,
    queryFn: async () => unwrap(await commands.githubListIssues(owner, repo)),
  })

  return {
    prs: showPrs ? (prs.data?.length ?? 0) : 0,
    issues: showIssues ? (issues.data?.length ?? 0) : 0,
  }
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
      // The Integrations list carries its own copy of who is signed in, so it
      // has to be refreshed alongside the auth query or the row keeps naming an
      // account that was just disconnected.
      qc.invalidateQueries({ queryKey: hostingKeys.providers })
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
          // Keeps the Integrations row in step with the auth query; see signOut.
          qc.invalidateQueries({ queryKey: hostingKeys.providers })
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
