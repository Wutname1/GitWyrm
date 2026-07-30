# Tasks

## 1. Handoffs

- [x] 1.1 Composer: change id, task n of m, read-first file list (proposal, design,
      each delta), rules block (only this task, tick tasks.md, run validate, keep
      unrelated work intact); review-handoff variant when all tasks are done
- [x] 1.2 Rail card: next task title, Copy task handoff, Open in opencode
      (terminal in repo + handoff pasted), Open in VS Code (files + clipboard)
- [x] 1.3 Handoff preview box showing exactly what gets copied
- [x] 1.4 Per-task copy from task-row hover; spec-card copy in the main window uses
      the same composer
- [ ] 1.5 Editable templates in Settings → Specs with {change} {task} {deltas} variables

## 2. Spec check

- [x] 2.1 Run `openspec validate <id>`; render pass (what was checked) and warning
      (what is wrong + what to do) inline in the rail - results are not toast-only
- [x] 2.2 Result persists until the user switches changes; CLI-missing outcome shows
      the install hint

## 3. Archive

- [x] 3.1 Blocked with remaining-count message until every task is done (and until at
      least one delta exists)
- [x] 3.2 Plain-language confirm (what merges where; no type-to-confirm)
- [x] 3.3 On archive: deltas merge into specs/, folder moves to changes/archive/,
      list + counts + selection update in both windows. Built; the merge itself is
      the OpenSpec CLI's job and is exercised by 5.2, which is still open.

## 4. New blank change

- [x] 4.1 Dialog: description + kebab-case folder name (user-chosen, duplicates
      rejected while the field is still editable)
- [x] 4.2 Writes template proposal.md and tasks.md; change appears selected in both windows

## 5. Verify

- [ ] 5.1 Copied handoff pastes cleanly into opencode and produces a scoped task
- [ ] 5.2 Archive a finished change; confirm merge into specs/ and archive count bump
- [ ] 5.3 All of the above with the OpenSpec CLI uninstalled (graceful outcomes)
