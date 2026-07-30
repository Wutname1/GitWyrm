# Tasks

## 1. Window

- [x] 1.1 Second Tauri window (label `spec-desk-<repoId>`), opened/focused from the
      main-window entry points; closes with the repo tab
- [x] 1.2 Titlebar: wyrm mark, "Spec Desk · <repo>", Keep on top toggle,
      "Show main window", window controls
- [x] 1.3 Remember size/position per repo; sane default (940×760)

## 2. Changes list

- [x] 2.1 Rows: mono id, "updated <relative time>", status pill
      (In build / Needs review / Draft / Ready), progress bar
- [x] 2.2 Selection synced with the main window (both directions)
- [x] 2.3 Filters: Active / Needs review / Mine, with counts and an empty-filter message
- [x] 2.4 "＋ New" button (opens the new-change flow from later changes)
- [ ] 2.5 Archive link with completed count; opens a searchable archived-changes list

## 3. Status bar

- [x] 3.1 Selected change id + "tasks.md watched · saved instantly"

## 4. Verify

- [x] 4.1 Select in Desk → main-window sidebar and card follow; and the reverse
- [x] 4.2 External tasks.md edit updates both windows within 1s
- [x] 4.3 Keep on top actually floats over the editor; both windows usable simultaneously
- [x] 4.4 Manual check in a native window
