import { Cloud } from 'lucide-react'
import type { SVGProps } from 'react'

export type RemoteProvider = 'github' | 'gitlab' | 'bitbucket' | 'azure' | 'unknown'

/**
 * Identify the hosting provider from a remote URL (ssh or https). Returns
 * 'unknown' for self-hosted or unrecognized hosts, which fall back to the cloud
 * glyph. Matches on the host substring so enterprise subdomains still resolve
 * (e.g. github.myco.com -> github).
 */
export function detectProvider(url: string | undefined): RemoteProvider {
  if (!url) return 'unknown'
  const u = url.toLowerCase()
  if (u.includes('github')) return 'github'
  if (u.includes('gitlab')) return 'gitlab'
  if (u.includes('bitbucket')) return 'bitbucket'
  if (u.includes('dev.azure.com') || u.includes('visualstudio.com')) return 'azure'
  return 'unknown'
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
