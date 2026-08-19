import claudeIcon from '@/assets/icons/claude.svg'
import copilotIcon from '@/assets/icons/githubcopilot.svg'
import deepseekIcon from '@/assets/icons/deepseek-color.svg'
import geminiIcon from '@/assets/icons/gemini-color.svg'
import githubIcon from '@/assets/icons/github.svg'
import grokIcon from '@/assets/icons/grok.svg'
import mistralIcon from '@/assets/icons/mistral-color.svg'
import openaiIcon from '@/assets/icons/openai.svg'
import openrouterIcon from '@/assets/icons/openrouter.svg'
import renovateIcon from '@/assets/icons/renovate.svg'
import snykIcon from '@/assets/icons/snyk.svg'

/**
 * Logos for the third parties that show up inside a repository.
 *
 * Two different questions are answered here, and they are kept apart on
 * purpose. `providerLogo` maps an AI provider id from the catalog, which is a
 * value we control. `botIdentity` maps a commit author's email, which is just a
 * string a commit happens to carry -- so it matches on the bot's actual account
 * address and nothing looser.
 *
 * Separate from `editors.tsx`: that map is keyed by `EditorKind`, a union
 * generated from a Rust enum, so it cannot hold anything that is not a
 * launchable editor.
 */

/**
 * Catalog provider ids to logos.
 *
 * The ids are the ones the app actually surfaces: `POPULAR_PROVIDER_IDS` in
 * `AiSettings.tsx`, plus the providers with a base URL in `known_base_url`
 * (`src-tauri/src/ai/catalog.rs`). The catalog itself comes from models.dev and
 * is far longer than this; shipping art for the whole tail is not the goal, so
 * anything else falls back to its name.
 */
const providerLogos: Record<string, string> = {
  'github-copilot': copilotIcon,
  anthropic: claudeIcon,
  openai: openaiIcon,
  google: geminiIcon,
  deepseek: deepseekIcon,
  openrouter: openrouterIcon,
  mistral: mistralIcon,
  xai: grokIcon,
}

/**
 * An AI provider's logo, or null when we have no mark for it.
 *
 * A miss is the normal case rather than a failure -- callers fall back to
 * showing the provider's name alone.
 */
export function providerLogo(id: string): string | null {
  return providerLogos[id.trim().toLowerCase()] ?? null
}

/** An AI provider's logo, or nothing at all when we have no mark for it. */
export function ProviderGlyph({ id, size = 14 }: { id: string; size?: number }) {
  const src = providerLogo(id)
  if (!src) return null
  return (
    <img
      src={src}
      alt=""
      aria-hidden
      width={size}
      height={size}
      className="flex-none"
      style={{ width: size, height: size }}
    />
  )
}

/**
 * Bots that author commits, keyed by the account each one commits as.
 *
 * Matching is on the whole address rather than a substring: `snyk-bot@snyk.io`
 * identifies Snyk, but a human at `someone@snyk.io` is a person and must keep
 * their own picture. GitHub apps commit under a no-reply address in either the
 * numeric-id form (`1234+name[bot]@...`) or the older bare form, so both are
 * accepted.
 */
interface BotSource {
  /** Exact addresses this bot commits under. */
  emails?: string[]
  /** GitHub app logins, matched inside a users.noreply.github.com address. */
  githubLogins?: string[]
  logo: string
  name: string
}

const bots: BotSource[] = [
  // Dependabot is GitHub's own, and has no mark of its own to use.
  { githubLogins: ['dependabot'], logo: githubIcon, name: 'Dependabot' },
  { githubLogins: ['copilot', 'copilot-swe-agent'], logo: copilotIcon, name: 'GitHub Copilot' },
  { githubLogins: ['claude'], logo: claudeIcon, name: 'Claude' },
  { emails: ['noreply@anthropic.com'], logo: claudeIcon, name: 'Claude' },
  { githubLogins: ['snyk-bot'], emails: ['snyk-bot@snyk.io'], logo: snykIcon, name: 'Snyk' },
  { githubLogins: ['chatgpt-codex-connector'], logo: openaiIcon, name: 'Codex' },
  { githubLogins: ['renovate'], emails: ['bot@renovateapp.com'], logo: renovateIcon, name: 'Renovate' },
  // No mark of their own, so they carry GitHub's rather than showing initials.
  { githubLogins: ['github-actions'], logo: githubIcon, name: 'GitHub Actions' },
  { githubLogins: ['semantic-release-bot'], logo: githubIcon, name: 'semantic-release' },
]

/** The login inside a GitHub no-reply address, lowercased, with `[bot]` dropped. */
function noReplyLogin(email: string): string | null {
  const match = email.match(/^(?:\d+\+)?([^@]+)@users\.noreply\.github\.com$/i)
  if (!match) return null
  return match[1].toLowerCase().replace(/\[bot\]$/, '')
}

export interface BotIdentity {
  logo: string
  name: string
}

/**
 * The bot behind a commit author's email, or null for an ordinary person.
 *
 * Used by the commit list so an automated commit reads as the tool that made
 * it, rather than as a stranger on the team with two-letter initials. A
 * person's commits never match, so their Gravatar is untouched.
 */
export function botIdentity(email: string): BotIdentity | null {
  const key = email.trim().toLowerCase()
  if (!key) return null
  const login = noReplyLogin(key)
  for (const bot of bots) {
    const byEmail = bot.emails?.includes(key)
    const byLogin = !!login && !!bot.githubLogins?.includes(login)
    if (byEmail || byLogin) return { logo: bot.logo, name: bot.name }
  }
  return null
}
