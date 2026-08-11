import { Cloud } from 'lucide-react'
import type { SVGProps } from 'react'
import type { RemoteProvider as BackendRemoteProvider } from '@/lib/bindings'

export type RemoteProvider = 'github' | 'gitlab' | 'bitbucket' | 'azure' | 'unknown'

/**
 * Rust (`git::remote_url`) classifies more hosts than we draw glyphs for. Map
 * its vocabulary onto the four we can badge; the rest fall back to the cloud
 * icon.
 */
const PROVIDER_FROM_BACKEND: Record<BackendRemoteProvider, RemoteProvider> = {
  git_hub: 'github',
  git_lab: 'gitlab',
  bitbucket: 'bitbucket',
  azure_dev_ops: 'azure',
  gitea: 'unknown',
  source_hut: 'unknown',
  code_commit: 'unknown',
  self_hosted: 'unknown',
}

/** Bare host from a remote URL (ssh, scp-style, or https), lowercased. */
function hostOf(url: string): string | null {
  const value = url.trim()
  if (!value || /^[a-z]:[\\/]/i.test(value)) return null

  let authority: string
  if (value.includes('://')) {
    try {
      authority = new URL(value).host
    } catch {
      return null
    }
  } else {
    const scp = value.match(/^(?:[^@/]+@)?([^:/]+):(.+)$/)
    if (!scp || scp[1].length === 1) return null
    authority = scp[1]
  }

  // Drop credentials and any port so `user@host:443` can't smuggle a label in.
  const afterCredentials = authority.includes('@') ? authority.slice(authority.lastIndexOf('@') + 1) : authority
  if (afterCredentials.startsWith('[')) return null
  const bare = afterCredentials.split(':')[0]
  return bare ? bare.toLowerCase() : null
}

/**
 * Identify the hosting provider for a remote.
 *
 * Prefer passing the whole `RemoteInfo`: Rust already parsed the host in
 * `git::remote_url` and ships the answer on every remote, so the UI and the
 * backend can never disagree. The string overload re-derives it for the few
 * call sites that only hold a URL.
 *
 * Matching is per host label, never a bare substring, so `github.myco.com`
 * resolves to GitHub while `github-phishing.example.com` does not. This
 * mirrors `provider_for_host` in `git::remote_url`; keep the two in step.
 */
export function detectProvider(
  remote: string | { url: string; provider?: BackendRemoteProvider } | undefined
): RemoteProvider {
  if (!remote) return 'unknown'
  if (typeof remote !== 'string' && remote.provider) {
    return PROVIDER_FROM_BACKEND[remote.provider] ?? 'unknown'
  }

  const host = hostOf(typeof remote === 'string' ? remote : remote.url)
  if (!host) return 'unknown'

  const labels = host.split('.')
  const has = (needle: string) => labels.includes(needle)

  if (has('github')) return 'github'
  if (has('gitlab')) return 'gitlab'
  if (has('bitbucket')) return 'bitbucket'
  if (host === 'dev.azure.com' || host === 'ssh.dev.azure.com' || host === 'visualstudio.com' || host.endsWith('.visualstudio.com')) {
    return 'azure'
  }
  return 'unknown'
}

const providerNames: Record<RemoteProvider, string | null> = {
  github: 'GitHub',
  gitlab: 'GitLab',
  bitbucket: 'Bitbucket',
  azure: 'Azure DevOps',
  unknown: null,
}

/**
 * The host's brand name for use in sentences, or null when the host isn't one
 * we recognize -- callers fall back to generic wording like "the remote".
 */
export function providerLabel(provider: RemoteProvider): string | null {
  return providerNames[provider]
}

// Brand glyphs inlined as single-path SVGs (CSP-safe, no external assets).
// currentColor lets them inherit the pill's text color.
function GithubGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden {...props}>
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
    </svg>
  )
}

function GitlabGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden {...props}>
      <path d="m15.73 6.36-.02-.06-2.13-5.55a.55.55 0 0 0-.54-.35.56.56 0 0 0-.53.42L10.95 5.6H5.05L3.5.82A.55.55 0 0 0 2.96.4a.56.56 0 0 0-.53.35L.3 6.3l-.02.06a3.95 3.95 0 0 0 1.31 4.56l.01.01.02.02 3.24 2.43 1.6 1.21.98.74a.65.65 0 0 0 .78 0l.98-.74 1.6-1.21 3.26-2.44.01-.01a3.95 3.95 0 0 0 1.3-4.55Z" />
    </svg>
  )
}

function BitbucketGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden {...props}>
      <path d="M.52 1a.5.5 0 0 0-.5.58l2.17 13.17a.68.68 0 0 0 .67.57h10.3a.5.5 0 0 0 .5-.42l2.17-13.9a.5.5 0 0 0-.5-.58L.52 1Zm9.13 9.4H6.4l-.88-4.6h4.9l-.77 4.6Z" />
    </svg>
  )
}

function AzureGlyph(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden {...props}>
      <path d="M15 3.62v8.53l-3.5 2.85-5.42-1.97v1.95l-3.07-4.01 8.9.7V4.13L15 3.62ZM11.98 4.2 6.5 1v2.2L1.47 4.68.98 11l2.09-.8V5.6l3.43-1.87v9.29l5.48-1.35V4.2Z" />
    </svg>
  )
}

/**
 * The icon for a remote: its provider logo, or the cloud glyph when the host
 * isn't recognized. Accepts standard SVG props (size, className) so callers
 * control sizing and color.
 */
export function RemoteIcon({
  provider,
  ...props
}: { provider: RemoteProvider } & SVGProps<SVGSVGElement>) {
  switch (provider) {
    case 'github':
      return <GithubGlyph {...props} />
    case 'gitlab':
      return <GitlabGlyph {...props} />
    case 'bitbucket':
      return <BitbucketGlyph {...props} />
    case 'azure':
      return <AzureGlyph {...props} />
    default:
      return <Cloud {...props} />
  }
}
