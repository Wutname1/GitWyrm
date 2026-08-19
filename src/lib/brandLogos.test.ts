import { describe, expect, it } from 'vitest'
import { botIdentity, isMono, providerLogo } from './brandLogos'

describe('providerLogo', () => {
  it('knows the providers we ship art for', () => {
    expect(providerLogo('anthropic')).toBeTruthy()
    expect(providerLogo('openai')).toBeTruthy()
  })

  it('ignores case and surrounding space', () => {
    expect(providerLogo('  Anthropic ')).toBeTruthy()
  })

  /**
   * The catalog comes from models.dev and lists far more providers than we have
   * logos for, so a miss is ordinary and callers show the name alone.
   */
  it('returns null for a provider we have no mark for', () => {
    expect(providerLogo('together')).toBeNull()
    expect(providerLogo('')).toBeNull()
  })

  /**
   * Every provider the settings picker calls popular has a logo, so that list
   * never shows one row with a mark and the next without.
   */
  it('covers every provider the picker promotes', () => {
    for (const id of [
      'github-copilot',
      'anthropic',
      'openai',
      'google',
      'openrouter',
      'deepseek',
    ]) {
      expect(providerLogo(id), id).toBeTruthy()
    }
  })
})

describe('botIdentity', () => {
  it('names the bots that commit through a GitHub app', () => {
    expect(botIdentity('49699333+dependabot[bot]@users.noreply.github.com')?.name).toBe(
      'Dependabot'
    )
    expect(botIdentity('198982749+Copilot@users.noreply.github.com')?.name).toBe('GitHub Copilot')
    expect(botIdentity('1234+claude[bot]@users.noreply.github.com')?.name).toBe('Claude')
    expect(botIdentity('1234+snyk-bot@users.noreply.github.com')?.name).toBe('Snyk')
    expect(botIdentity('29139614+renovate[bot]@users.noreply.github.com')?.name).toBe('Renovate')
    expect(botIdentity('41898282+github-actions[bot]@users.noreply.github.com')?.name).toBe(
      'GitHub Actions'
    )
  })

  it('accepts the older address form with no numeric id', () => {
    expect(botIdentity('dependabot[bot]@users.noreply.github.com')?.name).toBe('Dependabot')
    expect(botIdentity('snyk-bot@users.noreply.github.com')?.name).toBe('Snyk')
  })

  it('names the bots that commit under their own domain', () => {
    expect(botIdentity('snyk-bot@snyk.io')?.name).toBe('Snyk')
    expect(botIdentity('noreply@anthropic.com')?.name).toBe('Claude')
  })

  it('ignores case and surrounding space', () => {
    expect(botIdentity('  49699333+Dependabot[BOT]@users.noreply.github.com ')?.name).toBe(
      'Dependabot'
    )
    expect(botIdentity(' Snyk-Bot@Snyk.io ')?.name).toBe('Snyk')
  })

  /**
   * The whole point of matching the exact address: a person who works at one of
   * these companies is still a person, and must keep their own picture.
   */
  it('does not claim a human who shares the bot domain', () => {
    expect(botIdentity('someone@snyk.io')).toBeNull()
    expect(botIdentity('jane@anthropic.com')).toBeNull()
  })

  it('does not claim a human whose name merely contains a bot name', () => {
    expect(botIdentity('notdependabot@users.noreply.github.com')).toBeNull()
    expect(botIdentity('renovater@users.noreply.github.com')).toBeNull()
    expect(botIdentity('claudemonet@users.noreply.github.com')).toBeNull()
    expect(botIdentity('1234+snyk-bot-fan@users.noreply.github.com')).toBeNull()
  })

  it('does not match a bot name at a domain the bot does not use', () => {
    expect(botIdentity('dependabot@example.com')).toBeNull()
    expect(botIdentity('copilot@example.com')).toBeNull()
  })

  it('returns null for an ordinary person and for no address at all', () => {
    expect(botIdentity('1234+octocat@users.noreply.github.com')).toBeNull()
    expect(botIdentity('someone@example.com')).toBeNull()
    expect(botIdentity('')).toBeNull()
    expect(botIdentity('   ')).toBeNull()
  })

  it('gives every bot something to draw', () => {
    const addresses = [
      '49699333+dependabot[bot]@users.noreply.github.com',
      '198982749+Copilot@users.noreply.github.com',
      '1234+claude[bot]@users.noreply.github.com',
      'snyk-bot@snyk.io',
      '1234+chatgpt-codex-connector@users.noreply.github.com',
      '29139614+renovate[bot]@users.noreply.github.com',
      '41898282+github-actions[bot]@users.noreply.github.com',
      '1234+semantic-release-bot@users.noreply.github.com',
    ]
    for (const address of addresses) {
      const bot = botIdentity(address)
      expect(bot, address).not.toBeNull()
      // Never a row with an empty image and nothing to show.
      expect(bot!.logo, address).toBeTruthy()
      expect(bot!.name.length).toBeGreaterThan(0)
    }
  })
})

/**
 * The silhouette marks carry no color of their own, so they must be drawn as a
 * mask that takes the surrounding text color. Getting this wrong is invisible
 * in one theme and illegible in the other, so the classification is pinned.
 */
describe('isMono', () => {
  const monoProviders = ['github-copilot', 'openai', 'xai']
  const colorProviders = ['anthropic', 'google', 'openrouter', 'deepseek', 'mistral']

  it('flags the silhouette marks', () => {
    for (const id of monoProviders) {
      expect(isMono(providerLogo(id)!), id).toBe(true)
    }
  })

  it('leaves brand-colored art alone, so its colors survive', () => {
    for (const id of colorProviders) {
      expect(isMono(providerLogo(id)!), id).toBe(false)
    }
  })

  it('agrees with what the bot rows are told to draw', () => {
    const cases: [string, boolean][] = [
      ['49699333+dependabot[bot]@users.noreply.github.com', true],
      ['198982749+Copilot@users.noreply.github.com', true],
      ['29139614+renovate[bot]@users.noreply.github.com', true],
      ['1234+chatgpt-codex-connector@users.noreply.github.com', true],
      // Full-color art: inverting these would wash the logo out.
      ['1234+claude[bot]@users.noreply.github.com', false],
      ['snyk-bot@snyk.io', false],
    ]
    for (const [email, mono] of cases) {
      const bot = botIdentity(email)
      expect(bot, email).not.toBeNull()
      expect(bot!.mono, email).toBe(mono)
      expect(isMono(bot!.logo), email).toBe(mono)
    }
  })
})
