# AGENTS.md

Guardrails for AI agents and contributors in GitWyrm.

## Rule #1 - every user action has a visible change

**Every user action must produce a visible response.** A click, a keypress, a
drag - if the user did something, the UI must acknowledge it immediately and
unmistakably: a selection state, a flash, a toast, a spinner, a moved element,
a color change. Never let an action complete silently. If the real result is
slow or off-screen, show intent right away (highlight the target, scroll it into
view, pulse it) and confirm the outcome when it lands.

This is the first thing to check in any UI review. A handler that mutates state
the user can't see is a bug, not a feature.

## Rule #2 - simplify for beginners

**GitWyrm's goal is to make git easy for people who don't know git.** Don't
expose raw git concepts and options as a checklist. Combine related operations
into a single, easy-to-understand modal - use arrows, icons, diagrams,
before/after previews, whatever makes the choice visual instead of verbal.
If a user has to know what "reflog" or "--force-with-lease" means to use a
feature, the feature isn't done yet.

All user-facing text - labels, buttons, dialog copy, tooltips, error
messages - is written in plain language at about a 6th-grade reading level.
Prefer "Switch branches" over "Checkout ref"; prefer "This will erase your
changes" over "This operation is destructive and irreversible." Explain what
will happen in terms of the user's files and work, not git's internal model.

This is the second thing to check in any UI review, right after Rule #1: does
this screen assume git knowledge the user might not have, and could a picture
or a plain sentence replace the jargon?

## Rule #3 - never make the user type to confirm

**No action is ever gated behind typing a word or phrase.** No "type DISCARD to
confirm", no "type the branch name to continue", no typed confirmation of any
kind, anywhere. If an action is dangerous, guard it with a plain confirm dialog
(a clear warning + a labeled button), never a text box the user has to copy a
word into. `ConfirmDialog` intentionally has no typed-phrase option - don't add
one back.

## What this repo is

GitWyrm - a fast, focused Git client for Windows. Tauri v2 shell, React +
TypeScript + Tailwind v4 + shadcn/ui frontend, Rust `git2` backend. See
`README.md` for the full architecture and dev commands; see `C:\code\CLAUDE.md`
for the shared house style across all of these repos.

- **Local git ops** use `git2` (libgit2) in `spawn_blocking`; **network ops**
  (fetch/pull/push/clone) shell out to system `git.exe` so Git Credential
  Manager handles auth. Keep that split.
- **`src/lib/bindings.ts` is generated** by tauri-specta - never hand-edit it.
  It regenerates on `npm run tauri dev`. Change the Rust command, not the binding.
- **Graph lanes** are computed in Rust during the revwalk; the frontend is a dumb
  renderer. Keep lane logic in `src-tauri/src/git/graph.rs`, not in TSX.

## Hard rules

- **TypeScript strict, no `any`.** Run `npm run typecheck` before committing.
- **No `print!`/`println!`/`console.log` left in.** Use the app's logging.
- **Never reference other apps** (competing Git clients, example projects) in
  user-facing text or commit messages.
- **NEVER `git push`** unless explicitly told to. Pushes trigger CI/CD and
  releases. Committing to `main` is fine pre-release.
- **Don't commit a broken build.** When fixing a bug, don't commit until the fix
  is confirmed working - avoid a trail of "fixes X" commits that need squashing.

## Commit messages (adopted from the HearthShelf changelog convention)

Commit **subjects are end-user-facing**: an automated changelog walks
`git log` between release tags, categorizes each subject by its prefix, strips
the prefix, and publishes it. Write the first line for a user ("what changed for
me?"), plain language, ~6th-grade reading level. Put technical detail in the body.

### Prefix -> changelog section

Use one of these prefixes. The prefix is stripped from the published text.

| Prefix | Section | Use for |
| --- | --- | --- |
| `new:` (also `feat:` / `feature:` / `enhancement:`) | Features | Brand-new functionality |
| `improved:` (also `chore:` / `refactor:` / `perf:` / `style:`) | Changes | Enhancements to existing features |
| `fixes:` (also `fix:` / `bug:` / `bugfix:`) | Fixes | Bug fixes |
| `docs:` (also `documentation:`) | Docs | Documentation-only changes |
| `breaking:` | Breaking | Backward-incompatible changes |

A leading natural-language verb also categorizes (e.g. `Add ...`, `Fix ...`,
`Improve ...`, `Remove ...`), but prefer the explicit prefix.

**Anything that matches no prefix or known verb falls through to "other" and is
silently dropped from the changelog.** So a real user-facing change must carry a
prefix, and pure noise (dependency bumps, CI tweaks) can be left prefix-less on
purpose to keep it out - or tagged `chore:`/`improved:` if it should show as a change.

### Explicit tags: `[tag]` / `#tag`

To tag a changelog line, add a trailing marker to the subject: `[tag]` or
`#tag`. Multiple are allowed. They are stripped from the displayed text and
lower-cased into tag slugs. Content-based auto-tagging is done server-side on the
website - here you only pass through what you deliberately mark.

```
fixes: Diff view no longer jumps to the top after staging a hunk #diff
new: Stage individual lines from the gutter [staging] #diff
improved: Commit graph draws a WIP node for uncommitted changes #graph
```

### Writing style

- **Summary (first line):** plain user language - "what changed for me?"
- **Body (optional):** technical detail, file changes, API references, the *why*.
- **No em dashes.** Use hyphens or rewrite.
- **Reserve `new:`** for genuinely new features, not tweaks to existing ones.

### Example

```
fixes: WIP node click now flashes the Changes panel #graph

Selecting the synthetic WIP row bumps a focus nonce that RightPanel
watches; it scrolls the CHANGES header into view and flashes its border
so the action has a visible result (Rule #1).
Files: src/views/GraphView.tsx, src/stores/uiStore.ts, src/components/domain/RightPanel.tsx
```
