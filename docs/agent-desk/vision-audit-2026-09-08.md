# Agent Desk vision audit, September 8, 2026

Re-verified 2026-10-10 against current source. Two of the four findings were
stale in the way this project's Q&A log keeps recording: carried forward as
open after an earlier pass had already settled them. One named the wrong
problem. Verdicts below replace the original table.

| ID | Original finding | Verdict | Where it stands |
| --- | --- | --- | --- |
| V1 | Check outcomes carry no evidence provenance; helper completion trusts agent-reported passed flags. | Named the wrong problem; the real half is fixed. | The described path is real in code but unreachable today: nothing in production emits a `RunStep::Check`, and `completion.rs` refuses an empty check list rather than reading it as a pass. The live defect was on screen, not in the engine: the review panel drew a green tick and "2 passed" over checks only the agent had witnessed. `ResultCheckOutcome` now carries `source: CheckEvidenceSource` and the panel attributes and withholds the tick accordingly. |
| V2 | Account quota remains unavailable for every provider. | Accurate as fact, wrong as a defect. | `plan_limit`/`plan_reset_at` are still `None`, with no provider able to supply them: ACP exposes no quota and GitHub returns entitlement data only to allowlisted OAuth apps. The frontend already handles this correctly, emitting an explicit "not reported" row and never turning unknown into zero. Recorded as a product decision in `release-readiness.md` twice before this. It should stop appearing as an open finding. |
| V3 | OpenChamber import is disabled and detection-only. | Accurate, and understates what was fixed. | Still disabled by flag and detection-only, deliberately. The detection half was materially repaired since: the data directory now resolves the way OpenChamber's own CLI resolves it, replacing a guess that reported "not installed" on machines that had it. The blocker to enabling import is a fixture from a real installation to parse against, not code. |
| V4 | Full native lifecycle acceptance remains unproven. | Accurate, and the largest real gap. | Stop semantics, recovery and the frontend's pure logic are well covered by tests. Streaming, a real turn, and the auditor loop have real-binary tests that are `#[ignore]`d and have never been run. Follow-up turns, an Auto lead's helper graph, the approval gate, and a helper whose dependency fails are reachable only in the built app. This cannot be closed by a code change. |

Confirmed architecture: `useStartAgentSession` delegates create-and-start to `agent_session_start`, preserving durable source identity; `AgentDeskView` mounts per-pane state and result review; result acceptance invokes the OpenSpec return path. These are source traces, not runtime acceptance.

Standing rule this pass reinforces: a finding parked in a list of open work gets argued again by whoever reads the list next, so a settled one is recorded as settled here rather than left to look unfinished. No release-readiness claim follows merely from green unit tests.
