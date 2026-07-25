/**
 * Redaction for anything that leaves the machine (crash reports, breadcrumbs).
 *
 * GitWyrm handles three kinds of text that must never be uploaded verbatim:
 * access tokens, filesystem paths that carry the user's account name, and
 * personal data like the git author email. Error messages and log lines pick all
 * three up incidentally - a failed API call embeds its URL, a git error embeds
 * the repo path - so scrubbing happens centrally here rather than at each site.
 *
 * The goal is a report that is still diagnosable: shapes and filenames survive,
 * identities do not.
 */

/** Token and credential shapes, longest-prefix first so nothing is half-matched. */
const SECRET_PATTERNS: Array<[RegExp, string]> = [
  // GitHub: ghp_ (classic PAT), gho_ (OAuth), ghu_/ghs_ (app tokens), ghr_ (refresh),
  // github_pat_ (fine-grained).
  [/\bgithub_pat_[A-Za-z0-9_]{20,}/g, 'github_pat_[redacted]'],
  [/\bgh[pousr]_[A-Za-z0-9_]{20,}/g, 'gh*_[redacted]'],
  // OpenAI and compatible gateways: sk-, sk-proj-, sk-ant- (Anthropic).
  [/\bsk-[A-Za-z0-9-_]{16,}/g, 'sk-[redacted]'],
  // Google/Firebase API keys.
  [/\bAIza[A-Za-z0-9_-]{30,}/g, 'AIza[redacted]'],
  // AWS access key ids.
  [/\b(?:AKIA|ASIA)[A-Z0-9]{16}/g, 'AKIA[redacted]'],
  // Bearer/token/key in a header or query string.
  [/\b(Bearer|token|api[-_]?key|x-api-key)(\s*[:=]\s*|\s+)[A-Za-z0-9._~+/-]{12,}=*/gi, '$1 [redacted]'],
  // Credentials embedded in a URL: https://user:pass@host
  [/\/\/[^/\s:@]+:[^/\s@]+@/g, '//[redacted]@'],
  // JSON Web Tokens.
  [/\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/g, '[jwt redacted]'],
]

/** Personal data that is not a credential but still identifies the user. */
const PII_PATTERNS: Array<[RegExp, string]> = [
  // Email addresses (git author identity travels in commit errors).
  [/\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b/g, '[email]'],
]

/**
 * Replace the user's account name in Windows and Unix home paths, keeping the
 * rest of the path so the failing file is still identifiable.
 * `C:\Users\jerem\code\repo` -> `C:\Users\[user]\code\repo`
 */
function scrubHomePaths(value: string): string {
  return value
    .replace(/([A-Za-z]:[\\/]+Users[\\/]+)[^\\/\s"']+/gi, '$1[user]')
    .replace(/(\/(?:home|Users)\/)[^/\s"']+/g, '$1[user]')
}

/**
 * Redact secrets, PII, and home-directory account names from a string. Safe to
 * call on any text; returns the input unchanged when there is nothing to remove.
 */
export function scrubText(value: string): string {
  let out = value
  for (const [pattern, replacement] of SECRET_PATTERNS) out = out.replace(pattern, replacement)
  for (const [pattern, replacement] of PII_PATTERNS) out = out.replace(pattern, replacement)
  return scrubHomePaths(out)
}

/**
 * Deep-scrub every string in an arbitrary structure, in place where possible.
 * Sentry events are plain JSON, so walking them covers messages, exception
 * values, breadcrumb text, request URLs, and extra context in one pass.
 *
 * Cycles are tracked because Sentry events can contain repeated references.
 */
export function scrubDeep<T>(input: T, seen = new WeakSet<object>()): T {
  if (typeof input === 'string') return scrubText(input) as unknown as T
  if (input === null || typeof input !== 'object') return input

  const obj = input as unknown as object
  if (seen.has(obj)) return input
  seen.add(obj)

  if (Array.isArray(input)) {
    for (let i = 0; i < input.length; i++) input[i] = scrubDeep(input[i], seen)
    return input
  }

  const record = input as Record<string, unknown>
  for (const key of Object.keys(record)) {
    record[key] = scrubDeep(record[key], seen)
  }
  return input
}
