# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

GitWyrm serves two primary groups:

- People who want Git's power without first learning Git jargon.
- People who already know Git and want common, multi-step work to take fewer clicks and stay in one focused interface.

Both groups use GitWyrm while working in local software repositories, often with several projects open at once. They need to understand what happened, what will happen next, and whether their work is safe. GitWyrm is currently a tool for an individual developer, not a team administration product.

## Product Purpose

GitWyrm is a Windows-first desktop Git client with Linux support. It makes repository history and day-to-day Git work easier to see, understand, and complete. Success means a beginner can work confidently without translating Git terminology, while an experienced user can finish routine workflows with less navigation and repetition.

Agent Desk is being developed in this worktree to bring repository-based planning and local AI-agent sessions into that same workflow. Its intended success is a person moving from a real repository task through planning, implementation, review, and an explicit landing decision without losing the connection to the project that started the work.

## Positioning

GitWyrm combines Git's full working power with plain-language, visual workflows. It brings patterns that commonly require several clicks or separate screens into a focused workspace without taking useful control away from experienced users.

Agent Desk extends that position by starting an AI session from a real repository object, such as an issue, pull request, OpenSpec change, commit, diff, or failed check, and returning the result to that object for review. This repository connection is the mechanism; GitWyrm does not claim that a specification alone guarantees better or safer generated code.

## Operating Context

- GitWyrm runs as a Tauri desktop application with a React interface and a Rust backend.
- Windows is the primary platform. Linux is also supported, with platform-specific packaging and desktop behavior.
- People work with local repositories, commit history, branches, tags, stashes, worktrees, changed files, diffs, merges, rebases, cherry-picks, reverts, and conflicts.
- Optional connected workflows include hosted issues and pull requests, commit identity profiles, signing, AI-assisted work, and OpenSpec planning through Spec Desk.
- Network operations use the system Git installation so the user's existing credentials and authentication tools continue to work.
- Agent Desk uses AI tools installed on the person's machine. Each provider retains its own login, quota, and native protocol.
- OpenSpec files remain the planning source of truth. GitWyrm reads and writes their existing project files rather than keeping a shadow task database.
- Agent runs may use isolated Git worktrees. Keeping, undoing, committing, pushing, and opening a pull request remain explicit human decisions.
- Session history and settings remain local files. GitWyrm has no required account or hosted service of its own.

## Capabilities and Constraints

- Local Git operations use `git2` in blocking worker tasks. Network operations use the system `git` executable.
- Commit-graph lane calculation belongs in the Rust backend; the frontend renders the result.
- Every user action must produce an immediate and unmistakable visible response, followed by confirmation when a slower result completes.
- User-facing language must explain outcomes in terms of the user's files and work, at about a 6th-grade reading level.
- Dangerous actions use a clear warning and a labeled confirmation button. GitWyrm never asks someone to type a word or phrase to confirm.
- The product must remain approachable for beginners without slowing down experienced users or hiding useful control.
- Generated frontend bindings are not edited by hand.
- Native desktop interaction is part of the product contract. Browser rendering, type checks, and builds do not replace verification in the shipped desktop environment.
- Agent Desk is under active development in this worktree. Commands, components, and passing tests do not establish product completeness without production wiring, live state and messages, result review, recovery, and signed-in provider acceptance.
- Provider integrations must work through their native protocols and existing sign-in flows, without requiring a user-installed bridge package.
- Agent actions must report what was verified and what was not. The product must never present incomplete or disconnected work as ready.

Open decisions remain around mobile or remote access, displaying provider quota details that providers do not expose directly, and a macOS build.

## Brand Commitments

- The product name is GitWyrm.
- The established voice is direct, calm, and plain. It describes visible outcomes rather than internal implementation details.
- Canonical product artwork lives in `src/assets/logo.png` and `src-tauri/icons/`. Spec Desk has its own established mark at `src/assets/specdesk-logo.png`.
- Deep Mint and the dark-first theme are established identity commitments, while supported themes must preserve the same hierarchy and interaction clarity.
- The product is local-first and account-free. The established public promise is a Git client with nothing to hide; claims must stay verifiable against the shipping app.
- User-facing product copy and release notes must not name competing applications.

## Evidence on Hand

- `README.md` documents the product architecture, development workflow, supported Git operations, Windows distribution, and release process.
- `src/components/modals/OnboardingModal.tsx` demonstrates the beginner-first onboarding language and hands-on practice repository.
- `src/lib/tutorialLessons.ts` contains real task-based guidance for repository gestures and workflows.
- `src/views/GraphView.tsx`, `src/views/DiffView.tsx`, `src/views/ConflictView.tsx`, `src/views/GithubView.tsx`, and `src/views/SpecDeskView.tsx` are working product surfaces.
- `docs/agent-desk/`, the related OpenSpec changes, and the Agent Desk source in this worktree record the intended architecture and implementation evidence. They are not, by themselves, proof that the full workflow is ready to ship.
- Real-binary tests exist for some provider and auditor paths. Signed-in native acceptance and complete start-to-result-to-landing evidence remain separate verification boundaries.
- `src/assets/logo.png`, `src/assets/specdesk-logo.png`, and `src-tauri/icons/` contain established brand assets.
- No customer testimonials, independent benchmarks, or adoption claims are recorded in this repository. Future work must not invent them.

## Product Principles

1. Make every action visible so people always know the app heard them.
2. Explain Git and agent work through files, tasks, and outcomes instead of requiring jargon.
3. Turn fragmented, multi-step patterns into clear workflows without limiting expert control.
4. Keep state honest and landing explicit: show what was verified, protect the user's work, and never claim incomplete work is ready.
5. Keep the repository and the user's machine in control; native desktop behavior and real workflows are the final measure of whether the product works.
