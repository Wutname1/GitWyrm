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

- [ ] 4.1 Commit on a linked branch → chip appears without refresh
- [ ] 4.2 Tick a task → tip chip count updates
- [ ] 4.3 Unlinked branches show no trailer and no chips
