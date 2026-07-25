import { toast } from 'sonner'
import { log } from '@/lib/log'
import { detectProvider, providerLabel, type RemoteProvider } from '@/lib/remoteProvider'

export interface RemoteWebTarget {
  provider: RemoteProvider
  label: string
  repositoryUrl: string
}

interface ParsedRemote {
  host: string
  path: string
  protocol: 'http' | 'https'
}

/** Turn HTTPS, SSH, and scp-style Git remotes into a safe browser location. */
function parseRemote(url: string): ParsedRemote | null {
  const value = url.trim()
  if (!value || /^[a-z]:[\\/]/i.test(value)) return null

  if (value.includes('://')) {
    try {
      const parsed = new URL(value)
      return {
        host: parsed.host,
        path: parsed.pathname.replace(/^\/+/, ''),
        protocol: parsed.protocol === 'http:' ? 'http' : 'https',
      }
    } catch {
      return null
    }
  }

  const scp = value.match(/^(?:[^@/]+@)?([^:/]+):(.+)$/)
  if (!scp) return null
  if (scp[1].length === 1) return null
  return { host: scp[1], path: scp[2], protocol: 'https' }
}

function trimRepositorySuffix(path: string): string {
  return path.replace(/^\/+|\/+$/g, '').replace(/\.git$/i, '')
}

function azureWebBase(remote: ParsedRemote): string | null {
  const parts = trimRepositorySuffix(remote.path).split('/').filter(Boolean)
  const host = remote.host.toLowerCase()

  // Azure's SSH URLs use v3/org/project/repo while its website uses
  // org/project/_git/repo.
  if (host === 'ssh.dev.azure.com' && parts[0]?.toLowerCase() === 'v3' && parts.length >= 4) {
    return `https://dev.azure.com/${parts[1]}/${parts[2]}/_git/${parts.slice(3).join('/')}`
  }
  if (host.endsWith('vs-ssh.visualstudio.com') && parts[0]?.toLowerCase() === 'v3' && parts.length >= 4) {
    return `https://${parts[1]}.visualstudio.com/${parts[2]}/_git/${parts.slice(3).join('/')}`
  }

  if (host === 'dev.azure.com' || host.endsWith('.visualstudio.com')) {
    return `${remote.protocol}://${remote.host}/${parts.join('/')}`
  }
  return null
}

/**
 * Browser target for a configured remote, including self-hosted servers.
 *
 * Remote URL parsing is owned by Rust (`git::remote_url`), and `list_remotes`
 * ships the parsed `provider` and `web_base` on every RemoteInfo. Prefer those:
 * pass the whole remote so the UI and the backend can never disagree about which
 * host a remote belongs to. The `string` overload re-parses here and is kept for
 * the few call sites that only hold a URL.
 */
export function remoteWebTarget(
  remote: string | { url: string; provider?: RemoteProvider; web_base?: string | null },
): RemoteWebTarget | null {
  if (typeof remote !== 'string' && remote.web_base && remote.provider) {
    return {
      provider: remote.provider,
      label: providerLabel(remote.provider) ?? remote.web_base,
      repositoryUrl: remote.web_base,
    }
  }

  const url = typeof remote === 'string' ? remote : remote.url
  const parsed = parseRemote(url)
  if (!parsed || !parsed.host || !trimRepositorySuffix(parsed.path)) return null

  const provider = detectProvider(url)
  const repositoryUrl =
    provider === 'azure'
      ? azureWebBase(parsed)
      : `${parsed.protocol}://${parsed.host}/${trimRepositorySuffix(parsed.path)}`
  if (!repositoryUrl) return null

  return {
    provider,
    label: providerLabel(provider) ?? parsed.host,
    repositoryUrl,
  }
}

function encodeBranchPath(branch: string): string {
  return branch.split('/').map(encodeURIComponent).join('/')
}

/** Provider-specific page for a branch, or null when the host route is unknown. */
export function remoteBranchWebUrl(target: RemoteWebTarget, branch: string): string | null {
  const name = branch.trim()
  switch (target.provider) {
    case 'github':
      return `${target.repositoryUrl}/tree/${encodeBranchPath(name)}`
    case 'gitlab':
      return `${target.repositoryUrl}/-/tree/${encodeBranchPath(name)}`
    case 'bitbucket':
      return `${target.repositoryUrl}/branch/${encodeURIComponent(name)}`
    case 'azure':
      return `${target.repositoryUrl}?version=GB${encodeURIComponent(name)}`
    default:
      return null
  }
}

/** Open a host page and show a plain error if Windows refuses the link. */
export function openWebUrl(url: string, label: string): void {
  void import('@tauri-apps/plugin-opener')
    .then(({ openUrl }) => openUrl(url))
    .catch((error: unknown) => {
      log.error(`could not open ${label}: ${String(error)}`)
      toast.error(`Could not open ${label}`)
    })
}
