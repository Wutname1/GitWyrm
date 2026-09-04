# Agent Desk Q&A log

One-sentence questions with one-sentence answers, so the decisions taken between sessions
can be reviewed and corrected later. Newest at the bottom. Where a question was settled by
competing sub-agents, the answer names the losing option too.

| # | Date | Question | Answer |
| --- | --- | --- | --- |
| 1 | 2026-09-03 | Is `Some("fetch") => ToolCapability::Read` a hole letting a read-only chat POST to an external service? | No, because `ALWAYS_DENIED_TOOLS` denies `url` at CLI launch for every run and a `--deny-tool` outranks any provider allow rule. |
| 2 | 2026-09-03 | Do the "graph nodes have no job title" and "plan checklist convention is unproduced" gaps in `gaps-and-contradictions.md` still hold? | No, both are closed in code and the doc is stale, which is why the 2026-09-03 audit records them as corrections rather than findings. |
| 3 | 2026-09-03 | DESIGN.md declares a hard 0.6875rem (11px) micro floor but the app breaks it ~138 times -- enforce the rule, or amend it with a named 10px tier? | Neither wholesale: two competing sub-agents converged on doing the risk-free part now (the 34 at-or-above-floor bypasses become `text-2xs`) and deferring the sub-floor question to a decision made in the running app, because whole-app `zoom` genuinely weakens the accessibility case while `AgentGraphPanel`'s fixed node geometry genuinely raises the regression risk. |
| 4 | 2026-09-03 | Does GitWyrm's zoom really scale arbitrary `px` sizes, or only rem-based type? | It scales everything -- `useUiScale.ts:26` sets `document.body.style.zoom`, so a 9px label at 200% renders at 18px, which is why the floor is a consistency question here more than an accessibility one. |
| 5 | 2026-09-03 | Should the scarce Deep Mint accent stay on the result panel's "Fix this" button? | No, it now sits on Keep -- the accent must point at the expected landing action, not at the escalation path. |
| 6 | 2026-09-03 | Is the Split View active pane really marked only by a 10% opacity difference, breaking DESIGN.md's Selected Must Read Rule? | No -- rejected as a false positive: `AgentDeskView.tsx:222` draws a 2px Deep Mint inset edge and sets `aria-current` plus a distinct label, which the reviewer missed by reading the pane component in isolation. |
| 7 | 2026-09-03 | The graph panel's empty state told users "Sol starts helpers" while the composer explicitly refuses to name a lead agent -- which side wins? | The refusal wins: nothing produces the name "Sol", so the empty state now describes the team shape, matching the reasoning already written in `SessionComposer`. |
| 8 | 2026-09-03 | Should a "Revise" button that only toasts "revise it in the chat" keep telling the user to go do the work elsewhere? | No -- it now moves the caret into that chat's own composer and the toast says what to write, so the button performs an action instead of assigning homework. |
