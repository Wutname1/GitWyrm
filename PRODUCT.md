# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Desktop application (Tauri v2, React front end, Rust backend). Windows first; Linux is
shipped too. No mobile client exists today; a phone companion is a stated ambition for
Agent Desk, not a product fact.

## Users

There is no single target user. GitWyrm is for people doing software development who
want the Git side of their work, and increasingly the AI-agent side, to take less effort.
They work on their own machine with several repositories open at once. Team features
(self-hosted, shared) do not exist yet, so the person using GitWyrm today is an
individual developer, not a team administrator.

Copy is written for a reader who does not know Git internals: plain language, no jargon
without explanation, nothing gated behind typing a phrase to confirm.

## Product Purpose

GitWyrm streamlines the Git and software development process. Today its job is to be a
fast Git client that keeps useful information at your fingertips: the commit graph,
working changes, and the issues and pull requests across all your repositories, so you
stay on top of them without leaving the app.

The next step, being built on a second branch as "GitWyrm Agent Desk", is an AI harness
in the family of OpenChamber and Paseo: it brings spec-driven development (SDD), Git and
your AI agents together so an app can go from idea to launched inside GitWyrm, with the
code editor reduced to a viewer. Success is a person doing that whole loop here.

## Positioning

Today: the fast Git client that surfaces issues and pull requests across repos, with
nothing to hide (local-first, no account, no tracking beyond crash reports).

By 1.0: the AI harness that lives beside your Git client. What a neighbouring agent
client cannot honestly copy is that an agent session is born from a real repository
object (an issue, a pull request, an OpenSpec change or task, a commit, a diff, a failed
check) and can return to it, inside the same tool that tracks the repository. Companies
are adopting SDD and OpenSpec because they are told a strong spec limits AI slop and
insecure patterns; developers adopt OpenSpec without a UI and read markdown files.
GitWyrm supplies the UI for the specs, the harness to create and implement them, and the
Git client to track the result. Whether a strong spec really limits slop is not a claim
GitWyrm makes.

## Operating Context

- Repositories on the person's own disk; several open at once in tabs.
- Git hosts (GitHub first) for issues, pull requests and checks; sign-in is handled by
  Git Credential Manager or device flow, never by a GitWyrm account.
- AI agents installed on the machine and driven as child processes: GitHub Copilot CLI,
  Claude Code, Codex, Gemini CLI, opencode. Each keeps its own login and quota.
- OpenSpec folders (`openspec/changes/<change>/{proposal,design,tasks}.md`) as the
  planning layer; GitWyrm reads and writes them through their own writer and never
  keeps a shadow task database.
- Isolated git worktrees per agent run; commit, push and pull request remain explicit
  human actions.
- Chat history, sessions and settings are files on disk. Settings sync is planned
  through the person's own cloud storage or a git gist, at no cost to the project.

## Capabilities and Constraints

Confirmed:
- Local git operations through libgit2; network operations through the system git so
  the existing credential setup works.
- Commit graph with lanes computed in Rust, virtualized for large repositories.
- Status, staging, commit, branches, tags, stashes, worktrees, submodules, diffs,
  merge and conflict resolution, cherry-pick, revert, rebase.
- Fetch, pull, push, clone with streamed progress; auto-update via a CDN bootstrapper.
- GitHub pull requests and issues across repositories.
- Agent Desk (second branch): sessions with Source + Intent + Conversation +
  Execution; Ask, Plan and Auto modes; solo or lead-plus-helpers teams; an
  independent auditor that checks finished work; per-chat AI tool choice; usage
  reporting only from what a provider states.
- Terminology: "chat" or "session" for an agent conversation; "helper" for a
  sub-agent; "project" for a repository in user-facing copy; "Keep" for accepting an
  agent's changes.

Constraints:
- Windows first, Linux supported; no macOS commitment recorded.
- Local-first, no accounts, no hosted infrastructure of GitWyrm's own; nothing may
  require a GitWyrm server.
- Plain-language copy at roughly a 6th-grade reading level; never type-to-confirm.
- Every user action must produce a visible response.
- Never reference competing apps in user-facing text.

Undecided (recorded, not invented):
- Whether plan quota lines per AI provider will ever be shown (needs credential
  scraping the project has not chosen to do).
- Mobile or remote access design (constraints noted in the Agent Desk plan: no opened
  ports, rendezvous handoff only, phone is plainly offline when the desktop sleeps).
- A macOS build.

## Brand Commitments

- Name: GitWyrm. Logo: `logo.png` / `logo.ico`. Wordmark font: `Sora-SemiBold.ttf`.
  All binding as they are.
- Deep Mint accent and the dark-first theme are a commitment, not merely the
  incumbent look.
- Voice: dev-honest and plain. Marketing angle on the website: "A Git client with
  nothing to hide". Claims must stay verifiable against the app repository.

## Evidence on Hand

- The shipping app itself and its README (`README.md`), architecture and readiness
  notes for Agent Desk (`docs/agent-desk/`), and the marketing site in a separate
  repository (`C:\code\GitWyrm-Website`).
- Real-binary test runs proving Codex and Copilot chats and the auditor loop work
  (`src-tauri/src/airun/cli_run.rs`, `src-tauri/tests/copilot_acp.rs`).
- No testimonials, customer logos, benchmarks, pricing or download counts exist;
  future work must not fabricate them. GitWyrm is free and MIT licensed; there is no
  code signing today.

## Product Principles

1. Put the information a developer needs in front of them before they go looking:
   issues, pull requests, changes and agent progress are one glance away.
2. Fast first. A Git client that hesitates loses to the terminal.
3. Honest state. Show what was verified, say plainly what was not, never claim work
   was checked or changes are ready when they are not.
4. The repository is the source of truth: agent sessions start from real repo objects,
   specs live in their own files, and landing work is always an explicit human choice.
5. Nothing to hide: local-first, no accounts, no hosted dependency, copy anyone can
   read.
