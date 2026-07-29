# Tasks

## 1. Draft flow

- [ ] 1.1 New-change dialog gains "✦ Draft it for me" (primary when AI configured;
      absent otherwise); "Start blank" always present
- [ ] 1.2 Name field stays user-owned; collisions auto-unique with a visible suffix
- [ ] 1.3 Staged drafting view: reads specs library → drafting proposal → breaking
      into tasks → writing deltas, each stage visibly completing; Cancel discards
- [ ] 1.4 Drafting reads the specs library, recent commits, and the description
      (shown to the user as what the AI reads)

## 2. Review before write

- [ ] 2.1 Per-artifact cards (Proposal / Tasks / Spec deltas) with content previews
      and Keep/Skip toggles that control creation
- [ ] 2.2 Live "writes N files" count; Create blocked at zero kept with a hint
- [ ] 2.3 Create writes exactly the kept artifacts; Discard leaves no trace
- [ ] 2.4 Created change appears selected in both windows; History starts with
      "Drafted with <provider> · reviewed by you"

## 3. Validate-fix loop

- [ ] 3.1 Failed spec check (with AI configured) offers "✦ Fix with AI" in the result
- [ ] 3.2 Drafts a requirement from that change's proposal; review card with the delta
      preview, Add this delta / Dismiss
- [ ] 3.3 Adding writes the delta file, updates the deltas tab and History, and the
      re-run check passes; the fix attaches to the change that was checked even if the
      selection moved meanwhile

## 4. Verify

- [ ] 4.1 Draft → skip one artifact → Create: only kept files exist on disk
- [ ] 4.2 Cancel mid-draft and Discard at review: openspec/ untouched both times
- [ ] 4.3 Fix-with-AI on a deltaless change; check passes after Add
