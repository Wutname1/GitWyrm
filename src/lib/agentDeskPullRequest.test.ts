import { describe, expect, it } from 'vitest'
import { pullRequestUrlWithDraft } from './agentDeskPullRequest'

describe('pullRequestUrlWithDraft', () => {
  it('fills in the GitHub form and keeps it expanded', () => {
    const url = new URL(
      pullRequestUrlWithDraft(
        'https://github.com/acme/widgets/compare/fix-parser?expand=1',
        'Fix the parser',
        'Spec: add-the-thing'
      )
    )
    expect(url.searchParams.get('title')).toBe('Fix the parser')
    expect(url.searchParams.get('body')).toBe('Spec: add-the-thing')
    // Without this GitHub shows the comparison, not the form.
    expect(url.searchParams.get('expand')).toBe('1')
    expect(url.pathname).toBe('/acme/widgets/compare/fix-parser')
  })

  it('uses each host its own parameter names', () => {
    const gitlab = new URL(
      pullRequestUrlWithDraft('https://gitlab.com/acme/widgets/compare/fix', 'T', 'B')
    )
    expect(gitlab.searchParams.get('merge_request[title]')).toBe('T')
    expect(gitlab.searchParams.get('merge_request[description]')).toBe('B')

    const bitbucket = new URL(
      pullRequestUrlWithDraft('https://bitbucket.org/acme/widgets/compare/fix', 'T', 'B')
    )
    expect(bitbucket.searchParams.get('title')).toBe('T')
    expect(bitbucket.searchParams.get('description')).toBe('B')

    const azure = new URL(
      pullRequestUrlWithDraft('https://dev.azure.com/acme/widgets/pullrequestcreate', 'T', 'B')
    )
    expect(azure.searchParams.get('title')).toBe('T')
    expect(azure.searchParams.get('description')).toBe('B')
  })

  it('leaves an unfamiliar host and a non-URL exactly as they came', () => {
    const unknown = 'https://git.example.internal/acme/widgets/compare/fix'
    expect(pullRequestUrlWithDraft(unknown, 'T', 'B')).toBe(unknown)
    expect(pullRequestUrlWithDraft('not a url', 'T', 'B')).toBe('not a url')
  })

  it('omits an empty field rather than sending a blank one', () => {
    const url = new URL(
      pullRequestUrlWithDraft('https://github.com/acme/widgets/compare/fix', 'Title only', '   ')
    )
    expect(url.searchParams.get('title')).toBe('Title only')
    expect(url.searchParams.has('body')).toBe(false)
  })
})

describe('an existing pull request page is not a form to prefill', () => {
  // Why the "update" path copies the description to the clipboard instead of
  // putting it in the link: this builder works by adding query parameters to
  // a host's COMPARE page, which is what opens a new-pull-request form. An
  // already-open pull request's own page has no such form and ignores them.
  //
  // The tempting "fix" is to route the update path through this function too.
  // That produces a URL the host quietly ignores, which is worse than the
  // honest copy button because it looks like it worked.
  it('would silently produce a URL the host ignores, if misused this way', () => {
    // Pinned as a WARNING, not an endorsement: this function cannot tell a
    // compare page from a pull request page, so it appends parameters to
    // both. That is the trap. The assertion records that the output for a
    // pull request page is a link carrying text the host will drop -- which
    // is why the dialog copies the description instead of passing it here.
    const existing = 'https://github.com/o/r/pull/42'
    const withDraft = pullRequestUrlWithDraft(existing, 'A title', 'A body')
    expect(withDraft).toContain('title=A+title')
    expect(withDraft).toContain('/pull/42')
    // The real compare-page case, which DOES work, for contrast.
    const compare = pullRequestUrlWithDraft('https://github.com/o/r/compare/main...x', 'A title', 'A body')
    expect(compare).toContain('expand=1')
  })
})
