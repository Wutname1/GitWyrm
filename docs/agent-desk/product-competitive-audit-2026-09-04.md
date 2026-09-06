# Agent Desk: product and competitive audit

Date: 2026-09-04. Internal product research, not public product copy.

Worktree: `C:\code\GitWyrm\.claude\worktrees\agent-desk-openchamber-audit-0f40fb`

Source baseline: `0c17193a5ffed301f4e4c53558070f0a19a9ac78`. An existing change to generated `src/lib/bindings.ts` was present and was not modified. Source references below are relative to this worktree and use this snapshot's line numbers.

## Product verdict

Agent Desk has a credible purpose: a comfortable daily chat workspace that turns the repository work already in front of you into agent work, with OpenSpec supplying the intended outcome. The most promising experience is selecting an issue, PR, or exact spec task and having the right project, source, instructions, and execution settings arrive with it. A person can then steer a capable lead agent, inspect its helpers when needed, and finish the change through the Git client they already use.

The competitive bar has moved. Issue kickoffs, source context, isolated branches, multiple agents, review, and remote access are already documented by competitors. Even an independent auditor driving goal continuation is now documented by OpenChamber. These are useful capabilities to deliver, but weak claims of uniqueness. Our opportunity is the quality of the complete OpenSpec-to-working-change experience, combined with genuinely usable configuration and session portability across agent backends. Neither advantage is established merely by having commands, panels, or specifications for it.

The current worktree has substantial implementation beyond earlier audits. It also has concrete interoperability problems that undermine the portability promise. I would prioritize those and native workflow acceptance before expanding the visible feature surface.

## Evidence and limits

- Current source inspection of execution entry points, usage, imports, configuration readers/writers, and mounted workspace components.
- Current first-party competitor documentation retrieved September 4, 2026. Documentation is evidence of a documented offering, not a hands-on reliability benchmark. Preview features remain identified as preview.
- Local OpenChamber source at `F:\openchamber`, revision `504fd8532`, used to verify its actual MCP ownership and schema. That checkout and current online documentation are separate snapshots.
- Independent UI assessments: A, `/root/ux_review`, examined interaction structure; B, `/root/ux_evidence`, ran the design detector and checked runtime availability. Neither saw the other's findings before synthesis. Impeccable guidance informed this portion.
- No verified worktree preview was running. The installed GitWyrm process does not establish what this worktree renders, and native computer inspection was unavailable. No current screenshots, native clicks, authenticated provider runs, or performance measurements are claimed.
- Existing test results in earlier audits were not rerun or presented as current evidence. This is a product/source audit with a documentation deliverable, not a release certification.

## Competitive comparison

