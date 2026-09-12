---
target: Agent Desk first launch and new chat
total_score: 22
max_score: 40
na_heuristics: 
p0_count: 1
p1_count: 1
target_identity: "file:C:\\code\\GitWyrm\\.claude\\worktrees\\agent-desk-openchamber-audit-0f40fb\\src\\components\\domain\\agent-desk\\NewChatLanding.tsx"
target_fingerprint: "sha256:dc6e8e139bd235e0328d7b2ddaa9ec9ab0e8825f4cc88844aa8e7778eadfc843"
target_path: "C:\\code\\GitWyrm\\.claude\\worktrees\\agent-desk-openchamber-audit-0f40fb\\src\\components\\domain\\agent-desk\\NewChatLanding.tsx"
timestamp: 2026-09-11T00-59-33Z
slug: ents-domain-agent-desk-newchatlanding-tsx-d93790b6
---
# Design critique — Agent Desk first launch & new chat

Method: dual-agent (A: design review · B: detector + static measurement)

## Design health: 22/40 — Needs work

| # | Heuristic | Score | Key issue |
|---|---|---|---|
| 1 | Visibility of System Status | 3 | "Opening…" persists over a form that is already interactive |
| 2 | Match System / Real World | 3 | "How much can it do?" asks about authority, which a beginner has no model for |
| 3 | User Control and Freedom | 4 | Nothing gated, nothing auto-lands, all reversible |
| 4 | Consistency and Standards | 2 | Same decisions render as cards AND chips in one viewport |
| 5 | Error Prevention | 4 | Blocked options disabled with real reasons |
| 6 | Recognition Rather Than Recall | 2 | Redundancy defeats recognition: "did I already answer this?" |
| 7 | Flexibility and Efficiency | 1 | No fast path; expert and beginner walk the same road |
| 8 | Aesthetic and Minimalist Design | 1 | 79-97 words and 5 labelled regions before typing |
| 9 | Error Recovery | 4 | Strong absence-vs-failure discipline |
| 10 | Help and Documentation | 2 | The screen IS the documentation — that is the problem |
| **Total** | | **22/40** | Needs work |

## Design specificity verdict

Category-interchangeable. The file's own doc comment argues provenance should come
"first and largest"; at runtime `startedFrom` returns null for manual chats, so on
first launch the differentiator is invisible and three commodity sections take all
the weight. The thesis is in the comments and absent from the pixels.

## Measured facts (Assessment B)

- **34 interactive controls** before the user types a character.
  Title bar 7 · Workspace toolbar 3 · Pane header 3 · Sidebar 6 · Landing 7 · Composer 8.
- **5 duplicated decisions.** Mode, team, and provider each render twice within one
  viewport because the landing is mounted *inside* the composer. Provider appears
  three times — and the title-bar chip is `scope="writing"`, a different scope from
  the other two, so two of three look identical and are not.
- **79 words** of instructional copy on the landing (97 worst case), plus the
  composer's own mode note repeating the selected card.
- **Font sizes clean.** Nothing below `text-2xs` (11px) anywhere on the surface.
- **Detector: 3 warnings, exit 0**, at least one false positive (a tab underline
  matched as an accent border). The detector checks decorative slop; these problems
  are structural.

## The extraction pattern — the through-line

Every string lifted into `agentDeskComposer.ts` (`MODE_NOTES`, `READ_ONLY_REASON`,
`TEAM_NEEDS_MODE_REASON`) stayed correct and shared. Every string left inline drifted:

| State | Names in use |
|---|---|
| solo | "One agent" (landing, composer) · "Solo agent" (popover) |
| helpers | "A team" (landing) · "Lead + helpers" (popover) · "A lead agent, up to 3 helpers" (composer) |

Four strings, two states, all reachable on one screen. The duplication is the defect
generator; file comments record three bugs it has already produced.

## Priority issues

**[P0] The same decisions appear twice on one screen.** Delete the mode and team
cards; the composer chips are the single home. The landing unmounts on first message,
so it teaches a UI the user then loses.

**[P1] The differentiator is invisible.** The repo — the product thesis — is a grey
card indistinguishable from "Which AI?", with a full Windows path as subtitle.

**[P2] Five section labels turn a chat into a form.** Paseo has zero.

**[P3] Four questions asked before the user knows what they are asking about.**
Mode is the hardest concept and the largest widget, demanded first.

**[P4] Chrome outweighs content.** Three bars over an empty pane; Split view, source
bars and Panels are all meaningless with no chat. Sidebar shows three filter controls
above an empty list.

## Accessibility

Above average and consistently reasoned — `aria-disabled` over `disabled` in five
places so refusal reasons stay in the tab order. Two real gaps: **no focus management
when the landing unmounts** (focus falls to body on send), and the landing's card
grids lack the `role="group"` their composer duplicate has.

## The Paseo delta

| | Paseo | Ours |
|---|---|---|
| Decisions before typing | 0 | 4 |
| Section labels | 0 | 4 |
| Words before the input | ~3 | 79-97 |
| Duplicated decisions | 0 | 5 |
| Interactive controls | ~9 | 34 |
| Chrome bars | 1 | 3 |

## Direction chosen by the user

- Full Paseo: heading + project chip + composer. Delete all four sections.
- Repo-aware starter pills, only when real context exists; no pills is acceptable.
- Project picker must allow opening an arbitrary folder (e.g. `c:\code`).
- Mode becomes a dropdown, not a segment — "more and more people only use auto."
