# Tasks

## 1. Sidebar

- [x] 1.1 Specs section with header count badge, one row per active change
      (mono id, thin progress bar, `n/m` or `draft` count)
- [x] 1.2 Row click selects the change app-wide (shared selection store)
- [x] 1.3 Footer line: source path note + "Open Spec Desk" link
- [x] 1.4 Section absent entirely when the repo has no `openspec/`

## 2. Spec card

- [x] 2.1 Card above the commit form: change id, progress ring with percent,
      "X of Y tasks done", next-task preview line
- [x] 2.2 States: next task available / all tasks complete (ready to archive) / draft
- [x] 2.3 Actions: "Copy next-task handoff" primary, "Open Spec Desk" secondary
      (labels change under AI in `add-ai-provider-surface`)
- [x] 2.4 Card reflects the app-wide selected change and updates live on file changes

## 3. Status bar

- [x] 3.1 Segment `openspec · N active` plus CLI presence/version
- [x] 3.2 Segment hidden when the repo has no `openspec/`

## 4. Verify

- [x] 4.1 Tick a task via an external editor: sidebar bar, card ring, and counts move within 1s
- [x] 4.2 Repo without openspec/ shows zero trace of the feature
- [x] 4.3 Manual check in a native window
