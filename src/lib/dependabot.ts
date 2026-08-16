/**
 * Dependabot pull requests, and the commands it still listens for.
 *
 * Dependabot is driven by comments: you write `@dependabot rebase` on the pull
 * request and the bot acts on it. There is no API for this, which is why these
 * are posted through the ordinary comment endpoint.
 *
 * IMPORTANT -- verified against GitHub's changelog on 2026-08-15: the commands
 * `merge`, `squash and merge`, `cancel merge`, `close` and `reopen` were REMOVED
 * on 2026-01-27. Posting one today does nothing, so none of them appear here.
 * Merging a Dependabot pull request goes through the normal merge button like
 * any other pull request. Re-check the reference before adding a command:
 * https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-pull-request-comment-commands
 */

/** Dependabot's account. The `[bot]` suffix is part of the login. */
const DEPENDABOT_LOGIN = 'dependabot[bot]'

/**
 * Whether a pull request was opened by Dependabot.
 *
 * Login rather than the API's `type` field: that reports `Bot` for every app, so
 * it says something is a bot but not which one. The preview login some accounts
 * carry is normalised away first.
 */
export function isDependabot(author: string): boolean {
  return author.trim().toLowerCase() === DEPENDABOT_LOGIN
}

export interface DependabotCommand {
  id: string
  /** Literal comment text posted to the pull request. */
  command: string
  /** Button label, in plain language rather than the command's own wording. */
  label: string
  /** What it does, for the tooltip. */
  description: string
  /** Closes the pull request and suppresses future ones, so it is confirmed. */
  destructive?: boolean
}

/** The commands that always apply to a Dependabot pull request. */
export const DEPENDABOT_COMMANDS: DependabotCommand[] = [
  {
    id: 'rebase',
    command: '@dependabot rebase',
    label: 'Rebase',
    description: 'Replay this update on top of the latest base branch.',
  },
  {
    id: 'recreate',
    command: '@dependabot recreate',
    label: 'Recreate',
    description: 'Rebuild this pull request from scratch, discarding any edits made to it.',
  },
]

/**
 * The "stop offering this" commands.
 *
 * Each one closes the pull request as well as suppressing future ones, which is
 * why they are kept apart from the commands above and confirmed before sending.
 */
export const DEPENDABOT_IGNORE_COMMANDS: DependabotCommand[] = [
  {
    id: 'ignore-patch',
    command: '@dependabot ignore this patch version',
    label: 'Skip this patch version',
    description: 'Close this pull request and stop offering this patch version.',
    destructive: true,
  },
  {
    id: 'ignore-minor',
    command: '@dependabot ignore this minor version',
    label: 'Skip this minor version',
    description: 'Close this pull request and stop offering this minor version.',
    destructive: true,
  },
  {
    id: 'ignore-major',
    command: '@dependabot ignore this major version',
    label: 'Skip this major version',
    description: 'Close this pull request and stop offering this major version.',
    destructive: true,
  },
  {
    id: 'ignore-dependency',
    command: '@dependabot ignore this dependency',
    label: 'Stop updating this package',
    description:
      'Close this pull request and stop offering any future update for this package.',
    destructive: true,
  },
]
