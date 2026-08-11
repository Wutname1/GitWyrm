import { describe, expect, it } from 'vitest'
import { detectProvider } from '@/lib/remoteProvider'

describe('detectProvider', () => {
  it('recognizes the hosts we badge', () => {
    expect(detectProvider('https://github.com/o/r.git')).toBe('github')
    expect(detectProvider('git@github.com:o/r.git')).toBe('github')
    expect(detectProvider('https://gitlab.com/o/r.git')).toBe('gitlab')
    expect(detectProvider('git@bitbucket.org:o/r.git')).toBe('bitbucket')
    expect(detectProvider('https://dev.azure.com/org/proj/_git/r')).toBe('azure')
    expect(detectProvider('git@ssh.dev.azure.com:v3/org/proj/r')).toBe('azure')
    expect(detectProvider('https://org.visualstudio.com/proj/_git/r')).toBe('azure')
  })

  it('resolves enterprise subdomains', () => {
    expect(detectProvider('git@github.myco.com:o/r.git')).toBe('github')
    expect(detectProvider('https://gitlab.myco.com/o/r.git')).toBe('gitlab')
  })

  // The bug CodeQL flagged: a bare substring match let any host containing the
  // brand name impersonate it.
  it('does not match a brand name inside another label', () => {
    expect(detectProvider('https://github-phishing.evil.com/o/r.git')).toBe('unknown')
    expect(detectProvider('https://notgithub.example.com/o/r.git')).toBe('unknown')
    expect(detectProvider('git@evilbitbucket.example.com:o/r.git')).toBe('unknown')
  })

  // Supporting `github.myco.com` means any host with the brand as a whole label
  // matches, wherever it sits -- `gitlab.evil.com` included. Matching the
  // registrable domain instead would break every enterprise install, so this is
  // deliberate. Safe only because the result picks a glyph and a label; never
  // route a token or an API base on it. Mirrors `provider_for_host` in Rust.
  it('accepts the brand as any whole host label, including a spoofed prefix', () => {
    expect(detectProvider('https://gitlab.attacker.io.evil.com/o/r')).toBe('gitlab')
  })

  it('does not match a brand name outside the host', () => {
    expect(detectProvider('https://git.example.com/github/r.git')).toBe('unknown')
    expect(detectProvider('https://git.example.com/o/gitlab.git')).toBe('unknown')
  })

  it('ignores credentials and ports in the authority', () => {
    expect(detectProvider('https://github@evil.example.com/o/r.git')).toBe('unknown')
    expect(detectProvider('https://github.com:443/o/r.git')).toBe('github')
  })

  it('rejects suffix-only matches on visualstudio.com', () => {
    expect(detectProvider('https://evilvisualstudio.com/o/r')).toBe('unknown')
  })

  it('returns unknown for non-remotes and unparseable input', () => {
    expect(detectProvider(undefined)).toBe('unknown')
    expect(detectProvider('')).toBe('unknown')
    expect(detectProvider('C:\\code\\GitWyrm')).toBe('unknown')
    expect(detectProvider('https://git.example.com/o/r.git')).toBe('unknown')
  })

  it("prefers the backend's parsed provider over re-deriving it", () => {
    // The URL alone would read as unknown; the backend already classified it.
    expect(detectProvider({ url: 'https://code.myco.com/o/r.git', provider: 'git_hub' })).toBe('github')
    // Hosts we have no glyph for fall back to the cloud icon.
    expect(detectProvider({ url: 'https://codeberg.org/o/r.git', provider: 'gitea' })).toBe('unknown')
    expect(detectProvider({ url: 'https://git.sr.ht/~o/r', provider: 'source_hut' })).toBe('unknown')
  })

  it('falls back to host parsing when the remote has no provider', () => {
    expect(detectProvider({ url: 'https://github.com/o/r.git' })).toBe('github')
  })
})
