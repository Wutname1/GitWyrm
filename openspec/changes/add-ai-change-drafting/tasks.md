# Tasks

## 1. Draft flow

- [x] 1.1 New-change dialog gains "✦ Draft it for me" (primary when AI configured;
      absent otherwise); "Start blank" always present
- [x] 1.2 Name field stays user-owned; collisions auto-unique with a visible suffix
      (matched case-insensitively, so Windows folder rules cannot bite at write time)
- [x] 1.3 Staged drafting view: reads specs library → drafting proposal → breaking
      into tasks → writing deltas, each stage visibly completing; Cancel discards.
      Cancel deliberately does not use FormDialog's `pending`, which blocks Escape;
      an in-flight draft is matched on a run id and dropped if the user left.
- [x] 1.4 Drafting reads the specs library, recent commits, and the description
      (shown to the user as what the AI reads)

## 2. Review before write

- [x] 2.1 Per-artifact cards (Proposal / Tasks / Spec deltas) with content previews
      and Keep/Skip toggles that control creation. The preview is the exact file
      body, not a summary, or "review before write" would not be true.
- [x] 2.2 Live "writes N files" count; Create blocked at zero kept with a hint
- [x] 2.3 Create writes exactly the kept artifacts; Discard leaves no trace.
      Enforced by the skipped artifact simply not being passed to the backend,
      and by a failed create removing the folder rather than leaving a partial
      change. Covered by `creates_only_the_artifacts_it_is_given` and
      `nothing_is_left_behind_when_an_artifact_path_is_unusable`.
- [x] 2.4 Created change appears selected in both windows (`selectChangeEverywhere`
      on create). The "Drafted with <provider>" History line is deliberately NOT
      built: History is git, and git is the right store.

      A stored provenance line would have to survive a gap that is unbounded in
      practice - agents routinely leave drafted work uncommitted for human review,
      and any recorded sha is invalidated by the first rebase or squash. A file
      claiming provenance for a commit that no longer exists is worse than no
      provenance, because the UI would render it confidently.

      There is also no standard to adopt: `Co-authored-by`, `Assisted-by`, and
      `Generated-by` all compete, with no cross-vendor agreement, and OpenSpec's
      own schema (`schemas/spec-driven/`) carries no metadata concept at all.
      Inventing one in a shared `openspec/` folder is not GitWyrm's call to make
      unilaterally.

      What ships instead: `change_history` already logs every commit touching the
      change folder and flags `Assisted-by:` per commit, and the empty state says
      "Nothing committed yet" - which is the honest answer for a change that has
      just been drafted. Commit-level attribution stays `add-spec-commit-links`'
      job. `get_file_blame` already accepts any repo-relative path, so
      per-requirement blame on spec files needs no new backend work if it is
      wanted later.

## 3. Validate-fix loop

- [x] 3.1 Failed spec check (with AI configured) offers "✦ Fix with AI" in the result
- [x] 3.2 Drafts a requirement from that change's proposal; review card with the delta
      preview, Add this delta / Dismiss
- [x] 3.3 Adding writes the delta file, updates the deltas tab and History, and the
      re-run check passes; the fix attaches to the change that was checked even if the
      selection moved meanwhile. The checked change id is captured when the check
      runs and carried through drafting, adding, and the re-check, so none of them
      follow the selection. Adding refuses to overwrite an existing delta
      (`adding_a_delta_never_overwrites_one`).

## 4. Verify

- [ ] 4.1 Draft → skip one artifact → Create: only kept files exist on disk
- [ ] 4.2 Cancel mid-draft and Discard at review: openspec/ untouched both times
- [ ] 4.3 Fix-with-AI on a deltaless change; check passes after Add
