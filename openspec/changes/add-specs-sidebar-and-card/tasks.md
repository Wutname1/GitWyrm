# Tasks

## 1. Sidebar

- [ ] 1.1 Specs section with header count badge, one row per active change
      (mono id, thin progress bar, `n/m` or `draft` count)
- [ ] 1.2 Row click selects the change app-wide (shared selection store)
- [ ] 1.3 Footer line: source path note + "Open Spec Desk" link
- [ ] 1.4 Section absent entirely when the repo has no `openspec/`

## 2. Spec card

- [ ] 2.1 Card above the commit form: change id, progress ring with percent,
      "X of Y tasks done", next-task preview line
- [ ] 2.2 States: next task available / all tasks complete (ready to archive) / draft
- [ ] 2.3 Actions: "Copy next-task handoff" primary, "Open Spec Desk" secondary
      (labels change under AI in `add-ai-provider-surface`)
- [ ] 2.4 Card reflects the app-wide selected change and updates live on file changes

## 3. Status bar

- [ ] 3.1 Segment `openspec · N active` plus CLI presence/version
- [ ] 3.2 Segment hidden when the repo has no `openspec/`

## 4. Verify

- [ ] 4.1 Tick a task via an external editor: sidebar bar, card ring, and counts move within 1s
- [ ] 4.2 Repo without openspec/ shows zero trace of the feature
- [ ] 4.3 Manual check in a native window
