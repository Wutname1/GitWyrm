import type { SettingsSection } from '@/stores/uiStore'

/**
 * One searchable setting. `id` is passed to SettingRow's `searchId` so picking
 * a result can scroll to and flash the row it names.
 */
export interface SettingsIndexEntry {
  id: string
  section: SettingsSection
  label: string
  hint: string
  /**
   * Extra words people are likely to type instead of the label. Beginners
   * search for the git word ("checkout", "gpg") or the plain word ("dark
   * mode"), and rarely for the label we chose.
   */
  keywords?: string[]
}

/** Section labels, duplicated across App / This repository where the nav does. */
export const SECTION_LABELS: Record<SettingsSection, string> = {
  general: 'General',
  behavior: 'Behavior',
  repository: 'Repository',
  repositoryTags: 'Tags (this repository)',
  tags: 'Tags',
  profiles: 'Profiles',
  ai: 'AI',
  openspec: 'OpenSpec',
  security: 'Security',
  appearance: 'Appearance',
  logs: 'Logs',
  about: 'About',
}

/**
 * Hand-kept index of every setting. Rows are declared here rather than
 * collected from rendered components so search finds settings in sections the
 * user has not opened yet -- and so a setting hidden behind a toggle (tab row,
 * per-repo tag rules) is still findable.
 *
 * When you add a SettingRow, add it here too and give it a matching `searchId`.
 */
