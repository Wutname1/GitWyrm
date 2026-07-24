import { CircleDot, GitPullRequest } from 'lucide-react'

/**
 * Issues and pull requests are shown in three places (sidebar rows, the item
 * view header, the context panel). Each kind gets one icon and one color here
 * so the same thing never appears as a red dot in one place and an accent
 * chevron in another -- the icon alone should say which kind you are looking at.
 *
 * `CircleDot` is reserved for issues; pull requests always use `GitPullRequest`.
 */
export const ISSUE_COLOR = 'var(--gw-red)'
export const PR_COLOR = 'var(--gw-purple)'

export function githubItemColor(kind: 'issue' | 'pr'): string {
  return kind === 'pr' ? PR_COLOR : ISSUE_COLOR
}

/** The icon for an issue or pull request, already colored for its kind. */
export function GithubItemIcon({ kind, size = 14 }: { kind: 'issue' | 'pr'; size?: number }) {
  const Icon = kind === 'pr' ? GitPullRequest : CircleDot
  return <Icon aria-hidden size={size} style={{ color: githubItemColor(kind) }} />
}
