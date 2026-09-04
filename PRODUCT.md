# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

GitWyrm serves two primary groups:

- People who want Git's power without first learning Git jargon.
- People who already know Git and want common, multi-step work to take fewer clicks and stay in one focused interface.

Both groups use GitWyrm while working in local software repositories. They need to understand what happened, what will happen next, and whether their work is safe.

## Product Purpose

GitWyrm is a Windows-first desktop Git client with Linux support. It makes repository history and day-to-day Git work easier to see, understand, and complete. Success means a beginner can work confidently without translating Git terminology, while an experienced user can finish routine workflows with less navigation and repetition.

## Positioning

GitWyrm combines Git's full working power with plain-language, visual workflows. It brings patterns that commonly require several clicks or separate screens into a focused workspace without taking useful control away from experienced users.

## Operating Context

- GitWyrm runs as a Tauri desktop application with a React interface and a Rust backend.
- Windows is the primary platform. Linux is also supported, with platform-specific packaging and desktop behavior.
- People work with local repositories, commit history, branches, tags, stashes, worktrees, changed files, diffs, merges, rebases, cherry-picks, reverts, and conflicts.
- Optional connected workflows include hosted issues and pull requests, commit identity profiles, signing, AI-assisted work, and OpenSpec planning through Spec Desk.
- Network operations use the system Git installation so the user's existing credentials and authentication tools continue to work.

## Capabilities and Constraints

- Local Git operations use `git2` in blocking worker tasks. Network operations use the system `git` executable.
- Commit-graph lane calculation belongs in the Rust backend; the frontend renders the result.
- Every user action must produce an immediate and unmistakable visible response, followed by confirmation when a slower result completes.
- User-facing language must explain outcomes in terms of the user's files and work, at about a 6th-grade reading level.
- Dangerous actions use a clear warning and a labeled confirmation button. GitWyrm never asks someone to type a word or phrase to confirm.
- The product must remain approachable for beginners without slowing down experienced users or hiding useful control.
- Generated frontend bindings are not edited by hand.
- Native desktop interaction is part of the product contract. Browser rendering, type checks, and builds do not replace verification in the shipped desktop environment.

## Brand Commitments

- The product name is GitWyrm.
- The established voice is direct, calm, and plain. It describes visible outcomes rather than internal implementation details.
- Canonical product artwork lives in `src/assets/logo.png` and `src-tauri/icons/`. Spec Desk has its own established mark at `src/assets/specdesk-logo.png`.
- User-facing product copy and release notes must not name competing applications.

## Evidence on Hand

- `README.md` documents the product architecture, development workflow, supported Git operations, Windows distribution, and release process.
- `src/components/modals/OnboardingModal.tsx` demonstrates the beginner-first onboarding language and hands-on practice repository.
- `src/lib/tutorialLessons.ts` contains real task-based guidance for repository gestures and workflows.
- `src/views/GraphView.tsx`, `src/views/DiffView.tsx`, `src/views/ConflictView.tsx`, `src/views/GithubView.tsx`, and `src/views/SpecDeskView.tsx` are working product surfaces.
- `src/assets/logo.png`, `src/assets/specdesk-logo.png`, and `src-tauri/icons/` contain established brand assets.
- No customer testimonials, independent benchmarks, or adoption claims are recorded in this repository. Future work must not invent them.

## Product Principles

1. Make every action visible so people always know the app heard them.
2. Explain Git through files, work, and outcomes instead of requiring jargon.
3. Turn fragmented, multi-step patterns into clear workflows without limiting expert control.
4. Protect the user's work with understandable previews, warnings, and recovery paths.
5. Treat native desktop behavior and real repository interaction as the final measure of whether a workflow works.
