# Tasks

## 1. Linking

- [x] 1.1 Store branch→change links in git config (`branch.<name>.gitwyrm-spec`);
      link/unlink from the branch context menu. The Desk header entry point is
      still open - it lives in files another change owned at the time.
- [x] 1.2 Infer a link when a branch's commits carry `Spec:` trailers and no explicit link exists

## 2. Commit form

- [x] 2.1 Show the `Spec: <id>` trailer under the message box on linked branches,
      marked "added automatically", with a one-click remove for this commit
- [x] 2.2 AI commit-message generation includes the trailer. No generator change
      was needed: the trailer is applied at commit time from the branch link, so
      it is orthogonal to who wrote the message.

## 3. Graph

- [x] 3.1 Spec chip on commit rows whose message has a `Spec:` trailer
- [x] 3.2 Branch-tip chip variant with live `n/m` progress
- [x] 3.3 Chip click opens the Spec Desk at that change
- [x] 3.4 AI marker on commits with an `Assisted-by:` trailer. The graph reads the
      trailer; nothing writes it yet - that belongs to the AI run changes.

## 4. Verify

- [x] 4.1 Commit on a linked branch → chip appears without refresh. Covered by
      `commands::commit` composition tests and a `git::spec_link` test that
      commits a trailered message and resolves it back; the graph reads
      `spec_id` from the same parser in `commands/log.rs`.
- [x] 4.2 Tick a task → tip chip count updates. `openspecToggleTask` invalidates
      `openspecChanges`, which is the query `CommitRow` reads tip progress from,
      so every visible tip chip re-renders on the tick.
- [x] 4.3 Unlinked branches show no trailer and no chips. Composition returns the
      typed message byte-for-byte with no `spec_id` (blank ids included), and a
      branch with no trailer of its own does not inherit its base's link.
