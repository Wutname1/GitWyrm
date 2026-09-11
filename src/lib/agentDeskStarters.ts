import type { ChatStarter } from '@/components/domain/agent-desk/NewChatLanding'

/**
 * Somewhere to start, built only from work that is really in the repository.
 *
 * The empty screen used to be filled with four labelled sections of settings.
 * The alternative most agent clients reach for is a row of generic prompts --
 * "Explore the codebase", "Catch me up" -- which anyone could have written and
 * which say nothing about the project in front of you.
 *
 * These are the third option, and the only one this product is positioned to
 * offer: every entry names something GitWyrm has actually measured. If it has
 * measured nothing, the row is empty and the screen shows no suggestions at
 * all. That is the honest answer and a perfectly good screen -- an invented
 * suggestion would cost the row its meaning.
 */
export interface StarterInputs {
  /** Files changed but not committed, staged and unstaged together. */
  uncommittedFileCount: number
  /** The branch checked out right now, when it is known. */
  branchName: string | null
  /**
   * Whether this branch has commits the remote does not.
   *
   * `null` means GitWyrm has not been able to work it out -- unknown, which is
   * not the same as zero and must not produce a suggestion either way.
   */
  aheadCount: number | null
}

/** "1 file", never "1 file(s)". */
function fileCount(n: number): string {
  return `${n} file${n === 1 ? '' : 's'}`
}

export function buildStarters(inputs: StarterInputs): ChatStarter[] {
  const starters: ChatStarter[] = []

  if (inputs.uncommittedFileCount > 0) {
    const files = fileCount(inputs.uncommittedFileCount)
    starters.push({
      id: 'review-uncommitted',
      label: `Review my ${files}`,
      prompt: `Review the ${files} I have changed but not committed yet. Tell me what they do and anything that looks wrong.`,
    })
    starters.push({
      id: 'describe-uncommitted',
      label: 'Write my commit message',
      prompt: `Read the ${files} I have changed but not committed, and draft a commit message for them.`,
    })
  }

  // Only when there is something to explain. On a branch with nothing ahead of
  // the remote, "explain my branch" is a question about no commits.
  if (inputs.branchName && inputs.aheadCount != null && inputs.aheadCount > 0) {
    const commits = `${inputs.aheadCount} commit${inputs.aheadCount === 1 ? '' : 's'}`
    starters.push({
      id: 'explain-branch',
      label: `Explain the ${commits} on ${inputs.branchName}`,
      prompt: `Explain the ${commits} on the branch ${inputs.branchName} that are not on the remote yet.`,
    })
  }

  return starters
}
