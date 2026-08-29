# Mockup vs spec vs build: gaps and contradictions

Recorded 2026-08-20 from a full cross-reference of the unescaped mockup, all nine
OpenSpec packages, and the shipped code. `docs/` is gitignored, so this file lives
alongside the other Agent Desk docs and is not tracked.

**Status 2026-08-28.** The mockup-vs-spec analysis (G*/C* items) still stands - it is
design reasoning, not a status report. The "load-bearing facts" section was re-verified
against code; item 3 is closed and struck through. The plan-checklist section at the end
is still accurate and still open.

## How to read the mockup at all

`agent-desk-mockup.html` has `<title>Gitwyrm Agent Graph</title>` and keeps the real
markup inside a **srcdoc iframe, entity-escaped**. Raw greps for `ag-thread` return 0
matches and have already misled two agents into declaring the file unrelated. Unescape
first:

    python -c "import io,html; io.open('mock.html','w',encoding='utf-8').write(html.unescape(io.open('docs/agent-desk/agent-desk-mockup.html',encoding='utf-8').read()))"

The unescaped copy returns 67 matches for `ag-thread|ag-composer|ag-source`.

## Spec gaps - the mockup has it, no task covers it

Numbered G1-G30 in the source analysis. The ones that change what we build:

- **G6/G8/G9** Inline plan tables, the purple "Thinking" block, and the indented event
  feed are visual contracts the specs never describe. There is no `MessageKind::Plan`.
- **G10/G11** Graph nodes need a human job title and an agent display name. Neither
  exists in the model, which is why the panel renders "Helper" plus a hex id. G11 also
  asks where names like Sol/Luna/Fable come from - today "Sol" is a hardcoded string.
- **G3** Archive is a top-level destination with a count badge in the mockup; the spec
  only covers per-row archiving, and the code hardcodes `archived: false` in two places.
- **G13/G14** The source drawer's "Done means" acceptance criteria and its
  "Last checked - no source changes" freshness line have no spec home.
- **G20/G21/G22** Setup has four tabs, nine state badges and a Personal/This-repo scope
  column; the spec describes one filtered table with five states.
- **G30** The mockup contains **no keyboard handling at all**, yet three tasks require
  keyboard behavior. Those tasks have no visual reference to build from.

## Contradictions - do not copy these from the mockup

- **C1/C2/C3** Segoe UI, Cascadia Mono, twenty `--ag-*` hex values and `light-dark()`
  pairs. The app is dark-only, IBM Plex Sans, `--gw-*`. Only the *pattern* transfers -
  monospace for counts, timestamps, ids and branches. Note `--ag-purple` (used by the
  thought block) has no `--gw-*` counterpart yet.
- **C4/C5/C8** The mockup's own copy leaks jargon: raw slugs and task indices in the
  source meta line, "worktree" in node meta, and a "Diff" filter label whose own toast
  calls it "changed files". The toast wording is the plain version; prefer it.
- **C6** Many mockup buttons are `[data-toast]` only. The worst is a Setup tab that
  toasts "Connections view selected" **without switching the view** - a lying tab. A
  toast is not an implementation model.
- **C7** "All agents stopped; their work was kept" asserts preservation as fact.
  Preservation is fallible; the shipped panel is already more careful.
- **C9** At 620px the mockup hides the sidebar with no way to reopen it. The shipped
  `SessionSidebar` deliberately diverges with a drawer plus a persistent toggle. Do not
  regress to the mockup here.
- **C10** Its reduced-motion query kills transitions but not the jump keyframe.
- **C11** The detail popover builds content with `innerHTML` from DOM-read text. Never
  copy that pattern for real source titles.

## The load-bearing facts

1. `agentDeskUiStore.ts` and `agentWorkspaceLayout.ts` are complete and tested but had
   **zero consumers**. Workspace-layout groups 2-3 are done; 4-9 are wiring.
2. The graph panel's blocker is the **model**, not the renderer: `ExecutionRecord`
   carries no title, role, agent name, file summary or dependency.
3. ~~`ConversationPane`'s `onOpenSource` prop is passed by nobody~~ **Closed 2026-08-28.**
   It is now passed at `AgentDeskView.tsx:234`, re-enabling the source banner button,
   "View source" and `source`-kind message targets.

## Plan checklist rendering rests on an invented convention (2026-08-20)

`src/lib/agentDeskPlan.ts` renders the mockup's plan table by parsing a Markdown
checklist convention out of a message's `plainContent`:

    - [x] Step text (Owner - status)

Nothing produces that shape. `RunStep::Plan` is `{ text: String }` - free-form prose
(`src-tauri/src/airun/driver.rs:125`) - and `bridge.rs` maps it to `MessageKind::Assistant`
with the text passed straight through. No prompt asks a provider to emit checklists.

So the plan block is real code on a contract nobody honours: today it renders only if a
model spontaneously writes that exact format, and otherwise parses to `[]` and mounts
nothing. That is a safe failure, not a broken one - but it is not a finished feature.

Two honest ways to close it, both out of scope for the pass that built it:
1. Add a structured plan to the wire - a `RunStep::PlanSteps { steps: Vec<PlanStep> }`
   or a real `MessageKind::Plan` - and read fields instead of parsing prose.
2. Ask for the convention in the prompt that drives Plan mode, and treat the parser as
   the documented contract.

Until one of those happens, `agentDeskPlan.ts` is the single place to swap the
derivation. Same caveat applies more weakly to `agentDeskEvents.ts`, which groups
consecutive `tool` messages - that one does rest on real emitted data.