export const SETTINGS_INDEX: SettingsIndexEntry[] = [
  // ---------------------------------------------------------------- general
  {
    id: 'git-identity',
    section: 'general',
    label: 'Your name and email',
    hint: 'Goes on every commit you make, so other people can see who did the work.',
    keywords: ['identity', 'author', 'user.name', 'user.email', 'username', 'who am i'],
  },
  {
    id: 'code-folder',
    section: 'general',
    label: 'Code folder',
    hint: 'Scanned for repositories to quick-launch from the open dialog.',
    keywords: ['projects', 'directory', 'path', 'scan', 'workspace'],
  },
  {
    id: 'clone-directory',
    section: 'general',
    label: 'Default clone directory',
    hint: 'Where new clones go. Falls back to the code folder when empty.',
    keywords: ['download', 'copy repository', 'folder', 'path'],
  },
  {
    id: 'branch-switch-mode',
    section: 'general',
    label: 'When switching branches',
    hint: 'What happens to your uncommitted changes when you switch to another branch.',
    keywords: ['checkout', 'stash', 'carry', 'uncommitted', 'work in progress'],
  },
  {
    id: 'commit-button-mode',
    section: 'general',
    label: 'Commit button',
    hint: 'Whether the commit button also pushes in the same step.',
    keywords: ['push', 'commit and push', 'save', 'upload'],
  },
  {
    id: 'editor',
    section: 'general',
    label: 'Open in editor',
    hint: 'Which editor the open button and the file right-click menu use.',
    keywords: ['vs code', 'vscode', 'ide', 'external', 'program'],
  },
  {
    id: 'tab-layout',
    section: 'general',
    label: 'Repository tabs',
    hint: 'Put repository tabs across the top or in a list down the left side.',
    keywords: ['vertical', 'horizontal', 'sidebar', 'layout'],
  },
  {
    id: 'tab-row',
    section: 'general',
    label: 'Tab row',
    hint: 'Give the tabs a row of their own under the app bar.',
    keywords: ['own row', 'app bar', 'title bar', 'layout'],
  },
  {
    id: 'worktrees',
    section: 'general',
    label: 'Worktrees',
    hint: 'Check out more than one branch at once, each in its own folder.',
    keywords: ['worktree', 'multiple branches', 'advanced'],
  },

  // --------------------------------------------------------------- behavior
  {
    id: 'restore-tabs',
    section: 'behavior',
    label: 'On startup',
    hint: 'Reopen the repositories you had open when you last closed GitWyrm.',
    keywords: ['restore', 'reopen', 'session', 'launch', 'start'],
  },
  {
    id: 'context-menu',
    section: 'behavior',
    label: 'Right-click menu',
    hint: 'Add GitWyrm to the Windows folder right-click menu.',
    keywords: ['explorer', 'windows', 'shell', 'open with', 'integration'],
  },

  // -------------------------------------------------------------------- ai
  {
    id: 'ai-provider',
    section: 'ai',
    label: 'Provider',
    hint: 'Bring your own API key. Keys are stored locally on this machine.',
    keywords: ['api key', 'openai', 'anthropic', 'claude', 'copilot', 'github copilot', 'token'],
  },
  {
    id: 'ai-model',
    section: 'ai',
    label: 'Model',
    hint: 'Which model writes your commit messages.',
    keywords: ['gpt', 'claude', 'sonnet', 'commit message', 'generate'],
  },
  {
    id: 'ai-instruction',
    section: 'ai',
    label: 'Commit instructions',
    hint: 'What you want the generated commit messages to look like.',
    keywords: ['prompt', 'system prompt', 'style', 'commit message', 'format'],
  },

  // -------------------------------------------------------------- openspec
  {
    id: 'spec-desk',
    section: 'openspec',
    label: 'Spec Desk',
    hint: 'Show the Spec Desk button and the Specs list in the sidebar.',
    keywords: ['spec desk', 'specs', 'openspec', 'plan', 'hide', 'sidebar', 'toolbar'],
  },
  {
    id: 'openspec-archive-commit-template',
    section: 'openspec',
    label: 'Archive commit message',
    hint: 'The message used when archiving a spec commits automatically.',
    keywords: ['archive', 'spec', 'commit message', 'template', 'chore', 'openspec'],
  },

  // -------------------------------------------------------------- profiles
  {
    // Section-level: opens Profiles rather than flashing one row, since the
    // list itself is the thing you are looking for.
    id: 'profiles-list',
    section: 'profiles',
    label: 'Profiles',
    hint: 'Named identities: the name, email, and signing key that go on your commits.',
    keywords: [
      'profile',
      'account',
      'identity',
      'work',
      'personal',
      'switch account',
      'multiple accounts',
      'name',
      'email',
    ],
  },
  {
    id: 'profile-folders',
    section: 'profiles',
    label: 'Use for these folders',
    hint: 'Repositories inside a folder use a profile automatically, in GitWyrm and your terminal.',
    keywords: ['folder', 'includeif', 'gitdir', 'automatic', 'per folder', 'rule'],
  },
  {
    id: 'repo-profile',
    section: 'profiles',
    label: 'Commit as (this repository)',
    hint: 'Pin one repository to a profile, whatever your active profile is.',
    keywords: ['override', 'pin', 'per repository', 'identity', 'commit as'],
  },

  // -------------------------------------------------------------- security
  {
    id: 'signing-enabled',
    section: 'security',
    label: 'Sign my commits',
    hint: 'Prove your commits came from you. Applies to the repository in the active tab.',
    keywords: ['gpg', 'signature', 'verified', 'key', 'signing'],
  },
  {
    id: 'signing-key',
    section: 'security',
    label: 'Key to sign with',
    hint: 'Which of your gpg keys signs commits in this repository.',
    keywords: ['gpg', 'key', 'choose key', 'switch key', 'work', 'personal', 'new key', 'generate'],
  },
  {
    id: 'ssh-host',
    section: 'security',
    label: 'Host (SSH)',
    hint: 'Check whether a git host accepts an SSH key from this computer.',
    keywords: [
      'ssh',
      'push',
      'pull',
      'permission denied',
      'publickey',
      'access',
      'connect',
      'test connection',
      'github',
    ],
  },
  {
    id: 'ssh-keys',
    section: 'security',
    label: 'Your SSH keys',
    hint: 'Keys in your .ssh folder, and which one each host uses.',
    keywords: ['ssh key', 'ed25519', 'id_rsa', 'identityfile', 'ssh config', 'new key'],
  },
  {
    id: 'git-executable',
    section: 'security',
    label: 'Git',
    hint: 'The git program used to fetch, pull, push, and clone.',
    keywords: ['git.exe', 'executable', 'program', 'path', 'bundled'],
  },
  {
    id: 'gpg-executable',
    section: 'security',
    label: 'GPG',
    hint: 'The gpg program used to sign commits.',
    keywords: ['gpg.exe', 'executable', 'program', 'path', 'bundled', 'signing'],
  },

  // ------------------------------------------------------------ appearance
  {
    id: 'theme-mode',
    section: 'appearance',
    label: 'Mode',
    hint: 'Use light, dark, or match your system setting.',
    keywords: ['dark mode', 'light mode', 'system', 'night'],
  },
  {
    id: 'theme',
    section: 'appearance',
    label: 'Theme',
    hint: 'Pick a color scheme. Auto follows the mode above.',
    keywords: ['colors', 'color scheme', 'palette', 'skin'],
  },
  {
    id: 'mint-accent',
    section: 'appearance',
    label: 'Mint accent',
    hint: 'Use GitWyrm’s mint green accent color.',
    keywords: ['accent', 'green', 'highlight color', 'brand'],
  },
  {
    id: 'font',
    section: 'appearance',
    label: 'Font',
    hint: 'The typeface used throughout the app.',
    keywords: ['typeface', 'typography', 'family', 'monospace'],
  },
  {
    id: 'text-size',
    section: 'appearance',
    label: 'Text size',
    hint: 'Makes all text bigger or smaller.',
    keywords: ['font size', 'bigger', 'smaller', 'scale', 'readability'],
  },
  {
    id: 'text-weight',
    section: 'appearance',
    label: 'Text weight',
    hint: 'Makes text lighter or bolder throughout the app.',
    keywords: ['bold', 'thin', 'font weight'],
  },
  {
    id: 'reset-fonts',
    section: 'appearance',
    label: 'Reset fonts',
    hint: 'Put the font, size, and weight back to defaults.',
    keywords: ['default', 'restore', 'undo'],
  },
  {
    id: 'repo-icons',
    section: 'appearance',
    label: 'Repository icons',
    hint: 'Show an icon on each repository tab.',
    keywords: ['tab icon', 'emoji', 'avatar'],
  },
  {
    id: 'icon-only-tabs',
    section: 'appearance',
    label: 'Icon-only tabs',
    hint: 'Shrink repository tabs down to just their icon.',
    keywords: ['compact', 'narrow', 'tab width', 'space'],
  },
  {
    id: 'change-size',
    section: 'appearance',
    label: 'Commit change size',
    hint: 'How the amount changed in a commit is shown.',
    keywords: ['lines changed', 'diff size', 'bar', 'insertions', 'deletions'],
  },
  {
    id: 'app-zoom',
    section: 'appearance',
    label: 'App zoom',
    hint: 'Makes everything in GitWyrm bigger or smaller.',
    keywords: ['zoom', 'scale', 'dpi', 'bigger', 'smaller'],
  },

  // ------------------------------------------------------------------ tags
  {
    id: 'tag-push-default',
    section: 'tags',
    label: 'After pushing',
    hint: 'What to do with local-only tags after a push, for every repository.',
    keywords: ['tag', 'push', 'remote', 'prompt', 'default'],
  },
  {
    id: 'tag-push-on-create',
    section: 'tags',
    label: 'New tags',
    hint: 'Whether the send-to-remote box starts checked when you make a tag.',
    keywords: ['tag', 'create', 'remote', 'checkbox'],
  },
  {
    id: 'tag-delete-on-remote',
    section: 'tags',
    label: 'Deleting tags',
    hint: 'Whether deleting a tag also removes it from the remote by default.',
    keywords: ['tag', 'delete', 'remove', 'remote', 'github'],
  },

  // ------------------------------------------------------------ repository
  {
    id: 'repo-tab-name',
    section: 'repository',
    label: 'Tab name',
    hint: 'Rename the tab for the repository in the active tab.',
    keywords: ['rename', 'title', 'label', 'nickname'],
  },
  {
    id: 'repo-tab-icon',
    section: 'repository',
    label: 'Tab icon',
    hint: 'Pick the icon shown on this repository’s tab.',
    keywords: ['icon', 'emoji', 'symbol', 'color'],
  },
  {
    id: 'repo-folder',
    section: 'repository',
    label: 'Repository folder',
    hint: 'Quick ways to open this repository outside GitWyrm.',
    keywords: ['explorer', 'terminal', 'path', 'location', 'open folder'],
  },

  // ------------------------------------------------------- repository tags
  {
    id: 'repo-tag-override',
    section: 'repositoryTags',
    label: 'Custom tag settings',
    hint: 'Give this repository its own tag rules instead of the application ones.',
    keywords: ['override', 'per repository', 'tag', 'custom'],
  },
  {
    id: 'repo-tag-push',
    section: 'repositoryTags',
    label: 'After pushing (this repository)',
    hint: 'What this repository does with local-only tags after a push.',
    keywords: ['tag', 'push', 'remote', 'override'],
  },
  {
    id: 'repo-tag-on-create',
    section: 'repositoryTags',
    label: 'New tags (this repository)',
    hint: 'Whether the send-to-remote box starts checked in this repository.',
    keywords: ['tag', 'create', 'remote', 'override'],
  },
  {
    id: 'repo-tag-delete',
    section: 'repositoryTags',
    label: 'Deleting tags (this repository)',
    hint: 'Whether deleting a tag here also removes it from the remote.',
    keywords: ['tag', 'delete', 'remote', 'override'],
  },

  // ------------------------------------------------------------------ logs
  {
    id: 'report-problem',
    section: 'logs',
    label: 'Report a problem',
    hint: 'Send us what went wrong, with your log attached automatically.',
    keywords: [
      'bug',
      'bug report',
      'report',
      'feedback',
      'support',
      'help',
      'broken',
      'issue',
      'contact',
      'send logs',
      'something is wrong',
    ],
  },
  {
    id: 'application-log',
    section: 'logs',
    label: 'Application log',
    hint: 'View, open, or clear the diagnostic output GitWyrm writes.',
    keywords: ['debug', 'diagnostics', 'troubleshoot', 'error', 'crash'],
  },

  // ----------------------------------------------------------------- about
  {
    id: 'version',
    section: 'about',
    label: 'Version',
    hint: 'Which build of GitWyrm you are running.',
    keywords: ['build', 'release', 'commit hash'],
  },
  {
    id: 'update-channel',
    section: 'about',
    label: 'Update channel',
    hint: 'Stable or beta pre-release builds.',
    keywords: ['beta', 'stable', 'prerelease', 'updates'],
  },
  {
    id: 'auto-update',
    section: 'about',
    label: 'Auto update',
    hint: 'Install new versions on the loading screen at startup.',
    keywords: ['automatic', 'auto update', 'background', 'silent', 'startup'],
  },
  {
    id: 'updates',
    section: 'about',
    label: 'Updates',
    hint: 'Check for a new version of GitWyrm.',
    keywords: ['upgrade', 'check for updates', 'download', 'new version'],
  },
  {
    id: 'reset-all',
    section: 'about',
    label: 'Reset all settings',
    hint: 'Put every setting back to its default.',
    keywords: ['defaults', 'factory reset', 'start over', 'restore'],
  },
]

export interface SettingsSearchResult extends SettingsIndexEntry {
  /** Lower is better. Label matches beat hint and keyword matches. */
  score: number
}

/**
 * Rank settings against a typed query. Every word must match somewhere, so
 * "dark mode" narrows rather than widening to everything containing "mode".
 */
export function searchSettings(query: string): SettingsSearchResult[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return []

  const results: SettingsSearchResult[] = []

  for (const entry of SETTINGS_INDEX) {
    const label = entry.label.toLowerCase()
    const hint = entry.hint.toLowerCase()
    const keywords = (entry.keywords ?? []).join(' ').toLowerCase()
    const section = SECTION_LABELS[entry.section].toLowerCase()

    let score = 0
    let matchedAll = true

    for (const word of words) {
      if (label.startsWith(word)) score += 0
      else if (label.includes(word)) score += 1
      else if (keywords.includes(word)) score += 2
      else if (hint.includes(word)) score += 3
      else if (section.includes(word)) score += 4
      else {
        matchedAll = false
        break
      }
    }

    if (matchedAll) results.push({ ...entry, score })
  }

  return results.sort((a, b) => a.score - b.score || a.label.localeCompare(b.label))
}