| Product | Evidence-backed strength | Implication for Agent Desk |
| --- | --- | --- |
| OpenChamber | Chat workspace around OpenCode; documented issue/PR starts, multi-run model comparisons, usage/quota views, and audited session goals. [Overview](https://docs.openchamber.dev/), [GitHub workflows](https://docs.openchamber.dev/github/), [multi-run](https://docs.openchamber.dev/multi-run/), [goals](https://docs.openchamber.dev/session-goals/), [usage](https://docs.openchamber.dev/usage/) | Closest daily-use reference. Preserve dense chats and quick controls. An issue source, team panel, or goal button alone is insufficient differentiation. |
| GitKraken / Kepler | Kepler documents issue/PR context attached to agent work and a path to review; its public-preview positioning includes work across repositories and agents. [Getting started](https://help.gitkraken.com/kepler/kepler-getting-started/), [public preview](https://gitkraken.com/blog/kepler-is-in-public-preview-one-task-every-repo-every-agent), [agent integrations](https://support.gitkraken.com/kepler/agent-integrations/) | Direct competition for repository-driven delegation. Git integration must reduce user effort measurably; its existence is not the differentiator. |
| VS Code / Copilot | A dedicated chat-oriented Agents window is in preview. Session management documents opening and continuing sessions from several agent tools. [Agents window](https://code.visualstudio.com/docs/agents/run/agents-window), [sessions](https://code.visualstudio.com/docs/agents/run/sessions/manage-sessions) | Chat-first multi-project UI and cross-tool session management are becoming expected. A simpler experience for Git users is our opening. |
| Codex app | Worktrees, parallel work, skills, automations, and remote access are documented. [App introduction](https://openai.com/index/introducing-the-codex-app/), [remote work](https://openai.com/index/work-with-codex-from-anywhere/) | Avoid rebuilding generic agent controls at the expense of spec/repository workflows. Native provider capabilities should inform our adapters. |
| Claude Code | Agent teams document shared tasks, inter-agent messages, and a coordinating lead; this is an experimental feature. [Agent teams](https://code.claude.com/docs/en/agent-teams) | Lead-plus-helpers is an established interaction pattern. Distinguish our dependency-driven delivery and acceptance experience from simply starting several agents. |
| OpenCode | Primary agents, subagents, and configurable permissions are documented. Current documentation has version-specific configuration differences. [Agents](https://opencode.ai/docs/agents), [V2 MCP](https://opencode.ai/v2/docs/mcp-servers) | Use a versioned compatibility contract. An abstraction that erases provider capabilities or assumes one timeless schema will age badly. |
| Paseo | Desktop/mobile orchestration, a headless daemon, optional encrypted relay, CLI, and SDK are documented. [Repository and setup](https://github.com/getpaseo/paseo) | Remote use is already a practical workflow. Establish a transport boundary early, while sequencing the full mobile service after desktop correctness. |
| Orca | The stablyai project advertises a broad CLI-agent fleet across desktop, mobile, and VPS. [Repository](https://github.com/stablyai/orca) | Backend breadth is useful, but we should preserve the user's preferred chat interface. This audit assumes stablyai/orca is the Orca previously discussed; it does not generalize to unrelated projects with that name. |

Absence of OpenSpec in these sampled sources does not prove a competitor cannot support it through skills or plugins. Our claim must be better built-in workflow integration, demonstrated through use, rather than exclusivity.

### What to carry, differentiate, and defer

**Carry as baseline:** one-line session rows; project/time grouping; clear backend identity; streaming messages and tool output; provider-exposed reasoning summaries; history jump; compact mode/team controls; split view; movable/pinnable details; session/helper usage; reliable stop and recovery. The user's previous OpenChamber screenshots establish the desired direction, but do not prove current GitWyrm visual parity.

**Invest in differentiation:** an exact, refreshable source; OpenSpec acceptance tied to delivered changes; a lead coordinating bounded helper work; configuration that actually loads in the destination app; continuity when switching providers without claiming a new handoff is a native resumed session. Treat these as product hypotheses to validate.

**Defer broad expansion:** a proprietary plugin marketplace, a complete editor or terminal workspace, model-comparison tournaments, a general automation platform, and extensive cosmetic customization. Keep plugin compatibility and remote access on the roadmap. Do not make read-only operation the product's identity; explain Plan/Auto and the actual authority of the current run.

## Current implementation: corrections to older findings

| Earlier concern | Current source evidence | Current conclusion |
| --- | --- | --- |
| Spec Desk starts an independent run without Agent Desk durability | `src/hooks/useStartRun.ts:39-70` delegates through `useStartAgentSession`, passing repository identity explicitly | The old bypass and empty-window-store lookup are addressed. Native acceptance still needed. |
| PR draft disappears into a toast | `src/components/domain/agent-desk/ResultReviewPanel.tsx:813-870` retains title/body and opens an editable draft | Do not report the discarded-draft bug again. This does not prove all host-specific PR workflows. |
| Missing MCP writers for Codex, Copilot, OpenChamber | `src-tauri/src/agent_config/registry.rs:146-229` now declares writers | Writers exist. The important remaining question is whether their output is consumed correctly; findings F1-F2 show why that distinction matters. |
| No skill-copy implementation | `src-tauri/src/commands/agent_config.rs:260,512,610` routes skills through `skill_write.rs` | Directory-copy mechanics exist. Cross-client coverage is still restricted; see F3. |
| Usage promises overall totals and forgets collapse state | `SessionUsageCard.tsx:27-29,63-66` uses the UI store and labels figures as this chat and its agents | These older UI findings are addressed. Account limits remain absent. |
| OpenChamber detection looks only in Windows AppData | `src-tauri/src/agentdesk/adapters/openchamber.rs:57-79` honors its data-directory override and home-based default | Detection-path bug addressed; import still unsupported. |
| Goal/remote/plugin directions have no granular plans | `openspec/changes/agent-desk-{goal-continuation,remote-access,plugin-compatibility}/tasks.md` exist with unchecked work | Planned, not delivered. Do not call the packages missing. |

## Prioritized findings

Priority means product impact and sequence, not an assertion that every item is a runtime defect. P1 items threaten a core promise; P2 items reduce usability or competitive completeness. No P0 is established by this audit.

### F1. P1: configuration writers target files that current clients do not use for this purpose

**Type:** source-confirmed interoperability mismatch; destination-app reproduction remains outstanding.

The Copilot registry selects home-relative `.config/Code/User/settings.json` and repository `.vscode/settings.json` (`agent_config/registry.rs:190-209`). Location discovery joins these to the home/repository root (`locations.rs:76-91`). Current VS Code documents workspace `.vscode/mcp.json` and profile `mcp.json`; its Agent Host has a separate portable configuration contract. The current registry misses these documented paths. [VS Code MCP reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)

The OpenChamber registry targets `.config/openchamber/config.json` with `mcpServers` or `mcp.servers` (`registry.rs:215-228`). Its local implementation instead reads merged OpenCode configuration and the `mcp` map (`F:\openchamber\packages\web\server\lib\opencode\mcp.js:28-50`). This is configuration ownership, not just a different filename.

**User impact:** discovery can miss an existing connector, or Copy can create a configuration file without making the connector available in the intended client.

**Required outcome:** version- and scope-aware location resolution, including overrides and profiles. Recognize when OpenChamber and OpenCode share the same underlying configuration so they are not treated as unrelated copies. Verify each supported destination by loading the result in that actual client. Do not mark a destination supported merely because a fixture round-trips.

### F2. P1: copying an MCP entry does not translate its client-specific schema

**Type:** source-confirmed interoperability defect for incompatible entry shapes.

`commands/agent_config.rs:460` passes `source_item.extra` directly to `writers::build_new_content`. `agent_config/writers.rs:59-82` wraps those unchanged fields in a destination map/table. Reader fields retain the client's original values.

For example, a command-string plus separate `args`/`env` entry is not an OpenCode local entry with a command array and `environment`. OpenChamber's OpenCode implementation explicitly handles those shapes at `mcp.js:180-184,249-263`. Changing the outer map name does not convert the entry. New OpenCode versions also warrant separate fixtures rather than silently inheriting old assumptions.

**Required outcome:** parse a normalized connector model, then serialize for the destination/version. Preserve source-only fields as explicit compatibility warnings, not blindly transplanted executable configuration. Cover local command/arguments, HTTP transport, environment references, inputs, and authentication requirements. A simple executable connector must start in both clients after copy; missing credential setup must be stated separately from successful file creation.

### F3. P1 for the portability promise: skill copying has mechanics but almost no cross-client reach

**Type:** coverage gap plus misleading failure copy.

`agent_config/skills.rs:205-229` returns skill directories only for Claude Code. `commands/agent_config.rs:260-289` uses that same function for copy destinations. Codex, OpenCode, Copilot, and OpenChamber therefore have no destination through this path. The message says the client “does not keep skills,” confusing GitWyrm's missing support with the client's capabilities.

**Required outcome:** add verified skill discovery and destinations incrementally, including the entire skill directory, scopes, collisions, preview, and undo. Replace the false capability assertion with “GitWyrm cannot copy skills to this tool yet.” Demonstrate one real cross-client copy before advertising a Skill/MCP Manager that keeps apps in sync. Copy-on-demand and ongoing synchronization must have distinct labels and state.

### F4. P1 roadmap correction: quota is a real product gap, not universally inaccessible data

**Type:** missing competitive capability and overbroad prior rationale.

The chat/helper view correctly distinguishes absent data and estimates. However, `commands/agent_desk.rs:2622-2623` always returns no plan limit/reset. September 3's audit argues that parity would require credential scraping because ACP does not expose quota. That inference is too broad: the documented Codex app-server exposes `account/rateLimits/read`. Whether an installed version/account supplies it must be negotiated and tested. [Official protocol](https://raw.githubusercontent.com/openai/codex/main/codex-rs/app-server/README.md)

OpenChamber documents provider quota windows and pacing, which directly answers the user's concern about running out of weekly usage. [Usage and quotas](https://docs.openchamber.dev/usage/)

**Required outcome:** provider-specific account-limit adapters where supported, with account identity, timestamp, reset time, and unavailable states. Keep account quota separate from session cost and helper totals. Start with a supported native API; do not promise every backend has one. Never infer remaining quota from the local transcript.

### F5. P1 UX: two controls give competing answers about which AI is active

**Type:** source-evidenced control-model mismatch.

`AgentDeskTitleBar.tsx:76` mounts the older `AiProviderChip`; that component uses global AI state and Spec Desk hooks (`spec-desk/AiProviderChip.tsx:32-40`). The chat independently selects a provider in `ProviderControl.tsx:45`; its Send eligibility is separate (`SessionComposer.tsx:271`). A titlebar setting and a session setting can communicate different identities or availability. This is not proof that execution bypasses a backend disable.

**Required outcome:** one obvious answer for the active chat: project, backend, model, mode. If the global control remains for planning tools, label its scope explicitly. Verify changing either control does not misrepresent the next send, especially in split view.

### F6. P1 UX: Pin Right can acknowledge a panel that is not visible

**Type:** source-traced interaction defect; native reproduction outstanding.

`PaneDetailPopover.tsx:178` invokes pin and closes. `AgentDeskView.tsx:612-624` stores the right dock and reports success without the width rejection used by toolbar pinning at `645-657`. Below 900px, `agentDeskDockPlacement.ts:71-86` resolves this placement to a popover fallback, and the view does not show the pinned panel (`739-745`). This violates Rule #1's visible-result requirement.

**Required outcome:** use one placement decision for all pin actions. Move the panel to a visible permitted edge or keep the popover open with an explanation. Acceptance must cover popover and toolbar entry points at narrow width and in split view.

### F7. P2: the first-chat experience asks the same setup questions twice

**Type:** product interaction friction.

`NewChatLanding.tsx:63-165` displays project, AI, authority, and team choices above the message box. `SessionComposer.tsx:439-534` mounts it and repeats mode/team/provider controls. Defaults mean this is not a mandatory wizard, but the visual structure makes starting a conversation feel more involved than necessary.

**Required outcome:** retain current-repo default and an obvious “Open another project” path. Show source/project and one compact settings row around the composer; expand settings only when asked. Current choices are drawn from current/open/recent repositories (`SessionComposer.tsx:125-136`), so selecting a previously unseen project also needs a direct path. Keep dense one-line session rows.

### F8. P2: portability stops at the precise migration the user is most likely to want

**Type:** incomplete feature, honestly disabled.

`agentdesk/adapters/openchamber.rs:3,114-122,159` remains detection-only and reports unsupported continuation. The improved folder detection does not import a chat. This limits adoption for someone whose current daily work is in OpenChamber.

**Required outcome:** implement and fixture-test discovery/read through a supported OpenCode/OpenChamber interface; preserve project, origin, messages, and timestamps. Label imported history, a new continuation here, and native external resume separately. Verify duplicate imports and source updates. Do not manipulate another client's live database just to claim broad coverage.

### F9. P2 strategic gap: goal continuation and remote access are already competitive expectations

**Type:** planned future capability.

The goal and remote task packages remain unchecked. OpenChamber now documents continuation with independent auditing, and Paseo documents a headless/remote workflow. Our eventual goal feature can still be valuable by judging progress against specific OpenSpec acceptance and showing why it continued. Its existence alone will not distinguish the product. [OpenChamber goals](https://docs.openchamber.dev/session-goals/), [Paseo](https://github.com/getpaseo/paseo)

**Required outcome:** first stabilize the execution lifecycle, then deliver bounded goal continuation with stop, failure, budget, and restart behavior. Establish the transport-neutral service boundary before deeply coupling more controls to window-local state. Keep full remote pairing/mobile delivery later, with explicit offline/sleeping-host behavior.

### F10. P1 release evidence: another source audit cannot establish daily-driver quality

**Type:** verification gap.

`docs/agent-desk/release-readiness.md` still identifies native acceptance as outstanding. This pass found no running worktree preview. A product that competes on comfort and confidence needs direct evidence of message delivery, readable output, usable panels, and recoverable work under real provider behavior.

**Required outcome:** execute the acceptance scenarios below and attach screenshots/logs and exact versions. Fix what those workflows reveal before adding another broad family of features. Repeated source audits remain useful for contracts, but cannot replace this gate.

## Workspace design assessment

The code supports the intended direction: dense searchable sessions, backend logos on rows, genuine per-pane ownership, archive recovery, and saved layout choices. The source-linked conversation and OpenSpec/result relationship give the workspace a product-specific purpose.

The independent review's provisional heuristic score was **24/40, source-only**: status 2, real-world match 3, control 3, consistency 2, prevention 3, recognition 2, efficiency 3, minimalism 2, recovery 2, help 2. This is not comparable to a rendered usability score or a competitor benchmark. The main costs are conflicting AI identity, repeated setup, and panel discoverability.

Secondary improvements: add tooltips or short labels to detail icons (`PaneDetailPopover.tsx:68-94`); apply consistent team/graph availability in the header and Panels menu; offer actual retry/open actions beside shell failures (`AgentDeskView.tsx:765-788`). Keep the graph as optional detail that explains current work. Users should be able to ask the lead for help without learning a graph editor.

The detector returned three warnings: selected-tab borders in `SessionGroups.tsx:88` and `SessionSidebar.tsx:248`, and the semantic thought-block edge in `ThoughtBlock.tsx:33`. None establishes a defect: selection edges are required by the design system, and the thought cue also has text/icon meaning. No visual overlay was injected, no temporary server was started, and no runtime visual claims follow from this scan.

## Recommended build order and acceptance

| Order | Slice | Evidence required before calling it done |
| --- | --- | --- |
| 1 | Configuration interoperability: F1-F3 | Copy a simple local connector between two differently shaped clients; launch it in the destination. Repeat for an HTTP connector. Copy a complete skill to another client, load it there, then undo. Include profile/repo scopes and shared OpenCode/OpenChamber ownership. |
| 2 | Chat/provider and panel truth: F5-F7 | New chat defaults to the active repo; select an unseen repo; send to the displayed backend. In split view, switch each chat independently. Pin from both entry points at narrow/wide sizes with a visible result. |
| 3 | Native issue-to-change and PR-review acceptance | Right-click a real issue, Fix, observe immediate feedback, steer the run, stop once, resume deliberately, inspect the result, commit, and draft a PR. Review an existing PR, request a correction, and verify branch/source identity throughout. No unintended publish operation. |
| 4 | One complete OpenSpec delivery example | Select the exact task, plan, start helpers, inspect dependencies and usage, resolve a failed helper, review the combined result, and propose/accept the spec update. Editing the source during the run must not silently mark a different task complete. |
| 5 | Supported account quota, then migration | Fetch actual limits for a supported installed provider and display its reset/freshness. Import an OpenChamber chat with provenance and an honest continuation choice. |
| 6 | Goal continuation | Demonstrate several turns toward a task, a verified ending, a budget ending, user Stop, and restart. Show why each turn began; do not equate a fluent final answer with completion. |
| 7 | Plugin compatibility, then remote/mobile delivery | Enumerate what the selected backend actually loads and enforce advertised exclusions. Build remote read/steer/stop against the same session lifecycle, then phone interfaces. |

These are dependency slices, not time estimates. Reuse existing OpenSpec packages and keep closed work closed. Amend acceptance criteria around actual client consumption, not just successful file writes.

### Product success measures

- **Kickoff friction:** number of decisions and copied context fragments between selecting an issue and seeing the agent start. Aim for no re-entered repository/source information.
- **Daily chat efficiency:** locate one chat among 100, jump to an old user message, and switch projects without losing a draft or sending to the wrong session.
- **Interoperability success:** count only configurations that the destination client actually loads; track unsupported conversions separately from errors.
- **Completion quality:** an accepted OpenSpec task has reviewable changes and satisfied criteria. Track user corrections and reopened work; do not claim lower costs from SDD without measuring them.
- **Autonomy quality:** track unnecessary continuation turns, false success, stop latency, and recovery after interruption alongside token/cost totals.
- **Trust in state:** project, backend, source, activity, and panel feedback agree with what the next action actually does.

## Recommended product position

“Your agent workspace, connected to your Git work and your specs.”

Demonstrate that with one strong example: fix an issue, turn an ambiguous requirement into an OpenSpec task, let a lead delegate bounded work, inspect the result, and finish it in the same Git workspace. Pair it with working cross-client setup carryover. That combination can justify switching; another collection of agent controls will not justify it on its own.

Questions deferred: the requested deliverable is this saved product audit. The recommended sequence uses the user's already stated priorities; no implementation or additional scope approval is needed to review it.
