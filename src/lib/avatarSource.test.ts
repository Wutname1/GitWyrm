import { beforeEach, describe, expect, it, vi } from 'vitest'

/**
 * The cache lives in module state, so each test imports a fresh copy rather
 * than inheriting whatever the previous one resolved.
 */
type Mod = typeof import('./avatarSource')
let avatarUrl: Mod['avatarUrl']
let forgetAvatar: Mod['forgetAvatar']

/**
 * Gravatar is probed by loading an image, so the tests drive `Image` rather
 * than fetch. Each instance records itself so a test can decide, per URL,
 * whether the picture exists.
 */
let probes: string[] = []
let exists = true

class FakeImage {
  onload: (() => void) | null = null
  onerror: (() => void) | null = null
  set src(url: string) {
    probes.push(url)
    queueMicrotask(() => (exists ? this.onload?.() : this.onerror?.()))
  }
}

/** Enough of the Storage API for the cache; the suite runs headless. */
function fakeStorage(): Storage {
  const data = new Map<string, string>()
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
    clear: () => data.clear(),
    key: (i: number) => [...data.keys()][i] ?? null,
    get length() {
      return data.size
    },
  } as Storage
}

beforeEach(async () => {
  probes = []
  exists = true
  vi.stubGlobal('localStorage', fakeStorage())
  vi.stubGlobal('Image', FakeImage)
  // The hash only has to be stable, not real.
  vi.stubGlobal('crypto', {
    subtle: {
      digest: (_alg: string, bytes: Uint8Array) =>
        Promise.resolve(new Uint8Array(bytes.slice(0, 4)).buffer),
    },
  })

  vi.resetModules()
  ;({ avatarUrl, forgetAvatar } = await import('./avatarSource'))
})

describe('avatarUrl', () => {
  it('caches a hit so a second lookup does not probe again', async () => {
    await avatarUrl('a@example.com', 64)
    await avatarUrl('a@example.com', 64)
    expect(probes).toHaveLength(1)
  })

  it('caches a miss too, so absent pictures are not re-probed', async () => {
    exists = false
    expect(await avatarUrl('nobody@example.com', 64)).toBeNull()
    expect(await avatarUrl('nobody@example.com', 64)).toBeNull()
    expect(probes).toHaveLength(1)
  })

  it('reads GitHub no-reply addresses without any probe', async () => {
    const url = await avatarUrl('1234+octocat@users.noreply.github.com', 64)
    expect(url).toBe('https://avatars.githubusercontent.com/u/1234?s=64')
    expect(probes).toHaveLength(0)
  })

  it('serves every size from one cached entry', async () => {
    const small = await avatarUrl('a@example.com', 38)
    const large = await avatarUrl('a@example.com', 52)
    expect(small).toContain('s=38')
    expect(large).toContain('s=52')
    expect(probes).toHaveLength(1)
  })
})

describe('forgetAvatar', () => {
  it('re-probes the next lookup', async () => {
    await avatarUrl('a@example.com', 64)
    forgetAvatar('a@example.com')
    await avatarUrl('a@example.com', 64)
    expect(probes).toHaveLength(2)
  })

  it('picks up a picture that did not exist when it was first cached', async () => {
    exists = false
    expect(await avatarUrl('new@example.com', 64)).toBeNull()

    exists = true
    forgetAvatar('new@example.com')
    expect(await avatarUrl('new@example.com', 64)).not.toBeNull()
  })

  it('matches the address the way the cache keys it', async () => {
    await avatarUrl('A@Example.com ', 64)
    forgetAvatar(' a@example.COM')
    await avatarUrl('a@example.com', 64)
    expect(probes).toHaveLength(2)
  })

  it('leaves other addresses cached', async () => {
    await avatarUrl('a@example.com', 64)
    await avatarUrl('b@example.com', 64)
    forgetAvatar('a@example.com')
    await avatarUrl('b@example.com', 64)
    expect(probes).toHaveLength(2)
  })

  it('ignores a blank address', async () => {
    await avatarUrl('a@example.com', 64)
    forgetAvatar('   ')
    await avatarUrl('a@example.com', 64)
    expect(probes).toHaveLength(1)
  })
})
