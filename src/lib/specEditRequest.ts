/**
 * Reading an "edit this file" request out of a question typed into Ask.
 *
 * Ask answers questions; it does not write. But "reword the Why section" is a
 * reasonable thing to type at something that just explained the Why section, and
 * answering it with a refusal is unhelpful when GitWyrm can draft the edit and
 * show it. This works out whether a question is asking for a change, and which
 * file of the change package it is about.
 *
 * Kept here rather than in the component so it can be tested: the cost of
 * getting it wrong is either a missed offer or an offer to edit the wrong file,
 * and the second is worse.
 */

/** Files of a change package that can be drafted against. */
export type EditableSpecFile = 'proposal.md' | 'tasks.md' | 'design.md' | (string & {})

/** Verbs that read as a request to change the text rather than explain it. */
const EDIT_VERBS = [
  'reword',
  'rewrite',
  'rephrase',
  'edit',
  'change the wording',
  'fix the wording',
  'clean up',
  'tidy up',
  'shorten',
  'expand',
  'clarify',
  'make it clearer',
  'add a section',
  'add a requirement',
  'add a task',
  'remove the',
  'delete the',
  'update the',
]

/**
 * Words that point at a file. Ordered longest-first within each file so
 * "spec delta" is tested before "spec".
 */
const FILE_HINTS: Array<[RegExp, EditableSpecFile]> = [
  [/\bproposal(\.md)?\b/i, 'proposal.md'],
  [/\bwhy\b|\bwhat changes\b|\bimpact\b/i, 'proposal.md'],
  [/\btasks?(\.md)?\b/i, 'tasks.md'],
  [/\bdesign(\.md)?\b/i, 'design.md'],
]

export interface EditRequest {
  /** The file the question is about. */
  file: EditableSpecFile
  /** The question, passed through as the instruction to the drafter. */
  instruction: string
}

/**
 * Work out whether a question is asking for an edit, and to which file.
 *
 * Both halves must be present. A question with an edit verb but no file
 * ("reword this") is ambiguous -- the change has four files and guessing which
 * one would be worse than not offering. A file with no edit verb ("what does
 * tasks.md say?") is an ordinary question and must stay one.
 *
 * `deltaFiles` are the change's actual delta paths, so a question naming one is
 * matched against files that exist rather than a guessed path.
 */
export function detectEditRequest(
  question: string,
  deltaFiles: readonly string[] = []
): EditRequest | null {
  const q = question.toLowerCase()
  if (!EDIT_VERBS.some((verb) => q.includes(verb))) return null

  // A delta named outright wins: it is the most specific thing the user can say.
  for (const file of deltaFiles) {
    const base = file.split('/').pop() ?? file
    const capability = file.split('/').slice(-2)[0]
    if (q.includes(file.toLowerCase())) return { file, instruction: question }
    // "the ai-ask spec" -- the capability folder is what people actually say,
    // and only when it is distinctive enough not to collide.
    if (capability && capability.length > 3 && q.includes(capability.toLowerCase())) {
      return { file, instruction: question }
    }
    if (base !== 'spec.md' && q.includes(base.toLowerCase())) {
      return { file, instruction: question }
    }
  }

  for (const [pattern, file] of FILE_HINTS) {
    if (pattern.test(question)) return { file, instruction: question }
  }
  return null
}
