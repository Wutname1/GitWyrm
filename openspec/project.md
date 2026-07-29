# Project Context

## Purpose

GitWyrm is a Windows desktop git client for people who are not git experts. It shows every
action's result immediately, speaks plain language, and never gates an action behind
retyping a word. This OpenSpec tree tracks the OpenSpec-integration feature itself: a
"Specs" surface in the main window, a popout "Spec Desk" window, and in-app AI runs
powered by the user's own AI provider.

The reference design is the interactive mockup at `docs/openspec-mockup-v3.html`.
Every requirement in `changes/` traces back to a behavior demonstrated there.

## Tech Stack

- Tauri v2 (Rust backend, multi-window), React + TypeScript frontend
- git2-rs with shell-git fallbacks; typed IPC via specta-generated `src/lib/bindings.ts`
  (generated file - never hand-edit; regenerate with the bindings script)
- Zustand stores (`src/stores/`), TanStack Query for git data (`src/hooks/useGitQueries.ts`)
- Tailwind + design tokens in `src/index.css` (Slate-dark, Deep Mint accent
  `--gw-accent: #1db584` / `--gw-accent-text: #38b78e`)
- BYO-AI layer for commit messages (Copilot device flow, Anthropic, local models) -
  the AI features here reuse it, never a second auth path

## Project Conventions

### House rules (non-negotiable)

1. **Visible change**: every user action produces an immediate visible UI response.
2. **Plain language**: no git or AI jargon in primary copy. Approved framings:
   "handoff" (not "prompt"), "what the AI reads" (not "context"). No token counts,
   model IDs, or "agent loop" in primary UI.
3. **No type-to-confirm**: destructive actions use a normal plain-language confirm
   dialog, never "type the name to continue".

### Code style

- Components in `src/components/domain/`, views in `src/views/`, modals in
  `src/components/modals/`. Errors classified via `src/lib/errorClass.ts`; prefer typed
  outcome enums over throwing for expected conditions.
- Commit messages: `new:` / `improved:` / `fixes:` prefixes, first line is user-facing
  plain language (feeds the changelog).

### Architecture for this feature

- **Files are the database.** All spec state lives in `openspec/` markdown in the user's
  repository. GitWyrm parses and watches it; it writes only what a human would write
  (checkbox ticks, new-change scaffolds, archive moves). No sidecar database.
- **Two windows, one state.** The main window shows ambient status only; the Spec Desk
  popout holds all deep spec work. Both render the same parsed state and update together.
- **AI is optional everywhere.** Every AI feature has a working no-AI path (copy
  handoffs). AI entry points are hidden - not greyed out - when no provider is set up.

### Testing

- `npm run typecheck` and unit tests for parser/composer logic
- UI verification in a real native window (not just devtools) before a change is called
  done - see `dev-vs-prod-app-testing` conventions

## Domain Context

OpenSpec (the methodology this feature integrates) keeps a `openspec/` folder in a repo:
`changes/<change-id>/` holds `proposal.md`, `tasks.md` (checkbox list = progress),
optional `design.md`, and spec deltas under `specs/<capability>/spec.md` using
`## ADDED|MODIFIED|REMOVED Requirements` headers. `openspec validate` checks structure;
`openspec archive` merges deltas into the top-level `specs/` library and moves the
change to `changes/archive/`.

## Important Constraints

- The Spec Desk is a second OS window; the main window must stay fully usable while it
  is open, including while an AI run is active in the Desk.
- An embedded AI run may only: edit files in the repo working tree on the linked branch,
  run the project's own checks, and create commits the user approves. It never pushes.
- External handoff (copy / opencode / VS Code) must remain one click away even when AI
  is configured - it is an option being *supplemented*, not replaced.

## External Dependencies

- `openspec` CLI (optional): used for `validate` and `archive` when installed; GitWyrm
  bundles a read-only parser fallback so viewing never requires the CLI
- The user's AI provider via the existing BYO-AI settings (Copilot / Anthropic / local)
